//! Storage integration tests against a real SQLite file.
//!
//! These are not mocks: each test opens an actual database in the temp directory and
//! asserts on what is durably on disk. Anything less would not catch the class of bug
//! this layer exists to prevent (partial writes, lost idempotency).

use crate::history_import::{self, ImportCommit};
use crate::ingest::{self, IngestOutcome};
use crate::projections;
use devchievements_core::{FixedClock, IdGenerator, InjectedContext, TimezoneProvider};
use devchievements_events::{
    CanonicalRepositoryId, EmailOrigin, Event, EventId, EventSource, EventType, Identity,
    LocalDate, SchemaVersion, SourceConfidence,
};
use sqlx::SqlitePool;

/// A deterministic timezone provider for tests: Asia/Taipei semantics, no DST.
#[derive(Debug)]
struct TaiPei;
impl TimezoneProvider for TaiPei {
    fn canonicalize(&self, name: &str) -> Option<String> {
        Some(name.to_string())
    }
    fn local_date(&self, unix_millis: i64, _zone: &str, day_start_minutes: u16) -> Option<String> {
        devchievements_core::local_date_at(
            unix_millis,
            "Asia/Taipei",
            devchievements_core::DayBoundary::from_minutes(day_start_minutes).ok()?,
        )
    }
}

#[derive(Debug, Default)]
struct CountingIds(u64);
impl IdGenerator for CountingIds {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("gen-{}", self.0)
    }
}

#[derive(Debug)]
struct Cfg;
impl devchievements_core::ConfigurationProvider for Cfg {
    fn config_version(&self) -> u64 {
        1
    }
}

/// An identity matching the sample events' author payloads.
fn configured_identity() -> Identity {
    let mut identity = Identity::default();
    identity.add_email("me@example.com", EmailOrigin::UserConfigured);
    identity
}

/// Build the deterministic context pieces.
///
/// Returned by value so an async test can hold them across an `.await` and then
/// borrow them into an `InjectedContext`.
struct Fixture {
    clock: FixedClock,
    tz: TaiPei,
    ids: CountingIds,
    cfg: Cfg,
}

fn fixture() -> Fixture {
    Fixture {
        clock: FixedClock(1_700_000_000_000),
        tz: TaiPei,
        ids: CountingIds::default(),
        cfg: Cfg,
    }
}

impl Fixture {
    fn ctx(&mut self) -> InjectedContext<'_> {
        InjectedContext {
            clock: &self.clock,
            timezone: &self.tz,
            ids: &mut self.ids,
            config: &self.cfg,
        }
    }
}

async fn temp_pool(name: &str) -> SqlitePool {
    let dir = std::env::temp_dir().join(format!("devch-store-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let pool = crate::open(&dir.join("t.db")).await.expect("open");
    ingest::migrate(&pool).await.expect("migrate");
    pool
}

fn commit_event(sha: &str, occurred_at: i64, local_date: &str) -> Event {
    Event {
        event_id: EventId::new(format!("evt-{sha}")).expect("id"),
        event_type: EventType::CommitCreated,
        source: EventSource::LocalGitHook,
        source_confidence: SourceConfidence::VerifiedLocal,
        canonical_repository_id: CanonicalRepositoryId("github:o/r".to_string()),
        external_id: None,
        commit_sha: Some(sha.to_string()),
        occurred_at,
        observed_at: occurred_at,
        received_at: occurred_at,
        local_date: LocalDate::new(local_date).expect("date"),
        timezone: "Asia/Taipei".to_string(),
        date_policy: 0,
        // A real commit event always names its author; the gate depends on it (spec 7).
        payload: "{\"author_email\":\"me@example.com\"}".to_string(),
        schema_version: SchemaVersion(1),
    }
}

#[tokio::test]
async fn no_configured_email_blocks_all_statistics() {
    // Spec 7: statistics and history import must NOT begin without an email.
    // This must hold at the storage layer, which cannot be bypassed.
    let pool = temp_pool("no-identity").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"9".repeat(40), 1_794_787_200_000, "2026-11-16");

    let mut fx = fixture();
    let ctx = fx.ctx();
    let empty = Identity::default();
    let outcome = ingest::ingest_commit(&pool, &event, &facts, 0, &empty, &ctx)
        .await
        .expect("ingest");

    assert_eq!(
        outcome,
        IngestOutcome::Refused(ingest::IngestRefusal::IdentityNotConfigured)
    );
    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        0,
        "nothing written"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        0,
        "no projection may move either"
    );
}

#[tokio::test]
async fn a_commit_by_someone_else_is_refused_not_attributed() {
    // The whole reason the gate exists: an author email that is not the user's must
    // not become "their" commit.
    let pool = temp_pool("not-mine").await;
    let facts = ingest::commit_facts(1, false);
    let mut event = commit_event(&"8".repeat(40), 1_794_787_200_000, "2026-11-16");
    event.payload = "{\"author_email\":\"stranger@else.com\"}".to_string();

    let mut fx = fixture();
    let ctx = fx.ctx();
    let outcome = ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx)
        .await
        .expect("ingest");

    match outcome {
        IngestOutcome::Refused(ingest::IngestRefusal::NotTheUser { author_email }) => {
            assert_eq!(author_email, "stranger@else.com");
        }
        other => panic!("expected NotTheUser, got {other:?}"),
    }
    assert_eq!(ingest::event_count(&pool).await.expect("count"), 0);
}

#[tokio::test]
async fn an_event_without_an_author_is_refused_not_counted() {
    // Spec 7: attribution is mandatory. An event that says nothing about who did
    // it is refused rather than counted, because counting it would fold somebody
    // else's work into the user's statistics.
    let pool = temp_pool("no-author").await;
    let facts = ingest::commit_facts(1, false);
    let mut event = commit_event(&"7".repeat(40), 1_794_787_200_000, "2026-11-16");
    event.payload = "{}".to_string();

    let mut fx = fixture();
    let ctx = fx.ctx();
    let outcome = ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx)
        .await
        .expect("ingest");

    assert!(
        matches!(outcome, IngestOutcome::Refused(_)),
        "an authorless event must be refused, got {outcome:?}"
    );
    assert_eq!(ingest::event_count(&pool).await.expect("count"), 0);
}

#[tokio::test]
async fn schema_records_its_version() {
    let pool = temp_pool("version").await;
    let v = ingest::recorded_schema_version(&pool)
        .await
        .expect("read version");
    assert_eq!(v, Some(crate::schema::SCHEMA_VERSION));
}

#[tokio::test]
async fn ingesting_a_commit_writes_event_projection_and_outbox_together() {
    let pool = temp_pool("ingest").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"a".repeat(40), 1_794_787_200_000, "2026-11-16");

    let mut fx = fixture();
    let ctx = fx.ctx();
    let outcome = ingest::ingest_commit(
        &pool,
        &event,
        &facts,
        1_794_787_200_000,
        &configured_identity(),
        &ctx,
    )
    .await;

    match outcome.expect("ingest") {
        IngestOutcome::Ingested {
            queued_notifications,
        } => {
            assert!(
                !queued_notifications.is_empty(),
                "a counting commit queues feedback"
            );
        }
        other => panic!("expected Ingested, got {other:?}"),
    }

    assert_eq!(ingest::event_count(&pool).await.expect("count"), 1);
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("repo count"),
        1
    );
    let daily = projections::daily_projection(&pool, "github:o/r", "2026-11-16")
        .await
        .expect("daily")
        .expect("row exists");
    assert_eq!(daily.commit_count, 1);
    assert_eq!(
        daily.timezone, "Asia/Taipei",
        "the policy is recorded with the row"
    );

    let counts = ingest::outbox_counts(&pool).await.expect("outbox");
    assert!(counts.iter().any(|(s, n)| s == "created" && *n > 0));
}

#[tokio::test]
async fn re_ingesting_the_same_event_is_a_no_op() {
    // Spec 5.3: event_id is the idempotency key, so at-least-once delivery is safe.
    let pool = temp_pool("dup").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"b".repeat(40), 1_794_787_200_000, "2026-11-16");

    {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("first");
    let before = ingest::event_count(&pool).await.expect("count");

    let second = {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("second");

    assert_eq!(second, IngestOutcome::Duplicate);
    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        before,
        "no new row"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("repo count"),
        1,
        "the projection must not double-count a redelivered event"
    );
}

#[tokio::test]
async fn an_empty_commit_is_stored_but_not_counted() {
    // Spec 8.1: empty commits are excluded from ordinary statistics.
    let pool = temp_pool("empty").await;
    let facts = ingest::commit_facts(1, true); // tree == parent tree
    let event = commit_event(&"c".repeat(40), 1_794_787_200_000, "2026-11-16");

    let outcome = {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("ingest");

    assert_eq!(outcome, IngestOutcome::IngestedSilently);
    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        1,
        "still recorded"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("repo count"),
        0,
        "but it does not count"
    );
}

#[tokio::test]
async fn a_merge_commit_is_stored_but_not_counted() {
    let pool = temp_pool("merge").await;
    let facts = ingest::commit_facts(2, false);
    let event = commit_event(&"d".repeat(40), 1_794_787_200_000, "2026-11-16");
    let outcome = {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("ingest");
    assert_eq!(outcome, IngestOutcome::IngestedSilently);
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        0
    );
}

#[tokio::test]
async fn an_invalid_event_is_rejected_without_writing_anything() {
    let pool = temp_pool("reject").await;
    let facts = ingest::commit_facts(1, false);
    let mut event = commit_event(&"e".repeat(40), 1_794_787_200_000, "2026-11-16");
    event.commit_sha = None; // required for a commit event

    let outcome = {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("ingest");

    assert!(matches!(outcome, IngestOutcome::Rejected(_)));
    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        0,
        "nothing written"
    );
}

#[tokio::test]
async fn achievements_are_unlocked_inside_the_same_transaction() {
    let pool = temp_pool("ach").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"f".repeat(40), 1_794_787_200_000, "2026-11-16");
    {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
    }
    .expect("ingest");

    let unlocked = projections::unlocked_achievement_ids(&pool)
        .await
        .expect("unlocks");
    assert_eq!(unlocked, vec!["first_commit".to_string()]);

    // And the outbox row exists for it, in the same commit.
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT notification_id FROM outbox WHERE kind = 'achievement_unlocked'")
            .fetch_all(&pool)
            .await
            .expect("outbox");
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn projections_can_be_rebuilt_from_the_event_log() {
    // This property is what makes storing projections safe: they are derived.
    let pool = temp_pool("rebuild").await;
    let facts = ingest::commit_facts(1, false);
    for (i, sha) in ["11", "22", "33"].iter().enumerate() {
        let event = commit_event(&sha.repeat(20), 1_794_787_200_000 + i as i64, "2026-11-16");
        {
            let mut fx = fixture();
            let ctx = fx.ctx();
            ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
        }
        .expect("ingest");
    }

    let before = projections::repository_commit_count(&pool, "github:o/r")
        .await
        .expect("count");
    assert_eq!(before, 3);

    let replayed = projections::rebuild_from_events(&pool, 9_999)
        .await
        .expect("rebuild");
    assert_eq!(replayed, 3);

    let after = projections::repository_commit_count(&pool, "github:o/r")
        .await
        .expect("count");
    assert_eq!(after, before, "a rebuild must reproduce the same numbers");
}

#[tokio::test]
async fn history_import_uses_the_commit_time_not_the_import_time() {
    // Spec 11.2: importing must not dump everything onto today.
    let commit_time = 1_700_000_000_000; // 2023-11-14 in UTC
    let commit = ImportCommit {
        sha: "9".repeat(40),
        author_email: "me@example.com".to_string(),
        committed_at_ms: commit_time,
        parent_count: 1,
        tree_equals_parent: false,
    };

    let mut fx = fixture();
    let ctx = fx.ctx();
    let event = history_import::build_import_event("github:o/r", &commit, "Asia/Taipei", 0, &ctx)
        .expect("event built");

    assert_eq!(event.occurred_at, commit_time);
    // 1700000000000 is 2023-11-14T22:13:20Z, which in Asia/Taipei (UTC+8) is
    // 2023-11-15T06:13:20. Taking the UTC date here would be wrong by one day --
    // exactly the class of bug spec 12 exists to prevent, so the assertion pins the
    // zone-corrected value.
    assert_eq!(
        event.local_date.as_str(),
        "2023-11-15",
        "local_date must be derived in the configured zone, not from the UTC date"
    );
    assert_eq!(event.source_confidence, SourceConfidence::Imported);
    assert_eq!(event.source, EventSource::HistoryImport);
}

#[tokio::test]
async fn history_import_is_idempotent_and_resumable() {
    let pool = temp_pool("import").await;
    let batch = "batch-1";

    let already = history_import::begin_batch(&pool, batch, "github:o/r", 1_000)
        .await
        .expect("begin");
    assert!(!already, "first call starts the batch");

    history_import::finish_batch(
        &pool,
        batch,
        &history_import::BatchOutcome {
            imported: 10,
            ..Default::default()
        },
        2_000,
    )
    .await
    .expect("finish");

    let state = history_import::batch_state(&pool, batch)
        .await
        .expect("state")
        .expect("exists");
    assert_eq!(state.status, "completed");
    assert_eq!(state.imported_events, 10);

    // Resuming a completed batch is a no-op, which makes "import again" safe.
    let already = history_import::begin_batch(&pool, batch, "github:o/r", 3_000)
        .await
        .expect("begin again");
    assert!(already, "a completed batch must not restart");
}

#[tokio::test]
async fn author_matching_is_case_insensitive_and_list_based() {
    let identities = vec!["Me@Example.com".to_string(), "other@x.y".to_string()];
    assert!(history_import::author_matches(
        "me@example.com",
        &identities
    ));
    assert!(history_import::author_matches(
        "ME@EXAMPLE.COM",
        &identities
    ));
    assert!(!history_import::author_matches(
        "someone@else.com",
        &identities
    ));
    assert!(!history_import::author_matches("", &identities));
}

#[tokio::test]
async fn data_survives_closing_and_reopening_the_database() {
    let dir = std::env::temp_dir().join(format!("devch-store-reopen-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let path = dir.join("persist.db");

    {
        let pool = crate::open(&path).await.expect("open");
        ingest::migrate(&pool).await.expect("migrate");
        let facts = ingest::commit_facts(1, false);
        let event = commit_event(&"7".repeat(40), 1_794_787_200_000, "2026-11-16");
        {
            let mut fx = fixture();
            let ctx = fx.ctx();
            ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &ctx).await
        }
        .expect("ingest");
        pool.close().await;
    }

    let pool = crate::open(&path).await.expect("reopen");
    assert_eq!(ingest::event_count(&pool).await.expect("count"), 1);
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        1
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Spec 22 end to end: a user's own rule must unlock on the commit that satisfies it.
///
/// This is the test that would have caught the product shipping a Builder whose
/// achievements could be created, stored, and never unlocked: nothing above the
/// storage layer reads the rules, so the wiring has to be proven here.
#[tokio::test]
async fn a_user_rule_unlocks_at_its_threshold_and_only_once() {
    use devchievements_core::achievements::builder::{Condition, RuleExpr, UserMetric};
    use devchievements_core::achievements::definition::UserAchievement;

    let rule = UserAchievement {
        id: "three-commits".to_string(),
        name: "Three Commits".to_string(),
        description: "Reach three commits".to_string(),
        rule: RuleExpr::Condition(Condition {
            metric: UserMetric::Commits,
            at_least: 3,
        }),
        version: 1,
    };
    let rules = [rule];

    let pool = temp_pool("user-rule").await;
    let facts = ingest::commit_facts(1, false);
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = configured_identity();

    for (index, sha_byte) in ["c", "d"].iter().enumerate() {
        let event = commit_event(&sha_byte.repeat(40), 1_794_787_200_000, "2026-11-16");
        let outcome =
            ingest::ingest_commit_with_rules(&pool, &event, &facts, 0, &identity, &ctx, &rules)
                .await
                .expect("ingest");
        assert!(matches!(outcome, IngestOutcome::Ingested { .. }));
        assert!(
            !projections::unlocked_achievement_ids(&pool)
                .await
                .expect("ids")
                .contains(&"three-commits".to_string()),
            "unlocked after only {} commits",
            index + 1
        );
    }

    let third = commit_event(&"e".repeat(40), 1_794_787_200_000, "2026-11-16");
    ingest::ingest_commit_with_rules(&pool, &third, &facts, 0, &identity, &ctx, &rules)
        .await
        .expect("ingest");
    assert!(
        projections::unlocked_achievement_ids(&pool)
            .await
            .expect("ids")
            .contains(&"three-commits".to_string()),
        "the third commit must unlock the user rule"
    );

    // A fourth commit must not queue a second celebration for the same achievement.
    let fourth = commit_event(&"f".repeat(40), 1_794_787_200_000, "2026-11-16");
    ingest::ingest_commit_with_rules(&pool, &fourth, &facts, 0, &identity, &ctx, &rules)
        .await
        .expect("ingest");
    let queued: Vec<(String,)> =
        sqlx::query_as("SELECT payload FROM outbox WHERE kind = 'achievement_unlocked'")
            .fetch_all(&pool)
            .await
            .expect("outbox");
    let celebrations = queued
        .iter()
        .filter(|(payload,)| payload.contains("three-commits"))
        .count();
    assert_eq!(celebrations, 1, "one unlock, one celebration");
}

// ----------------------------------------------------------- browser activities

/// An identity that owns a GitHub login, which is how a browser report is
/// attributed (spec 7: the login maps to the configured emails).
fn identity_with_login() -> Identity {
    let mut identity = configured_identity();
    identity.add_login_mapping("me", vec!["me@example.com".to_string()]);
    identity
}

/// A browser report as the Native Messaging host produces it.
///
/// A page never reveals an author email, so attribution has to go through the
/// login mapping rather than through the payload's author email.
fn browser_event(
    event_type: EventType,
    external_id: &str,
    occurred_at: i64,
    local_date: &str,
) -> Event {
    Event {
        event_id: EventId::new(format!(
            "browser:github:o/r#{}:{external_id}",
            event_type.as_str()
        ))
        .expect("id"),
        event_type,
        source: EventSource::BrowserExtension,
        source_confidence: SourceConfidence::BrowserConfirmed,
        canonical_repository_id: CanonicalRepositoryId("github:o/r".to_string()),
        external_id: Some(external_id.to_string()),
        commit_sha: None,
        occurred_at,
        observed_at: occurred_at,
        received_at: occurred_at,
        local_date: LocalDate::new(local_date).expect("date"),
        timezone: "Asia/Taipei".to_string(),
        date_policy: 0,
        payload: "{\"author_login\":\"me\",\"kind\":\"browser_github\"}".to_string(),
        schema_version: SchemaVersion(1),
    }
}

/// Facts for an event that is not a commit; the engine must not count it.
fn not_a_commit() -> devchievements_core::CommitFacts {
    devchievements_core::CommitFacts {
        counts_as_commit: false,
        is_merge: false,
        is_empty: false,
    }
}

#[tokio::test]
async fn a_merged_pull_request_counts_and_unlocks_without_being_a_commit() {
    // Spec 2.3 and 16.4: a merged PR is its own statistic, unlocks its own
    // achievement, and must never be counted as a commit.
    let pool = temp_pool("browser-pr-merged").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();
    let event = browser_event(
        EventType::PullRequestMerged,
        "pr:1",
        1_794_787_200_000,
        "2026-11-16",
    );

    let outcome =
        ingest::ingest_commit_with_rules(&pool, &event, &not_a_commit(), 0, &identity, &ctx, &[])
            .await
            .expect("ingest");
    assert!(
        matches!(outcome, IngestOutcome::Ingested { .. }),
        "{outcome:?}"
    );

    let totals = projections::browser_totals(&pool).await.expect("totals");
    assert_eq!(totals.pull_request_merged_count, 1);
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        0,
        "a merged pull request must not move the commit counter"
    );
    assert!(
        projections::unlocked_achievement_ids(&pool)
            .await
            .expect("unlocked")
            .contains(&"first_pr_merged".to_string()),
        "a merged pull request must unlock first_pr_merged"
    );

    let queued: Vec<(String,)> = sqlx::query_as("SELECT kind FROM outbox")
        .fetch_all(&pool)
        .await
        .expect("outbox");
    assert_eq!(queued.len(), 1, "one unlock, one celebration");
    assert_eq!(queued[0].0, "achievement_unlocked");
}

#[tokio::test]
async fn an_opened_pull_request_is_recorded_without_counting_anything() {
    // Nothing in the rule set observes an opened pull request, so it is stored and
    // stays silent rather than inventing a counter nothing reads.
    let pool = temp_pool("browser-pr-opened").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();
    let event = browser_event(
        EventType::PullRequestOpened,
        "pr:2",
        1_794_787_200_000,
        "2026-11-16",
    );

    let outcome =
        ingest::ingest_commit_with_rules(&pool, &event, &not_a_commit(), 0, &identity, &ctx, &[])
            .await
            .expect("ingest");
    assert!(
        matches!(outcome, IngestOutcome::IngestedSilently),
        "{outcome:?}"
    );

    assert_eq!(
        projections::browser_totals(&pool).await.expect("totals"),
        projections::BrowserTotals::default()
    );
    assert_eq!(ingest::event_count(&pool).await.expect("events"), 1);
    let queued: Vec<(String,)> = sqlx::query_as("SELECT kind FROM outbox")
        .fetch_all(&pool)
        .await
        .expect("outbox");
    assert!(queued.is_empty(), "nothing to celebrate yet");
}

#[tokio::test]
async fn a_browser_only_repository_exists_with_zero_commits() {
    // Spec 24.1 lists repositories, and a repository known only from the browser is
    // still one of them. Zero commits is a number, not a missing row.
    let pool = temp_pool("browser-only-repo").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();
    let event = browser_event(
        EventType::ReleasePublished,
        "release:1",
        1_794_787_200_000,
        "2026-11-16",
    );

    ingest::ingest_commit_with_rules(&pool, &event, &not_a_commit(), 0, &identity, &ctx, &[])
        .await
        .expect("ingest");

    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT commit_count, release_count FROM repository_projection
         WHERE canonical_repository_id = 'github:o/r'",
    )
    .fetch_optional(&pool)
    .await
    .expect("row");
    assert_eq!(row, Some((0, 1)));
}

#[tokio::test]
async fn a_refused_browser_report_is_parked_and_can_be_replayed() {
    // Spec 7 refuses the report; nothing says the report should be deleted. Keeping it
    // is what makes "link your account" a one-click repair instead of lost history.
    let pool = temp_pool("refusals").await;
    let payload = "{\"kind\":\"browser_github\",\"author_login\":\"Applepinea\"}";

    crate::refusals::park(
        &pool,
        "evt-1",
        "Applepinea",
        "github:Applepinea/w",
        payload,
        10,
    )
    .await
    .expect("park");
    // The same record arriving twice must not double count.
    crate::refusals::park(
        &pool,
        "evt-1",
        "Applepinea",
        "github:Applepinea/w",
        payload,
        11,
    )
    .await
    .expect("park again");

    let unlinked = crate::refusals::unlinked_logins(&pool).await.expect("list");
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0].login, "Applepinea");
    assert_eq!(unlinked[0].reports, 1, "one report, not two");

    let parked = crate::refusals::parked_for_login(&pool, "Applepinea")
        .await
        .expect("for login");
    assert_eq!(parked.len(), 1);
    assert_eq!(
        parked[0].payload, payload,
        "the payload must survive verbatim"
    );

    let removed = crate::refusals::forget(&pool, &["evt-1".to_string()])
        .await
        .expect("forget");
    assert_eq!(removed, 1);
    assert_eq!(
        crate::refusals::parked_count(&pool).await.expect("count"),
        0
    );
}

#[tokio::test]
async fn a_quiet_policy_records_the_unlock_without_queueing_a_notification() {
    // Spec 16.3: an event older than a day updates statistics silently. The unlock is
    // still real, and it is NOT relabelled as an import -- the event just happened to
    // the user's account, we simply learned about it late.
    let pool = temp_pool("quiet-policy").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();
    let event = browser_event(
        EventType::PullRequestMerged,
        "pr:3",
        1_794_787_200_000,
        "2026-11-16",
    );

    let outcome = ingest::ingest_event_with_policy(
        &pool,
        &event,
        &not_a_commit(),
        0,
        &identity,
        &ctx,
        ingest::NotificationPolicy::Quiet,
        &[],
    )
    .await
    .expect("ingest");
    assert!(
        matches!(outcome, IngestOutcome::IngestedSilently),
        "{outcome:?}"
    );

    let unlocked: Vec<(String, String)> =
        sqlx::query_as("SELECT achievement_id, unlock_source FROM unlocked_achievements")
            .fetch_all(&pool)
            .await
            .expect("unlocked");
    assert_eq!(
        unlocked,
        vec![("first_pr_merged".to_string(), "realtime".to_string())]
    );

    let queued: Vec<(String,)> = sqlx::query_as("SELECT notification_id FROM outbox")
        .fetch_all(&pool)
        .await
        .expect("outbox");
    assert!(
        queued.is_empty(),
        "a row in the outbox is delivered, so a quiet event must not create one"
    );
}

/// A commit event for a named repository, so a test can span several of them.
fn commit_event_in(repository_id: &str, sha: &str, local_date: &str) -> Event {
    let mut event = commit_event(sha, 1_794_787_200_000, local_date);
    event.canonical_repository_id = CanonicalRepositoryId(repository_id.to_string());
    event.event_id = EventId::new(format!("evt-{repository_id}-{sha}")).expect("id");
    event
}

#[tokio::test]
async fn upgrading_an_old_database_backs_up_first_and_keeps_every_unlock() {
    // AGENTS.md 11: an irreversible migration needs a backup, and spec 13.4: an
    // unlock is never revoked. This test builds a version-3 database by hand (the
    // shape this build is migrating FROM), then opens it the way the app does.
    let dir = std::env::temp_dir().join(format!("devch-migrate-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("dir");
    let pool = crate::open(&dir.join("companion.db")).await.expect("open");

    // A v3 database: the per-repository tables plus an unlock and a day of history.
    sqlx::raw_sql(
        "CREATE TABLE schema_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
         INSERT INTO schema_meta (key, value) VALUES ('schema_version', '3');
         CREATE TABLE events (
             event_id TEXT PRIMARY KEY NOT NULL, event_type TEXT NOT NULL,
             source TEXT NOT NULL, source_confidence TEXT NOT NULL,
             canonical_repository_id TEXT NOT NULL, external_id TEXT, commit_sha TEXT,
             occurred_at INTEGER NOT NULL, observed_at INTEGER NOT NULL,
             received_at INTEGER NOT NULL, local_date TEXT NOT NULL,
             timezone TEXT NOT NULL, date_policy INTEGER NOT NULL,
             payload TEXT NOT NULL, schema_version INTEGER NOT NULL, ingest_batch TEXT);
         CREATE TABLE repository_projection (
             canonical_repository_id TEXT PRIMARY KEY NOT NULL, commit_count INTEGER NOT NULL DEFAULT 0,
             pull_request_merged_count INTEGER NOT NULL DEFAULT 0, release_count INTEGER NOT NULL DEFAULT 0,
             issue_opened_count INTEGER NOT NULL DEFAULT 0,
             repositories_created_count INTEGER NOT NULL DEFAULT 0,
             star_given_count INTEGER NOT NULL DEFAULT 0,
             first_seen_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
         CREATE TABLE daily_projection (
             canonical_repository_id TEXT NOT NULL, local_date TEXT NOT NULL,
             commit_count INTEGER NOT NULL DEFAULT 0, daily_rank TEXT,
             timezone TEXT NOT NULL, date_policy INTEGER NOT NULL, updated_at INTEGER NOT NULL,
             PRIMARY KEY (canonical_repository_id, local_date));
         CREATE TABLE unlocked_achievements (
             achievement_id TEXT PRIMARY KEY NOT NULL, unlocked_at INTEGER NOT NULL,
             unlock_source TEXT NOT NULL, rule_version INTEGER NOT NULL,
             viewed_at INTEGER, triggering_event_id TEXT);
         INSERT INTO repository_projection
             (canonical_repository_id, commit_count, first_seen_at, updated_at)
             VALUES ('github:o/a', 3, 0, 0), ('github:o/b', 2, 0, 0);
         INSERT INTO daily_projection
             (canonical_repository_id, local_date, commit_count, daily_rank, timezone, date_policy, updated_at)
             VALUES ('github:o/a', '2026-11-16', 3, 'uncommon', 'Asia/Taipei', 0, 0),
                    ('github:o/b', '2026-11-16', 2, 'uncommon', 'Asia/Taipei', 0, 0);
         INSERT INTO unlocked_achievements
             (achievement_id, unlocked_at, unlock_source, rule_version)
             VALUES ('first_commit', 1, 'realtime', 1);",
    )
    .execute(&pool)
    .await
    .expect("seed v3");

    ingest::migrate(&pool).await.expect("migrate");

    // The backup is written by `migrate` itself, into a `backups/` directory next to
    // the database file. Asserting on the directory rather than on a returned path is
    // deliberate: the guarantee is "every caller gets a backup", not "the callers that
    // remembered to ask for one do".
    let backups: Vec<std::path::PathBuf> = std::fs::read_dir(dir.join("backups"))
        .expect("a backups directory must exist after a schema change")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(backups.len(), 1, "exactly one backup, got {backups:?}");
    let backup = &backups[0];
    assert!(backup.exists(), "the backup must be a real file");

    // The backup was taken BEFORE the migration, so it still says version 3.
    let restored = crate::open(backup).await.expect("open backup");
    assert_eq!(
        ingest::recorded_schema_version(&restored)
            .await
            .expect("version"),
        Some(3),
        "the backup must be the pre-migration state"
    );
    restored.close().await;

    // The migration itself must have aggregated the two repositories into one day.
    assert_eq!(
        ingest::recorded_schema_version(&pool)
            .await
            .expect("version"),
        Some(crate::schema::SCHEMA_VERSION)
    );
    let day = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("global day")
        .expect("the backfill must have created the row");
    assert_eq!(day.commit_count, 5, "3 + 2 commits on one day is 5");
    assert_eq!(
        day.daily_rank,
        Some(devchievements_daily_rank::DailyRank::Rare),
        "the rank is recomputed from the sum, so 5 is Rare even though both rows said Uncommon"
    );

    // Spec 13.4: nothing is revoked.
    let unlocked = projections::unlocked_achievement_ids(&pool)
        .await
        .expect("unlocked");
    assert_eq!(unlocked, vec!["first_commit".to_string()]);

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_repeatable_achievement_records_every_commit_independently() {
    // The central contract: a repeatable achievement is one row per trigger, while a
    // permanent one stays one row per achievement. Both live in this transaction.
    let pool = temp_pool("repeatable").await;
    let facts = ingest::commit_facts(1, false);
    let identity = configured_identity();

    for n in 0..2 {
        let sha = format!("{:040x}", n + 1);
        let event = commit_event(&sha, 1_794_787_200_000 + n, "2026-11-16");
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
            .await
            .expect("ingest");
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT achievement_id, triggering_event_id FROM repeatable_achievement_events
         ORDER BY id ASC",
    )
    .fetch_all(&pool)
    .await
    .expect("repeatable rows");
    assert_eq!(rows.len(), 2, "two commits are two independent triggers");
    assert!(rows.iter().all(|(id, _)| id == "commit_complete"));
    assert_ne!(rows[0].1, rows[1].1, "each trigger names its own event");

    let permanent: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM unlocked_achievements WHERE achievement_id = 'commit_complete'",
    )
    .fetch_one(&pool)
    .await
    .expect("permanent");
    assert_eq!(
        permanent.0, 0,
        "a repeatable must not occupy the permanent table"
    );

    let notifications: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE kind = 'achievement_triggered'")
            .fetch_one(&pool)
            .await
            .expect("outbox");
    assert_eq!(
        notifications.0, 2,
        "each trigger gets its own notification row"
    );

    // The read model derives its count from the same rows the UI lists.
    let entries = crate::read_models::repeatable_achievements(&pool, |id| id.to_string())
        .await
        .expect("read model");
    let entry = entries
        .iter()
        .find(|e| e.achievement_id == "commit_complete")
        .expect("commit_complete entry");
    assert_eq!(entry.trigger_count, 2);
    assert_eq!(entry.events.len(), 2);
}

#[tokio::test]
async fn replaying_one_commit_cannot_duplicate_its_repeatable_record() {
    // Database-level idempotency, not an in-memory check: the UNIQUE key refuses the
    // second pair even when the caller never looked first.
    let pool = temp_pool("repeatable-unique").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"a".repeat(40), 1_794_787_200_000, "2026-11-16");
    let identity = configured_identity();

    {
        let mut fx = fixture();
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
            .await
            .expect("first");
    }
    let before = ingest::outbox_counts(&pool).await.expect("outbox");

    // The same event id is the ingestion idempotency key (spec 5.3).
    {
        let mut fx = fixture();
        let ctx = fx.ctx();
        let replay = ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
            .await
            .expect("replay");
        assert_eq!(replay, IngestOutcome::Duplicate);
    }

    // And the constraint itself holds even if a future caller bypasses that guard.
    let direct = sqlx::query(
        "INSERT OR IGNORE INTO repeatable_achievement_events
            (achievement_id, triggering_event_id, unlocked_at, rule_version,
             canonical_repository_id)
         VALUES ('commit_complete', ?, 1, 1, 'github:o/r')",
    )
    .bind(event.event_id.as_str())
    .execute(&pool)
    .await
    .expect("direct insert");
    assert_eq!(direct.rows_affected(), 0, "the unique pair must refuse it");

    let rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repeatable_achievement_events")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows.0, 1, "a replay must not produce a second trigger");
    assert_eq!(
        ingest::outbox_counts(&pool).await.expect("outbox"),
        before,
        "and it must not manufacture a second notification"
    );
}

#[tokio::test]
async fn two_concurrent_deliveries_of_one_event_record_it_once() {
    // At-least-once delivery is the norm. Both calls may reach the insert; whichever
    // loses either sees Duplicate or a constraint error, but the user must still get
    // exactly one trigger and one notification.
    let pool = temp_pool("repeatable-race").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"b".repeat(40), 1_794_787_200_000, "2026-11-16");

    let mut left_fx = fixture();
    let mut right_fx = fixture();
    let left_event = event.clone();
    let right_event = event;
    let left_facts = facts;
    let right_facts = facts;
    let left_identity = configured_identity();
    let right_identity = configured_identity();
    let left_ctx = left_fx.ctx();
    let right_ctx = right_fx.ctx();
    let left = ingest::ingest_commit(
        &pool,
        &left_event,
        &left_facts,
        0,
        &left_identity,
        &left_ctx,
    );
    let right = ingest::ingest_commit(
        &pool,
        &right_event,
        &right_facts,
        0,
        &right_identity,
        &right_ctx,
    );
    let (first, second) = tokio::join!(left, right);
    // The requirement is about what is durable, not about which caller notices the
    // duplicate. Both may return Ingested under SQLite's snapshot semantics, so the
    // assertions below are the ones that matter: one event, one trigger, one
    // notification. A caller returning Duplicate or a transient busy error is also
    // acceptable; the retry path then sees the committed row.
    let first = format!("{first:?}");
    let second = format!("{second:?}");

    let triggers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repeatable_achievement_events")
        .fetch_one(&pool)
        .await
        .expect("triggers");
    assert_eq!(
        triggers.0, 1,
        "one event is one trigger, race or no race; results: first={first}, second={second}"
    );
    let notifications: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE kind = 'achievement_triggered'")
            .fetch_one(&pool)
            .await
            .expect("notifications");
    assert_eq!(notifications.0, 1);
    assert_eq!(ingest::event_count(&pool).await.expect("events"), 1);
}

#[tokio::test]
async fn an_imported_commit_records_its_trigger_without_a_notification() {
    // Spec 11.2: history is silent. Silence means no outbox row, not "not recorded".
    let pool = temp_pool("repeatable-import").await;
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"c".repeat(40), 1_794_787_200_000, "2026-11-16");
    let mut fx = fixture();
    let ctx = fx.ctx();
    let outcome = ingest::ingest_import(&pool, &event, &facts, 0, &configured_identity(), &ctx)
        .await
        .expect("import");
    assert_eq!(outcome, IngestOutcome::IngestedSilently);

    let triggers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repeatable_achievement_events")
        .fetch_one(&pool)
        .await
        .expect("triggers");
    assert_eq!(triggers.0, 1, "the trigger is still a fact worth showing");

    let pending: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM outbox WHERE kind = 'achievement_triggered'")
            .fetch_one(&pool)
            .await
            .expect("outbox");
    assert_eq!(pending.0, 0, "an import must not announce the trigger");

    // The daily rank is its own table and was never involved in this record.
    let rank_rows: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM daily_milestones WHERE milestone_id = 'commit_complete'",
    )
    .fetch_one(&pool)
    .await
    .expect("milestones");
    assert_eq!(
        rank_rows.0, 0,
        "a repeatable achievement is not a daily milestone"
    );
}

#[tokio::test]
async fn upgrading_v4_to_v5_keeps_the_permanent_unlocks_and_adds_the_trigger_table() {
    let dir = std::env::temp_dir().join(format!("devch-v4-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("dir");
    let pool = crate::open(&dir.join("companion.db")).await.expect("open");

    // The v4 shape: schema_meta, an existing permanent unlock, and no repeatable table.
    sqlx::raw_sql(
        "CREATE TABLE schema_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
         INSERT INTO schema_meta (key, value) VALUES ('schema_version', '4');
         CREATE TABLE events (event_id TEXT PRIMARY KEY NOT NULL, event_type TEXT NOT NULL,
             source TEXT NOT NULL, source_confidence TEXT NOT NULL,
             canonical_repository_id TEXT NOT NULL, external_id TEXT, commit_sha TEXT,
             occurred_at INTEGER NOT NULL, observed_at INTEGER NOT NULL,
             received_at INTEGER NOT NULL, local_date TEXT NOT NULL,
             timezone TEXT NOT NULL, date_policy INTEGER NOT NULL,
             payload TEXT NOT NULL, schema_version INTEGER NOT NULL, ingest_batch TEXT);
         CREATE TABLE unlocked_achievements (
             achievement_id TEXT PRIMARY KEY NOT NULL, unlocked_at INTEGER NOT NULL,
             unlock_source TEXT NOT NULL, rule_version INTEGER NOT NULL,
             viewed_at INTEGER, triggering_event_id TEXT);
         INSERT INTO unlocked_achievements
             (achievement_id, unlocked_at, unlock_source, rule_version)
             VALUES ('first_commit', 10, 'realtime', 1);",
    )
    .execute(&pool)
    .await
    .expect("seed v4");

    ingest::migrate(&pool).await.expect("migrate");
    assert_eq!(
        ingest::recorded_schema_version(&pool)
            .await
            .expect("version"),
        Some(5)
    );
    let unlocked = projections::unlocked_achievement_ids(&pool)
        .await
        .expect("unlocked");
    assert_eq!(unlocked, vec!["first_commit".to_string()]);

    // Running it again is a no-op, not a second backup or a wiped table.
    let backups_before = std::fs::read_dir(dir.join("backups"))
        .expect("backups")
        .count();
    ingest::migrate(&pool).await.expect("second migrate");
    let backups_after = std::fs::read_dir(dir.join("backups"))
        .expect("backups")
        .count();
    assert_eq!(
        backups_after, backups_before,
        "a repeat migration takes no backup"
    );
    let unlocked = projections::unlocked_achievement_ids(&pool)
        .await
        .expect("unlocked");
    assert_eq!(unlocked, vec!["first_commit".to_string()]);

    let rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM repeatable_achievement_events")
        .fetch_one(&pool)
        .await
        .expect("new table");
    assert_eq!(
        rows.0, 0,
        "the new table starts empty, it does not invent history"
    );

    pool.close().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn the_daily_rank_counts_every_repository_together() {
    // Spec 14.1 / checklist T1: A 3 commits + B 2 commits + C 1 commit is SIX commits
    // today, which is Rare. Counting per repository is how 3 + 2 + 1 stayed Common.
    let pool = temp_pool("global-daily").await;
    let mut fx = fixture();
    let identity = configured_identity();
    let facts = ingest::commit_facts(1, false);

    let plan = [("github:o/a", 3usize), ("github:o/b", 2), ("github:o/c", 1)];
    let mut sha_counter = 0usize;
    for (repository, commits) in plan {
        for _ in 0..commits {
            sha_counter += 1;
            let sha = format!("{:040x}", sha_counter);
            let event = commit_event_in(repository, &sha, "2026-11-16");
            let ctx = fx.ctx();
            ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
                .await
                .expect("ingest");
        }
    }

    let today = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("global day");
    assert_eq!(
        today.map(|d| d.commit_count),
        Some(6),
        "six commits across three repositories are one day of six"
    );
    let rank = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("global day")
        .and_then(|d| d.daily_rank);
    assert_eq!(
        rank,
        Some(devchievements_daily_rank::DailyRank::Rare),
        "5-9 commits in a day is Rare"
    );

    // The per-repository rows must still exist: the Repositories page shows them.
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/a")
            .await
            .expect("repo a"),
        3
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/b")
            .await
            .expect("repo b"),
        2
    );
}

#[tokio::test]
async fn a_new_day_starts_the_daily_rank_again_at_common() {
    // Spec 14.1/14.3: the rank belongs to one local date. Yesterday's Uncommon must
    // not suppress today's Common, and today's count must start from zero.
    let pool = temp_pool("daily-reset").await;
    let identity = configured_identity();
    let facts = ingest::commit_facts(1, false);
    let mut fx = fixture();

    for (sha, date) in [
        ("1", "2026-11-16"),
        ("2", "2026-11-16"),
        ("3", "2026-11-17"),
    ] {
        let event = commit_event(&sha.repeat(40), 1_794_787_200_000, date);
        let ctx = fx.ctx();
        ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
            .await
            .expect("ingest");
    }

    let first = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("first day")
        .expect("first day row");
    assert_eq!(first.commit_count, 2);
    assert_eq!(
        first.daily_rank,
        Some(devchievements_daily_rank::DailyRank::Uncommon)
    );

    let second = projections::daily_global_projection(&pool, "2026-11-17")
        .await
        .expect("second day")
        .expect("second day row");
    assert_eq!(second.commit_count, 1, "a new day counts from zero");
    assert_eq!(
        second.daily_rank,
        Some(devchievements_daily_rank::DailyRank::Common),
        "one commit on a new day is Common even if yesterday was Uncommon"
    );

    // Both days announced their own first rank: the notification is per day, not per
    // lifetime, and the second day's Common is not swallowed as a duplicate.
    let ranks: Vec<String> = sqlx::query_scalar(
        "SELECT payload FROM outbox WHERE kind = 'daily_rank_upgrade' ORDER BY created_at, notification_id",
    )
    .fetch_all(&pool)
    .await
    .expect("rank notifications");
    let mut names: Vec<String> = ranks
        .iter()
        .filter_map(|p| serde_json::from_str::<serde_json::Value>(p).ok())
        .filter_map(|v| v.get("rank")?.as_str().map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["common", "common", "uncommon"],
        "each day announced its own first rank; the second day did not inherit Uncommon"
    );

    // The repeatable table only knows commits, never ranks.
    let trigger_ids: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT achievement_id FROM repeatable_achievement_events")
            .fetch_all(&pool)
            .await
            .expect("triggers");
    assert_eq!(trigger_ids, vec!["commit_complete".to_string()]);
}

#[tokio::test]
async fn the_commit_total_counts_every_repository_together() {
    // Century is about the user, not about one repository (spec 13.2).
    let pool = temp_pool("global-total").await;
    let mut fx = fixture();
    let identity = configured_identity();
    let facts = ingest::commit_facts(1, false);

    for (index, repository) in ["github:o/a", "github:o/b", "github:o/c"]
        .iter()
        .enumerate()
    {
        for n in 0..40 {
            let sha = format!("{}{:039x}", index, n);
            let event = commit_event_in(repository, &sha, "2026-11-16");
            let ctx = fx.ctx();
            ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
                .await
                .expect("ingest");
        }
    }

    assert_eq!(
        projections::global_commit_total(&pool)
            .await
            .expect("total"),
        120,
        "120 commits spread over three repositories is a total of 120"
    );

    let unlocked: Vec<(String,)> =
        sqlx::query_as("SELECT achievement_id FROM unlocked_achievements")
            .fetch_all(&pool)
            .await
            .expect("unlocked");
    assert!(
        unlocked.iter().any(|(id,)| id == "commits_100"),
        "Century must unlock on the combined total, got {unlocked:?}"
    );
}

#[tokio::test]
async fn a_rebuild_produces_the_same_global_numbers_as_live_ingestion() {
    // The projections are derived, so a rebuild must land on exactly what live
    // ingestion accumulated -- otherwise the migration this test guards is a lie.
    let pool = temp_pool("global-rebuild").await;
    let mut fx = fixture();
    let identity = configured_identity();
    let facts = ingest::commit_facts(1, false);

    for (index, repository) in ["github:o/a", "github:o/b"].iter().enumerate() {
        for n in 0..5 {
            let sha = format!("{}{:039x}", index, n);
            let event = commit_event_in(repository, &sha, "2026-11-16");
            let ctx = fx.ctx();
            ingest::ingest_commit(&pool, &event, &facts, 0, &identity, &ctx)
                .await
                .expect("ingest");
        }
    }

    let live_total = projections::global_commit_total(&pool)
        .await
        .expect("total");
    let live_day = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("day")
        .map(|d| (d.commit_count, d.daily_rank));
    // The outbox is NOT part of a projection rebuild: the rows already there describe
    // notifications the user may still be owed. What must hold is that the rebuild
    // does not ADD any, because a rebuild is a recomputation, not a new event.
    let queued_before = ingest::outbox_counts(&pool).await.expect("outbox");

    projections::rebuild_from_events(&pool, 0)
        .await
        .expect("rebuild");

    assert_eq!(
        projections::global_commit_total(&pool)
            .await
            .expect("total"),
        live_total,
        "the rebuild must reproduce the global total"
    );
    let rebuilt_day = projections::daily_global_projection(&pool, "2026-11-16")
        .await
        .expect("day")
        .map(|d| (d.commit_count, d.daily_rank));
    assert_eq!(rebuilt_day, live_day, "and the global day projection");

    // A rebuild must not revoke anything (spec 13.4) nor queue a notification.
    let unlocked: Vec<(String,)> =
        sqlx::query_as("SELECT achievement_id FROM unlocked_achievements")
            .fetch_all(&pool)
            .await
            .expect("unlocked");
    assert!(
        unlocked.iter().any(|(id,)| id == "first_commit"),
        "an unlock already recorded must survive a rebuild, got {unlocked:?}"
    );
    let queued_after = ingest::outbox_counts(&pool).await.expect("outbox");
    assert_eq!(
        queued_after, queued_before,
        "a rebuild must not add notifications: it recomputes, it does not re-announce"
    );
}

#[tokio::test]
async fn a_digest_policy_queues_one_summary_instead_of_a_celebration() {
    // Spec 16.3, the middle band: an event between ten minutes and a day old is
    // summarised. The unlock is real and stays realtime; what changes is that the
    // outbox carries ONE summary row instead of a celebration per reward.
    let pool = temp_pool("digest-policy").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();
    let event = browser_event(
        EventType::PullRequestMerged,
        "pr:4",
        1_794_787_200_000,
        "2026-11-16",
    );

    let outcome = ingest::ingest_event_with_policy(
        &pool,
        &event,
        &not_a_commit(),
        0,
        &identity,
        &ctx,
        ingest::NotificationPolicy::Digest,
        &[],
    )
    .await
    .expect("ingest");

    match &outcome {
        IngestOutcome::Ingested {
            queued_notifications,
        } => {
            assert_eq!(queued_notifications.len(), 1, "{outcome:?}");
            assert!(
                queued_notifications[0].starts_with("digest:"),
                "a digest row is not a celebration, got {:?}",
                queued_notifications[0]
            );
        }
        other => panic!("expected one summary, got {other:?}"),
    }

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, payload FROM outbox ORDER BY notification_id")
            .fetch_all(&pool)
            .await
            .expect("outbox");
    assert_eq!(rows.len(), 1, "one event, one summary row: {rows:?}");
    assert_eq!(rows[0].0, "digest_summary", "no celebration may be queued");
    assert!(
        rows[0].1.contains("first_pr_merged"),
        "the summary must name what unlocked, got {:?}",
        rows[0].1
    );

    let unlocked: Vec<(String, String)> =
        sqlx::query_as("SELECT achievement_id, unlock_source FROM unlocked_achievements")
            .fetch_all(&pool)
            .await
            .expect("unlocked");
    assert_eq!(
        unlocked,
        vec![("first_pr_merged".to_string(), "realtime".to_string())],
        "a late delivery is not an import; labelling it one would lie in the gallery"
    );
}

#[tokio::test]
async fn a_rebuild_reproduces_the_browser_counters() {
    // The projections are derived, so rebuilding from the event log must land on the
    // same numbers live ingestion produced -- otherwise "recompute history" is a lie.
    let pool = temp_pool("browser-rebuild").await;
    let mut fx = fixture();
    let ctx = fx.ctx();
    let identity = identity_with_login();

    for (event_type, external_id) in [
        (EventType::PullRequestMerged, "pr:1"),
        (EventType::PullRequestMerged, "pr:2"),
        (EventType::ReleasePublished, "release:1"),
        (EventType::StarGiven, "star:1"),
    ] {
        let event = browser_event(event_type, external_id, 1_794_787_200_000, "2026-11-16");
        ingest::ingest_commit_with_rules(&pool, &event, &not_a_commit(), 0, &identity, &ctx, &[])
            .await
            .expect("ingest");
    }

    let before = projections::browser_totals(&pool).await.expect("totals");
    assert_eq!(before.pull_request_merged_count, 2);
    assert_eq!(before.release_count, 1);
    assert_eq!(before.star_given_count, 1);

    projections::rebuild_from_events(&pool, 9_999)
        .await
        .expect("rebuild");

    assert_eq!(
        projections::browser_totals(&pool).await.expect("totals"),
        before,
        "a rebuild must reproduce the same numbers"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        0
    );
}

// ------------------------------------------------------ failure must not be silent

#[tokio::test]
async fn a_projection_read_failure_refuses_the_event_instead_of_guessing() {
    // A failed read used to be swallowed by `unwrap_or_default()`, so the Engine
    // judged the commit against a blank snapshot and then persisted the result of
    // that guess: the counter stayed at zero, the event was still recorded, and the
    // Companion marked the Spool record consumed -- the commit was lost for good.
    // The read error is now returned, the transaction rolls back, and the retry
    // counts once.
    let pool = temp_pool("read-failure").await;
    let mut fx = fixture();
    let facts = ingest::commit_facts(1, false);
    let event = commit_event(&"7".repeat(40), 1_794_787_200_000, "2026-11-16");

    // Break exactly one read and leave every write path intact. Renaming or dropping
    // a table cannot do that: ingestion writes the same three projection tables it
    // reads, so the write side reports the failure even when the read is swallowed,
    // and the test would pass for the wrong reason.
    //
    // So poison the value instead: put an invalid UTF-8 BLOB into the
    // `achievement_id` primary key. TEXT affinity never converts a BLOB, no table is
    // STRICT, and `impl Decode for String` fails on `from_utf8`, so the row can only
    // be read by erroring. Every write to this table still succeeds, because those
    // writes never read existing rows back.
    sqlx::raw_sql(
        "INSERT INTO unlocked_achievements (achievement_id, unlocked_at, unlock_source, rule_version) \
         VALUES (X'FFFE', 0, 'fault', 1)",
    )
    .execute(&pool)
    .await
    .expect("poison");

    let failed =
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &fx.ctx()).await;
    assert!(
        failed.is_err(),
        "a projection read failure must be returned, not defaulted: {failed:?}"
    );

    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        0,
        "the event must not be recorded when its evaluation could not be read"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        0,
        "the counter must not move either"
    );

    // The read failure must not have been turned into a different error: an event
    // that could not be evaluated is refused because it could not be read, and the
    // message has to say so rather than blaming the write that never happened.
    let message = format!("{}", failed.expect_err("read failure"));
    assert!(
        message.contains("decoding column"),
        "the failure must be the projection read, not an unrelated one: {message}"
    );

    // Clear the fault the way a repaired database would, then replay the same event.
    sqlx::raw_sql("DELETE FROM unlocked_achievements WHERE unlock_source = 'fault'")
        .execute(&pool)
        .await
        .expect("unpoison");
    let outcome =
        ingest::ingest_commit(&pool, &event, &facts, 0, &configured_identity(), &fx.ctx())
            .await
            .expect("ingest");
    assert!(
        matches!(
            outcome,
            IngestOutcome::Ingested { .. } | IngestOutcome::IngestedSilently
        ),
        "the retry must ingest, not see a duplicate: {outcome:?}"
    );

    assert_eq!(
        ingest::event_count(&pool).await.expect("count"),
        1,
        "the retry must count exactly once"
    );
    assert_eq!(
        projections::repository_commit_count(&pool, "github:o/r")
            .await
            .expect("count"),
        1
    );
}

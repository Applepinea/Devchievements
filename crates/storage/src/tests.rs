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

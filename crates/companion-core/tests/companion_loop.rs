//! Companion integration tests against real SQLite and a real Spool directory.
//!
//! # Why these are integration tests and not unit tests
//!
//! The whole point of separating the Companion core from the Tauri shell is that the
//! data loop can be exercised for real: a real database file, a real append-only
//! Spool, real records in the same format the Helper writes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_companion_core::delivery::{
    DeliverySink, DisplayOutcome, OutboxProcessor, RecordingSink,
};
use devchievements_companion_core::{Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_git_hooks::spool::Spool;
use devchievements_notifications::Presentation;
use devchievements_storage::ingest;
use std::path::{Path, PathBuf};

struct Fixture {
    root: PathBuf,
    spool: PathBuf,
    pool: sqlx::SqlitePool,
}

impl Fixture {
    async fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("devch-comp-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("root");
        let spool = root.join("spool");
        let pool = devchievements_storage::open(&root.join("companion.db"))
            .await
            .expect("open db");
        ingest::migrate(&pool).await.expect("migrate");
        Fixture { root, spool, pool }
    }

    fn companion(&self) -> Companion {
        Companion::new(
            self.pool.clone(),
            CompanionConfig::new(&self.spool, "Asia/Taipei"),
        )
    }

    fn cleanup(&self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn identity() -> Identity {
    let mut id = Identity::default();
    id.add_email("me@example.com", EmailOrigin::UserConfigured);
    id
}

/// Write a hook record in exactly the format the Helper produces.
fn push_record(spool_dir: &Path, event_id: &str, sha: &str, occurred_ms: i64, parents: usize) {
    let spool = Spool::open(spool_dir).expect("spool");
    let payload = serde_json::json!({
        "event_id": event_id,
        "hook": "post-commit",
        "hook_args": [],
        "repo_git_dir": "C:/repos/demo/.git",
        "commit_sha": sha,
        "parents": vec!["p"; parents],
        "author_name": "Me",
        "author_email": "me@example.com",
        "committer_email": "me@example.com",
        "subject": "a commit",
        "occurred_ms": occurred_ms,
    })
    .to_string();
    spool.append(event_id, &payload).expect("append");
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tokio::test]
async fn draining_the_spool_ingests_and_queues_notifications() {
    // The core of spec 32.3, minus the popup.
    let fx = Fixture::new("drain").await;
    push_record(&fx.spool, "evt-1", &"a".repeat(40), now_ms(), 1);

    let report = fx.companion().drain_once(&identity()).await.expect("drain");

    assert_eq!(report.read, 1);
    assert_eq!(report.ingested, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(
        report.files_consumed, 1,
        "files are consumed only on success"
    );
    assert!(
        report.queued_notifications > 0,
        "a first commit queues feedback"
    );

    let (commits, events) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(commits, 1);
    assert_eq!(events, 1);

    fx.cleanup();
}

#[tokio::test]
async fn draining_twice_does_not_double_count() {
    let fx = Fixture::new("twice").await;
    push_record(&fx.spool, "evt-1", &"b".repeat(40), now_ms(), 1);

    let first = fx
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain 1");
    assert_eq!(first.ingested, 1);

    let second = fx
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain 2");
    assert_eq!(second.read, 0, "consumed records are not re-read");

    let (commits, _) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(commits, 1);

    fx.cleanup();
}

#[tokio::test]
async fn a_record_replayed_before_consumption_is_a_duplicate() {
    // Simulates a crash between ingest and consume.
    let fx = Fixture::new("replay").await;
    let now = now_ms();
    push_record(&fx.spool, "evt-1", &"c".repeat(40), now, 1);
    let first = fx.companion().drain_once(&identity()).await.expect("drain");
    assert_eq!(first.ingested, 1);

    // The same record reappears, as it would after a crash before consumption.
    push_record(&fx.spool, "evt-1", &"c".repeat(40), now, 1);
    let again = fx
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain 2");

    assert_eq!(again.duplicates, 1, "the replay must be rejected");
    assert_eq!(again.ingested, 0);

    let (commits, _) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(commits, 1, "no double counting");

    fx.cleanup();
}

#[tokio::test]
async fn an_unconfigured_identity_is_refused() {
    let fx = Fixture::new("noid").await;
    push_record(&fx.spool, "evt-1", &"d".repeat(40), now_ms(), 1);

    let report = fx
        .companion()
        .drain_once(&Identity::default())
        .await
        .expect("drain");
    assert_eq!(report.refused, 1);
    assert_eq!(report.ingested, 0);

    fx.cleanup();
}

#[tokio::test]
async fn a_merge_record_is_recorded_but_not_counted() {
    let fx = Fixture::new("merge").await;
    push_record(&fx.spool, "evt-1", &"e".repeat(40), now_ms(), 2);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let (commits, events) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(events, 1, "the event is recorded");
    assert_eq!(commits, 0, "but merges do not count as commits (spec 8.1)");

    fx.cleanup();
}

#[tokio::test]
async fn delivery_shows_and_completes() {
    let fx = Fixture::new("deliver").await;
    push_record(&fx.spool, "evt-1", &"f".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let processor = OutboxProcessor::new(fx.pool.clone());
    let mut sink = RecordingSink::default();
    let (shown, unknown) = processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");

    assert!(shown > 0);
    assert_eq!(unknown, 0);
    assert_eq!(sink.shown.len(), shown);

    let pending = fx
        .companion()
        .pending_notifications()
        .await
        .expect("pending");
    assert!(pending.is_empty(), "delivered rows leave the queue");

    fx.cleanup();
}

#[tokio::test]
async fn a_failing_sink_reports_unknown_instead_of_retrying_forever() {
    let fx = Fixture::new("fail").await;
    push_record(&fx.spool, "evt-1", &"1".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let processor = OutboxProcessor::new(fx.pool.clone());
    let mut sink = RecordingSink {
        always_fail: true,
        ..Default::default()
    };
    let (shown, unknown) = processor
        .deliver_pending(&mut sink, 1)
        .await
        .expect("deliver");

    assert_eq!(shown, 0);
    assert!(unknown > 0, "exhausted retries are recorded as unknown");

    fx.cleanup();
}

#[tokio::test]
async fn a_crash_after_display_attempt_does_not_redisplay() {
    // Spec 17.9 / 32.5 item 6.
    let fx = Fixture::new("crash").await;
    push_record(&fx.spool, "evt-1", &"2".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    // Persist an attempt without completing: exactly what a kill would leave behind.
    sqlx::query(
        "UPDATE outbox SET status = 'desktop_displayed', display_attempted_at = 123
         WHERE status IN ('created','desktop_pending')",
    )
    .execute(&fx.pool)
    .await
    .expect("mark attempt");

    let processor = OutboxProcessor::new(fx.pool.clone());
    let recovered = processor.recover_after_crash().await.expect("recover");
    assert!(recovered > 0, "the interrupted row must be resolved");

    let mut sink = RecordingSink::default();
    let (shown, _) = processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");
    assert_eq!(shown, 0, "never redisplay a possibly-shown notification");
    assert!(sink.shown.is_empty());

    fx.cleanup();
}

#[tokio::test]
async fn statistics_survive_a_companion_restart() {
    let fx = Fixture::new("restart").await;
    push_record(&fx.spool, "evt-1", &"3".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let before = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");

    fx.pool.close().await;
    let reopened = devchievements_storage::open(&fx.root.join("companion.db"))
        .await
        .expect("reopen");
    let companion = Companion::new(
        reopened.clone(),
        CompanionConfig::new(&fx.spool, "Asia/Taipei"),
    );
    let after = companion
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(before, after);

    fx.cleanup();
}

#[tokio::test]
async fn an_old_record_still_counts_but_is_quiet() {
    // Spec 16.3: age decides presentation, not whether the event counts.
    let fx = Fixture::new("stale").await;
    let old = now_ms() - 2 * 24 * 60 * 60 * 1000;
    push_record(&fx.spool, "evt-1", &"4".repeat(40), old, 1);

    let report = fx.companion().drain_once(&identity()).await.expect("drain");
    assert_eq!(report.ingested, 1);
    // "Quiet" is a claim about side effects, so it is asserted as one: no celebration
    // may be queued and the outbox must actually be empty. A count of zero on its own
    // would still pass if a row had been written and simply not reported.
    assert_eq!(
        report.queued_notifications, 0,
        "a stale event must not queue a celebration"
    );
    let pending = fx
        .companion()
        .pending_notifications()
        .await
        .expect("pending");
    assert!(
        pending.is_empty(),
        "the outbox must be empty, found {} row(s)",
        pending.len()
    );

    let (commits, _) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(commits, 1, "statistics update even for stale events");

    fx.cleanup();
}

/// A sink that always reports success, used to prove the happy path.
struct AlwaysShown;
impl DeliverySink for AlwaysShown {
    fn present(&mut self, _p: &Presentation) -> DisplayOutcome {
        DisplayOutcome::Shown
    }
}

#[tokio::test]
async fn a_successful_sink_completes_everything() {
    let fx = Fixture::new("sink").await;
    push_record(&fx.spool, "evt-1", &"5".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let processor = OutboxProcessor::new(fx.pool.clone());
    let mut sink = AlwaysShown;
    let (shown, unknown) = processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");
    assert!(shown > 0);
    assert_eq!(unknown, 0);

    fx.cleanup();
}

#[tokio::test]
async fn one_commit_with_several_rewards_produces_exactly_one_popup() {
    // Spec 16.6: several rewards from one event merge into a single popup. Before
    // this was wired up, each intent became its own notification and the user would
    // have seen three windows for one commit.
    let fx = Fixture::new("merge-popup").await;
    push_record(&fx.spool, "evt-1", &"6".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let queued = fx
        .companion()
        .pending_notifications()
        .await
        .expect("pending");
    assert!(
        queued.len() >= 2,
        "the first commit queues several intents, got {}",
        queued.len()
    );

    let processor = OutboxProcessor::new(fx.pool.clone());
    let mut sink = RecordingSink::default();
    let (shown, _) = processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");

    assert_eq!(shown, 1, "one event must yield one presentation");
    assert_eq!(sink.shown.len(), 1, "and exactly one thing shown");

    match &sink.shown[0] {
        Presentation::Popup(popup) => {
            assert!(
                popup.title.to_lowercase().contains("first"),
                "the achievement leads an equal-rarity merge, got {:?}",
                popup.title
            );
            assert!(
                popup
                    .others
                    .iter()
                    .any(|o| o.to_lowercase().contains("day")),
                "the daily rank appears as a sub-line, got {:?}",
                popup.others
            );
        }
        other => panic!("expected a merged popup, got {other:?}"),
    }

    // Everything is resolved, so nothing is left pending.
    assert!(fx
        .companion()
        .pending_notifications()
        .await
        .expect("pending")
        .is_empty());

    fx.cleanup();
}

#[tokio::test]
async fn a_plain_commit_plays_a_sound_and_shows_no_popup() {
    // Spec 16.4: a normal commit gets a light acknowledgement, not a window.
    let fx = Fixture::new("sound").await;
    let processor = OutboxProcessor::new(fx.pool.clone());
    let mut sink = RecordingSink::default();

    // Commit 1 reaches Common, so it produces a popup.
    push_record(&fx.spool, "evt-1", &"7".repeat(40), now_ms(), 1);
    fx.companion()
        .drain_once(&identity())
        .await
        .expect("drain 1");
    processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver 1");
    assert!(!sink.shown.is_empty(), "the first commit must celebrate");
    sink.shown.clear();

    // Commit 2 moves Common -> Uncommon, which is still an upgrade.
    push_record(&fx.spool, "evt-2", &"8".repeat(40), now_ms(), 1);
    fx.companion()
        .drain_once(&identity())
        .await
        .expect("drain 2");
    processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver 2");
    assert!(
        matches!(sink.shown.first(), Some(Presentation::Popup(_))),
        "a rank upgrade is a popup, got {:?}",
        sink.shown.first()
    );
    sink.shown.clear();

    // Commit 3 stays inside Uncommon (2-4 commits), so nothing new unlocks.
    push_record(&fx.spool, "evt-3", &"9".repeat(40), now_ms(), 1);
    let report = fx
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain 3");
    assert_eq!(report.ingested, 1);
    processor
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver 3");

    assert_eq!(sink.shown.len(), 1, "one acknowledgement for one commit");
    assert_eq!(
        sink.shown[0],
        Presentation::SoundOnly,
        "a plain commit must not open a window"
    );

    fx.cleanup();
}

/// A record forwarded by the Native Messaging host must be judged like any other
/// event. This is the link between the extension and the desktop engine, so it is
/// tested here rather than only in a browser.
#[tokio::test]
async fn a_browser_report_is_ingested_as_a_commit() {
    let fixture = Fixture::new("browser").await;
    let spool = Spool::open(&fixture.spool).expect("spool");
    spool
        .append(
            "browser:github:o/r#abcdef0123456789abcdef0123456789abcdef01",
            &serde_json::json!({
                "kind": "browser_github",
                "event_id": "browser:github:o/r#abcdef0123456789abcdef0123456789abcdef01",
                "repository_id": "github:o/r",
                "activity": "commit",
                "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
                "external_id": null,
                "occurred_ms": now_ms() - 1_000,
                "author_login": "me",
            })
            .to_string(),
        )
        .expect("append");

    let mut id = Identity::default();
    id.add_email("browser@devchievements.local", EmailOrigin::UserConfigured);
    // Spec 7: the browser path attributes by login, and only an explicit mapping
    // makes a login the user's.
    id.add_login_mapping("me", vec!["browser@devchievements.local".to_string()]);

    let report = fixture.companion().drain_once(&id).await.expect("drain");

    assert_eq!(
        report.ingested, 1,
        "a browser report must count: {report:?}"
    );
    assert_eq!(report.failed, 0, "nothing should fail: {report:?}");

    let count =
        devchievements_storage::projections::repository_commit_count(&fixture.pool, "github:o/r")
            .await
            .expect("count");
    assert_eq!(count, 1, "the commit must be visible in projections");

    fixture.cleanup();
}

/// The same repository seen through the hook and through the browser must be ONE
/// repository (spec 6.1). Before the remote was used, the hook recorded
/// "local:<path>" while the browser recorded "github:owner/repo", so a user who
/// worked both ways saw their history split in two.
#[tokio::test]
async fn hook_and_browser_agree_on_one_repository() {
    let fixture = Fixture::new("identity").await;
    let spool = Spool::open(&fixture.spool).expect("spool");

    // The hook path, now carrying the remote it read from git config.
    spool
        .append(
            "post-commit:hook-sha:1",
            &serde_json::json!({
                "event_id": "post-commit:hook-sha:1",
                "hook": "post-commit",
                "hook_args": [],
                "repo_git_dir": "C:/repos/demo/.git",
                "repo_remote_url": "git@github.com:Owner/Repo.git",
                "commit_sha": "1111111111111111111111111111111111111111",
                "parents": ["p"],
                "author_name": "Me",
                "author_email": "me@example.com",
                "committer_email": "me@example.com",
                "subject": "a commit",
                "occurred_ms": now_ms() - 1_000,
            })
            .to_string(),
        )
        .expect("append");

    // The browser path for the same repository, spelled differently on purpose:
    // HTTPS, lowercase, with a .git suffix.
    spool
        .append(
            "browser:github:owner/repo#abc",
            &serde_json::json!({
                "kind": "browser_github",
                "event_id": "browser:github:owner/repo#abc",
                "repository_id": "github:owner/repo",
                "activity": "commit",
                "commit_sha": "2222222222222222222222222222222222222222",
                "external_id": null,
                "occurred_ms": now_ms() - 500,
                "author_login": "me",
            })
            .to_string(),
        )
        .expect("append");

    let mut id = identity();
    id.add_login_mapping("me", vec!["me@example.com".to_string()]);

    let report = fixture.companion().drain_once(&id).await.expect("drain");
    assert_eq!(report.ingested, 2, "both must ingest: {report:?}");

    let repositories =
        devchievements_storage::read_models::repositories(&fixture.pool, |id| id.to_string())
            .await
            .expect("repositories");

    assert_eq!(
        repositories.len(),
        1,
        "one repository, not two: {:?}",
        repositories.iter().map(|r| &r.alias).collect::<Vec<_>>()
    );
    assert_eq!(repositories[0].commits, 2);

    fixture.cleanup();
}

/// The other half of spec 6.1: without a remote there is nothing to merge, so the
/// repository stays a local identity. Merging two remote-less clones would be a
/// worse error than keeping them apart, and the spec forbids guessing.
#[tokio::test]
async fn a_repository_without_a_remote_stays_local() {
    let fixture = Fixture::new("no-remote").await;
    let spool = Spool::open(&fixture.spool).expect("spool");
    spool
        .append(
            "post-commit:local-sha:1",
            &serde_json::json!({
                "event_id": "post-commit:local-sha:1",
                "hook": "post-commit",
                "hook_args": [],
                "repo_git_dir": "C:/repos/private/.git",
                "repo_remote_url": null,
                "commit_sha": "3333333333333333333333333333333333333333",
                "parents": ["p"],
                "author_name": "Me",
                "author_email": "me@example.com",
                "committer_email": "me@example.com",
                "subject": "local only",
                "occurred_ms": now_ms() - 1_000,
            })
            .to_string(),
        )
        .expect("append");

    let report = fixture
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain");
    assert_eq!(report.ingested, 1, "{report:?}");

    let repositories =
        devchievements_storage::read_models::repositories(&fixture.pool, |id| id.to_string())
            .await
            .expect("repositories");

    assert_eq!(repositories.len(), 1);
    assert!(
        repositories[0].alias.starts_with("local:"),
        "must stay a local identity, got {:?}",
        repositories[0].alias
    );

    fixture.cleanup();
}

/// Spec 7: an unattributed browser event must not count.
///
/// The page says nothing about who committed, so attributing it to the user would
/// silently inflate every statistic with other people's work.
#[tokio::test]
async fn a_browser_report_without_attribution_is_refused() {
    let fixture = Fixture::new("no-author").await;
    let spool = Spool::open(&fixture.spool).expect("spool");
    spool
        .append(
            "browser:github:o/r#anonymous",
            &serde_json::json!({
                "kind": "browser_github",
                "event_id": "browser:github:o/r#anonymous",
                "repository_id": "github:o/r",
                "activity": "commit",
                "commit_sha": "4444444444444444444444444444444444444444",
                "external_id": null,
                "occurred_ms": now_ms() - 1_000,
            })
            .to_string(),
        )
        .expect("append");

    let report = fixture
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain");

    assert_eq!(
        report.ingested, 0,
        "an unattributed report must not count: {report:?}"
    );
    assert_eq!(
        report.refused, 1,
        "it must be refused, not silently dropped: {report:?}"
    );

    fixture.cleanup();
}

/// A login that maps to one of the configured emails counts; a stranger's does not.
#[tokio::test]
async fn only_a_mapped_login_is_the_user() {
    let fixture = Fixture::new("mapped-login").await;
    let spool = Spool::open(&fixture.spool).expect("spool");

    for (id, sha, login) in [
        ("mine", "5555555555555555555555555555555555555555", "me"),
        (
            "theirs",
            "6666666666666666666666666666666666666666",
            "stranger",
        ),
    ] {
        spool
            .append(
                &format!("browser:github:o/r#{id}"),
                &serde_json::json!({
                    "kind": "browser_github",
                    "event_id": format!("browser:github:o/r#{id}"),
                    "repository_id": "github:o/r",
                    "activity": "commit",
                    "commit_sha": sha,
                    "external_id": null,
                    "occurred_ms": now_ms() - 1_000,
                    "author_login": login,
                })
                .to_string(),
            )
            .expect("append");
    }

    let mut id = identity();
    id.add_login_mapping("me", vec!["me@example.com".to_string()]);

    let report = fixture.companion().drain_once(&id).await.expect("drain");

    assert_eq!(
        report.ingested, 1,
        "only the mapped login counts: {report:?}"
    );
    assert_eq!(
        report.refused, 1,
        "the stranger must be refused: {report:?}"
    );

    fixture.cleanup();
}

/// Spec 16.3: the band boundaries are load-bearing, so every edge is exercised.
///
/// 0-10 minutes is a full celebration. 10 minutes - 24 hours is a digest: it must
/// never open a window. Over 24 hours only the statistics move, and the display
/// channel must not even be called.
#[tokio::test]
async fn the_freshness_bands_decide_what_the_user_sees() {
    const MINUTE: i64 = 60 * 1000;

    for (name, age_ms, expected) in [
        ("band-9m", 9 * MINUTE, "popup"),
        ("band-11m", 11 * MINUTE, "digest"),
        ("band-23h59m", 23 * 60 * MINUTE + 59 * MINUTE, "digest"),
        ("band-24h01m", 24 * 60 * MINUTE + MINUTE, "nothing"),
    ] {
        let fx = Fixture::new(name).await;
        push_record(&fx.spool, "evt-1", &"b".repeat(40), now_ms() - age_ms, 1);

        let report = fx.companion().drain_once(&identity()).await.expect("drain");
        assert_eq!(report.ingested, 1, "{name}: the event must still count");

        let (commits, _) = fx
            .companion()
            .statistics("local:C:/repos/demo/.git")
            .await
            .expect("stats");
        assert_eq!(
            commits, 1,
            "{name}: age decides presentation, never whether the event counts"
        );

        let mut sink = RecordingSink::default();
        let (shown, _) = OutboxProcessor::new(fx.pool.clone())
            .deliver_pending(&mut sink, 3)
            .await
            .expect("deliver");

        match expected {
            "popup" => assert!(
                matches!(sink.shown.first(), Some(Presentation::Popup(_))),
                "{name}: a fresh event celebrates, got {:?}",
                sink.shown
            ),
            // A window here is the reported bug: a Spool replayed after a weekend
            // used to open one window per record.
            "digest" => {
                assert!(
                    sink.shown
                        .iter()
                        .all(|p| !matches!(p, Presentation::Popup(_))),
                    "{name}: the digest band must not open a window, got {:?}",
                    sink.shown
                );
                assert_eq!(shown, 1, "{name}: whatever is shown is one summary");

                // Spec 16.3 / 17.5: a digested (or suppressed) event must not lose its
                // discoverability. The gallery marks a row as new while `viewed_at` is
                // NULL, so a summary that replaced the popup still has to leave one.
                let unviewed: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM unlocked_achievements WHERE viewed_at IS NULL",
                )
                .fetch_one(&fx.pool)
                .await
                .expect("unviewed");
                assert!(
                    unviewed.0 > 0,
                    "{name}: the gallery must still mark the unlock as new"
                );
            }
            _ => {
                assert_eq!(shown, 0, "{name}: nothing may be shown");
                assert!(
                    sink.shown.is_empty(),
                    "{name}: the display channel must not be called, got {:?}",
                    sink.shown
                );
            }
        }

        fx.cleanup();
    }
}

/// A digested event that unlocked nothing produces no summary at all.
///
/// The middle band exists to summarise what happened while the user was away. A plain
/// commit that moved no rank and unlocked no achievement has nothing to summarise, so
/// it must not manufacture an empty "0 achievements" line -- and it must not fall back
/// to the fresh-band sound either, because the user is not at the keyboard.
#[tokio::test]
async fn a_digested_commit_with_no_reward_stays_completely_silent() {
    let fx = Fixture::new("digest-no-reward").await;

    // Two fresh commits establish the day and consume the first two ranks.
    push_record(&fx.spool, "evt-1", &"c".repeat(40), now_ms(), 1);
    push_record(&fx.spool, "evt-2", &"d".repeat(40), now_ms(), 1);
    fx.companion().drain_once(&identity()).await.expect("drain");

    let mut sink = RecordingSink::default();
    OutboxProcessor::new(fx.pool.clone())
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");
    sink.shown.clear();

    // The third commit stays inside Uncommon (2-4), so its only intent is the bare
    // commit -- and it is eleven minutes old, i.e. in the digest band.
    push_record(
        &fx.spool,
        "evt-3",
        &"e".repeat(40),
        now_ms() - 11 * 60 * 1000,
        1,
    );
    let report = fx
        .companion()
        .drain_once(&identity())
        .await
        .expect("drain stale");
    assert_eq!(report.ingested, 1, "{report:?}");
    assert_eq!(
        report.queued_notifications, 0,
        "nothing worth summarising must not queue a row"
    );

    let mut sink = RecordingSink::default();
    let (shown, _) = OutboxProcessor::new(fx.pool.clone())
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver stale");
    assert_eq!(shown, 0, "nothing may be presented");
    assert!(
        sink.shown.is_empty(),
        "the display channel must not be called, got {:?}",
        sink.shown
    );

    let (commits, _) = fx
        .companion()
        .statistics("local:C:/repos/demo/.git")
        .await
        .expect("stats");
    assert_eq!(commits, 3, "the stale commit still counts");

    fx.cleanup();
}

/// Spec 16.3 / 16.4: replaying a weekend's Spool must not open a pile of windows.
#[tokio::test]
async fn a_weekend_replay_produces_no_windows() {
    let fx = Fixture::new("weekend-replay").await;
    let base = now_ms() - 12 * 60 * 1000;
    for i in 0..50usize {
        let sha = format!("{:040x}", i + 1);
        push_record(
            &fx.spool,
            &format!("evt-{i}"),
            &sha,
            base + i as i64 * 100,
            1,
        );
    }

    let report = fx.companion().drain_once(&identity()).await.expect("drain");
    assert_eq!(report.ingested, 50, "{report:?}");

    let mut sink = RecordingSink::default();
    let (shown, _) = OutboxProcessor::new(fx.pool.clone())
        .deliver_pending(&mut sink, 3)
        .await
        .expect("deliver");

    assert!(
        sink.shown
            .iter()
            .all(|p| !matches!(p, Presentation::Popup(_))),
        "a replay must not open windows, got {} presentation(s): {:?}",
        sink.shown.len(),
        sink.shown
    );
    assert!(
        shown <= 1,
        "at most one summary for a whole replay, got {shown}"
    );

    fx.cleanup();
}

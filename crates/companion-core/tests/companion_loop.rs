//! Companion integration tests against real SQLite and a real Spool directory.
//!
//! # Why these are integration tests and not unit tests
//!
//! The whole point of separating the Companion core from the Tauri shell is that the
//! data loop can be exercised for real: a real database file, a real append-only
//! Spool, real records in the same format the Helper writes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_companion_core::delivery::{
    DeliverySink, DisplayOutcome, OutboxProcessor, PendingNotification, RecordingSink,
};
use devchievements_companion_core::{Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_git_hooks::spool::Spool;
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
    fn show(&mut self, _n: &PendingNotification) -> DisplayOutcome {
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

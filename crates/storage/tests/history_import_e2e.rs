//! History import against a REAL git repository (spec 11.2).
//!
//! # Why not a mocked repository
//!
//! The import walks git itself, and the rules it must respect are git rules: which
//! commits are merges, which authors are mine, what the committer date is. A mocked
//! walker would test my assumptions about git rather than git.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;
use std::sync::atomic::AtomicBool;

use devchievements_core::{ClockProvider, InjectedContext, TimezoneProvider};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_storage::history_scan::{import_repository, scan_repository, ImportOptions};
use devchievements_storage::{ingest, projections};

#[derive(Debug)]
struct Clock(i64);
impl ClockProvider for Clock {
    fn now_unix_millis(&self) -> i64 {
        self.0
    }
}

#[derive(Debug)]
struct Zone;
impl TimezoneProvider for Zone {
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
struct Ids(u64);
impl devchievements_core::IdGenerator for Ids {
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

struct Fixture {
    pool: sqlx::SqlitePool,
    dir: std::path::PathBuf,
    repo: std::path::PathBuf,
}

impl Fixture {
    async fn new(name: &str) -> Option<Self> {
        let dir = std::env::temp_dir().join(format!("devch-import-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).ok()?;

        let pool = devchievements_storage::open(&dir.join("t.db")).await.ok()?;
        ingest::migrate(&pool).await.ok()?;

        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).ok()?;

        Some(Fixture { pool, dir, repo })
    }

    fn git(&self, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init(&self) {
        self.git(&["init", "-q", "-b", "main"]);
        self.git(&["config", "user.name", "Import Test"]);
        self.git(&["config", "user.email", "me@example.com"]);
    }

    /// A commit whose author and committer date are both fixed, so the import has a
    /// stable input regardless of when the test runs.
    fn commit_at(&self, file: &str, at: &str, message: &str) {
        std::fs::write(self.repo.join(file), message).unwrap();
        self.git(&["add", "-A"]);
        let stamp = format!("{at} +0800");
        Command::new("git")
            .args(["commit", "-q", "-m", message])
            .current_dir(&self.repo)
            .env("GIT_AUTHOR_DATE", &stamp)
            .env("GIT_COMMITTER_DATE", &stamp)
            .output()
            .unwrap();
    }

    fn cleanup(&self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn identity() -> Identity {
    let mut id = Identity::default();
    id.add_email("me@example.com", EmailOrigin::UserConfigured);
    id
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn scanning_a_real_repository_returns_its_commits() {
    if !git_available() {
        return;
    }
    let Some(fx) = Fixture::new("scan").await else {
        return;
    };
    fx.init();
    fx.commit_at("a.txt", "2023-05-01T10:00:00", "first");
    fx.commit_at("b.txt", "2023-05-02T10:00:00", "second");

    let scanned = scan_repository(&fx.repo).expect("scan");
    assert_eq!(scanned.len(), 2, "two commits expected");
    assert!(
        scanned.iter().all(|c| c.author_email == "me@example.com"),
        "the author email must be read"
    );
    // Committer dates must come back as the real times, not the import time: spec 11.2
    // requires history to be archived to its original date.
    assert!(
        scanned
            .iter()
            .any(|c| c.committed_at_ms == 1_683_000_000_000),
        "expected 2023-05-01T02:00:00Z, got {:?}",
        scanned
            .iter()
            .map(|c| c.committed_at_ms)
            .collect::<Vec<_>>()
    );

    fx.cleanup();
}

#[tokio::test]
async fn importing_history_records_commits_silently() {
    if !git_available() {
        return;
    }
    let Some(fx) = Fixture::new("import").await else {
        return;
    };
    fx.init();
    for i in 0..5 {
        fx.commit_at(
            &format!("f{i}.txt"),
            "2023-06-0{i}T10:00:00",
            &format!("c{i}"),
        );
    }

    let cancel = AtomicBool::new(false);
    let mut ids = Ids::default();
    let clock = Clock(1_800_000_000_000);
    let tz = Zone;
    let cfg = Cfg;
    let ctx = InjectedContext {
        clock: &clock,
        timezone: &tz,
        ids: &mut ids,
        config: &cfg,
    };

    let options = ImportOptions {
        batch_size: 2,
        timezone: "Asia/Taipei".to_string(),
        day_start_minutes: 0,
    };

    let report = import_repository(
        &fx.pool,
        "github:acme/imported",
        &fx.repo,
        &identity(),
        &options,
        &cancel,
        &ctx,
    )
    .await
    .expect("import");

    assert_eq!(report.scanned, 5);
    assert_eq!(report.matched, 5);
    assert_eq!(report.ingested, 5);
    assert_eq!(report.duplicates, 0);
    assert_eq!(report.batches, 3, "5 commits in batches of 2");

    // Spec 11.2: silent. No per-commit notification may be queued.
    let queued = devchievements_storage::ingest::outbox_counts(&fx.pool)
        .await
        .expect("outbox counts");
    assert!(
        queued.is_empty(),
        "a history import must not queue notifications, got {queued:?}"
    );

    let commits = projections::repository_commit_count(&fx.pool, "github:acme/imported")
        .await
        .expect("count");
    assert_eq!(commits, 5, "all five count as valid commits");

    fx.cleanup();
}

#[tokio::test]
async fn importing_the_same_history_twice_is_idempotent() {
    if !git_available() {
        return;
    }
    let Some(fx) = Fixture::new("twice").await else {
        return;
    };
    fx.init();
    for i in 0..3 {
        fx.commit_at(
            &format!("g{i}.txt"),
            "2023-07-0{i}T10:00:00",
            &format!("d{i}"),
        );
    }

    let cancel = AtomicBool::new(false);
    let mut ids = Ids::default();
    let clock = Clock(1_800_000_000_000);
    let tz = Zone;
    let cfg = Cfg;
    let ctx = InjectedContext {
        clock: &clock,
        timezone: &tz,
        ids: &mut ids,
        config: &cfg,
    };
    let options = ImportOptions {
        batch_size: 10,
        timezone: "Asia/Taipei".to_string(),
        day_start_minutes: 0,
    };

    let first = import_repository(
        &fx.pool,
        "github:acme/twice",
        &fx.repo,
        &identity(),
        &options,
        &cancel,
        &ctx,
    )
    .await
    .expect("first import");
    assert_eq!(first.ingested, 3);

    let second = import_repository(
        &fx.pool,
        "github:acme/twice",
        &fx.repo,
        &identity(),
        &options,
        &cancel,
        &ctx,
    )
    .await
    .expect("second import");
    assert_eq!(second.ingested, 0, "nothing new");
    assert_eq!(
        second.duplicates, 3,
        "everything recognised as already known"
    );

    let commits = projections::repository_commit_count(&fx.pool, "github:acme/twice")
        .await
        .expect("count");
    assert_eq!(commits, 3, "no double counting");

    fx.cleanup();
}

#[tokio::test]
async fn only_commits_by_the_configured_identity_are_imported() {
    if !git_available() {
        return;
    }
    let Some(fx) = Fixture::new("filter").await else {
        return;
    };
    fx.init();
    fx.commit_at("mine.txt", "2023-08-01T10:00:00", "mine");

    // A commit authored by somebody else, of the kind found in any shared repository.
    std::fs::write(fx.repo.join("theirs.txt"), "x").unwrap();
    fx.git(&["add", "-A"]);
    Command::new("git")
        .args(["commit", "-q", "-m", "theirs"])
        .current_dir(&fx.repo)
        .env("GIT_AUTHOR_NAME", "Someone Else")
        .env("GIT_AUTHOR_EMAIL", "other@example.com")
        .env("GIT_COMMITTER_NAME", "Someone Else")
        .env("GIT_COMMITTER_EMAIL", "other@example.com")
        .env("GIT_AUTHOR_DATE", "2023-08-02T10:00:00 +0800")
        .env("GIT_COMMITTER_DATE", "2023-08-02T10:00:00 +0800")
        .output()
        .unwrap();

    let cancel = AtomicBool::new(false);
    let mut ids = Ids::default();
    let clock = Clock(1_800_000_000_000);
    let tz = Zone;
    let cfg = Cfg;
    let ctx = InjectedContext {
        clock: &clock,
        timezone: &tz,
        ids: &mut ids,
        config: &cfg,
    };
    let options = ImportOptions::default();

    let report = import_repository(
        &fx.pool,
        "github:acme/filtered",
        &fx.repo,
        &identity(),
        &options,
        &cancel,
        &ctx,
    )
    .await
    .expect("import");

    assert_eq!(report.scanned, 2, "both commits are in the repository");
    assert_eq!(report.matched, 1, "only one belongs to the user");
    assert_eq!(report.ingested, 1);

    fx.cleanup();
}

#[tokio::test]
async fn a_commit_captured_live_is_not_counted_again_by_import() {
    // Spec 11.2 requires a unified dedup rule. The two paths give the same commit
    // different event ids, so without a shared identity the import would double count
    // everything the user did since installing.
    if !git_available() {
        return;
    }
    let Some(fx) = Fixture::new("live-dedup").await else {
        return;
    };
    fx.init();
    fx.commit_at("live.txt", "2023-09-01T10:00:00", "live");

    let cancel = AtomicBool::new(false);
    let mut ids = Ids::default();
    let clock = Clock(1_800_000_000_000);
    let tz = Zone;
    let cfg = Cfg;

    let scanned = scan_repository(&fx.repo).expect("scan");
    let sha = scanned[0].sha.clone();

    // Capture the same commit the way the hook would.
    let live_event = devchievements_events::Event {
        event_id: devchievements_events::EventId::new(format!("post-commit:{sha}:1")).unwrap(),
        event_type: devchievements_events::EventType::CommitCreated,
        source: devchievements_events::EventSource::LocalGitHook,
        source_confidence: devchievements_events::SourceConfidence::VerifiedLocal,
        canonical_repository_id: devchievements_events::CanonicalRepositoryId(
            "github:acme/live".to_string(),
        ),
        external_id: None,
        commit_sha: Some(sha.clone()),
        occurred_at: 1_693_543_200_000,
        observed_at: 1_693_543_200_000,
        received_at: 1_693_543_200_000,
        local_date: devchievements_events::LocalDate::new("2023-09-01").unwrap(),
        timezone: "Asia/Taipei".to_string(),
        date_policy: 0,
        payload: "{\"author_email\":\"me@example.com\"}".to_string(),
        schema_version: devchievements_events::SchemaVersion(1),
    };

    let ctx = InjectedContext {
        clock: &clock,
        timezone: &tz,
        ids: &mut ids,
        config: &cfg,
    };
    let facts = ingest::commit_facts(1, false);
    let outcome = ingest::ingest_commit(&fx.pool, &live_event, &facts, clock.0, &identity(), &ctx)
        .await
        .expect("live ingest");
    assert!(
        matches!(
            outcome,
            devchievements_storage::ingest::IngestOutcome::Ingested { .. }
        ),
        "the live capture must record it, got {outcome:?}"
    );

    // Now import the same history.
    let options = ImportOptions::default();
    let report = import_repository(
        &fx.pool,
        "github:acme/live",
        &fx.repo,
        &identity(),
        &options,
        &cancel,
        &ctx,
    )
    .await
    .expect("import");

    assert_eq!(report.scanned, 1);
    assert_eq!(report.matched, 1);
    assert_eq!(
        report.ingested, 0,
        "the live-captured commit must not be imported again"
    );
    assert_eq!(report.duplicates, 1);

    let commits = projections::repository_commit_count(&fx.pool, "github:acme/live")
        .await
        .expect("count");
    assert_eq!(commits, 1, "counted exactly once across both paths");

    fx.cleanup();
}

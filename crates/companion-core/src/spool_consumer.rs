//! Draining the Spool into the database (spec 32.1, 32.3).
//!
//! # The ordering that makes this safe
//!
//! 1. Read every pending record.
//! 2. Ingest each one **idempotently** (the event id is the primary key).
//! 3. Only after all of them are durable, move the Spool files to `consumed/`.
//!
//! Step 3 must come last. If the process dies between 2 and 3, the same records are
//! re-read on the next start and rejected as duplicates; nothing is lost and nothing
//! is double-counted. Doing it the other way round would lose events on a crash,
//! which spec 10.5 forbids outright.
//!
//! # Freshness
//!
//! A Hook that ran while the Companion was closed produces records whose
//! `occurred_ms` may be days old. Spec 16.3 requires those to be handled by age:
//! silent after 24 hours, digested in between. That decision happens here, at drain
//! time, not at display time.

use std::path::{Path, PathBuf};

use devchievements_core::{BundledTimezoneProvider, Engine, InjectedContext};
use devchievements_events::{
    CanonicalRepositoryId, Event, EventId, EventSource, EventType, Identity, LocalDate,
    SchemaVersion, SourceConfidence,
};
use devchievements_git_hooks::spool::{Record, Spool, SpoolError};
use devchievements_notifications::freshness_action;
use devchievements_storage::ingest::{self, IngestOutcome};
use devchievements_storage::projections;
use sqlx::SqlitePool;

/// How the Companion is configured.
#[derive(Debug, Clone)]
pub struct CompanionConfig {
    /// Where the Spool lives.
    pub spool_dir: PathBuf,
    /// The IANA zone used to compute `local_date` for incoming events.
    pub timezone: String,
    /// Day boundary in minutes from local midnight (spec 12).
    pub day_start_minutes: u16,
    /// The user's own achievement rules (spec 22).
    ///
    /// They live in the settings document, which the storage layer never reads and
    /// the Engine never receives, so they arrive through the caller that owns both.
    pub user_rules: Vec<devchievements_achievements::definition::UserAchievement>,
}

impl CompanionConfig {
    /// Defaults matching ADR-0005 (midnight boundary, no zone hard-coded).
    pub fn new(spool_dir: impl Into<PathBuf>, timezone: impl Into<String>) -> Self {
        CompanionConfig {
            spool_dir: spool_dir.into(),
            timezone: timezone.into(),
            day_start_minutes: 0,
            user_rules: Vec::new(),
        }
    }
}

/// What one drain did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrainReport {
    /// Records read from the Spool.
    pub read: usize,
    /// Records ingested and counted.
    pub ingested: usize,
    /// Records rejected as duplicates.
    pub duplicates: usize,
    /// Records refused by the identity gate.
    pub refused: usize,
    /// Records that failed validation or storage.
    pub failed: usize,
    /// Records skipped as damaged before ingestion.
    pub damaged: usize,
    /// Outbox notifications created.
    pub queued_notifications: usize,
    /// Spool files moved to consumed.
    pub files_consumed: usize,
    /// Whether a drain should be retried soon (a record failed for a non-fatal reason).
    pub retry_needed: bool,
}

/// The Companion.
#[derive(Debug)]
pub struct Companion {
    pool: SqlitePool,
    config: CompanionConfig,
}

impl Companion {
    /// Open a Companion over an existing pool and configuration.
    pub fn new(pool: SqlitePool, config: CompanionConfig) -> Self {
        Companion { pool, config }
    }

    /// The pool, for diagnostics and for the UI layer.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// The configuration in force.
    pub fn config(&self) -> &CompanionConfig {
        &self.config
    }

    /// Swap in the user's current achievement rules.
    ///
    /// Rules are edited while the app runs, so the drain asks for them each time
    /// rather than taking a snapshot at startup: a rule the user just deleted must
    /// stop firing on the very next commit.
    pub fn replace_user_rules(
        &mut self,
        rules: Vec<devchievements_achievements::definition::UserAchievement>,
    ) {
        self.config.user_rules = rules;
    }

    /// Drain the Spool once.
    ///
    /// # Why this is not a loop
    ///
    /// The caller owns cadence. A background thread and a test both call this the same
    /// way, which is what makes the crash tests meaningful: they can stop between any
    /// two calls.
    pub async fn drain_once(&self, identity: &Identity) -> Result<DrainReport, SpoolError> {
        let spool = Spool::open(&self.config.spool_dir)?;
        let outcome = spool.read_all()?;

        let mut report = DrainReport {
            read: outcome.records.len(),
            damaged: outcome.damaged.len(),
            ..Default::default()
        };

        if outcome.records.is_empty() {
            return Ok(report);
        }

        for record in &outcome.records {
            match self.ingest_record(record, identity).await {
                Ok(IngestResult::Ingested { notifications }) => {
                    report.ingested += 1;
                    report.queued_notifications += notifications;
                }
                Ok(IngestResult::Duplicate) => report.duplicates += 1,
                Ok(IngestResult::Refused) => report.refused += 1,
                Err(reason) => {
                    // A storage failure is not the record's fault; leave the Spool
                    // file in place so nothing is lost and retry later.
                    // A discarded reason makes a failed record indistinguishable from
                    // a missing one. Keep the diagnostic available on demand.
                    if std::env::var_os("DEVCH_DRAIN_DEBUG").is_some() {
                        eprintln!("DRAIN RECORD FAILED: {reason}");
                    }
                    let _ = reason;
                    report.failed += 1;
                    report.retry_needed = true;
                }
            }
        }

        // Only consume the files when nothing failed. A partial failure means we want
        // the same records again on the next drain.
        if !report.retry_needed {
            report.files_consumed = spool.mark_consumed()?;
        }

        Ok(report)
    }

    async fn ingest_record(
        &self,
        record: &Record,
        identity: &Identity,
    ) -> Result<IngestResult, String> {
        let payload: serde_json::Value =
            serde_json::from_str(&record.payload).map_err(|e| format!("bad payload: {e}"))?;

        // Two record kinds arrive here: hook captures (the original path) and reports
        // forwarded by the browser extension through the Native Messaging host.
        // Both become events; neither is trusted beyond what its rule set accepts.
        let Some(event) = self.event_from_record_payload(&payload) else {
            return Err("payload did not describe a supported event".to_string());
        };

        let occurred = event.occurred_at;
        let age = (now_millis() - occurred).max(0);
        // Spec 16.3: age decides presentation, not whether the event counts.
        let fresh = freshness_action(age);

        // Only a commit may be counted as a commit. Every event used to reach
        // `evaluate_commit`, whose only input is `CommitFacts`, so a merged pull
        // request incremented the commit counter and could unlock "First Commit" --
        // measured: one browser PR produced repository_commit_count = 1.
        //
        // Non-commit events are recorded with facts that explicitly do not count, so
        // the Engine leaves the statistics alone and the event is still stored.
        let facts = if event.event_type == EventType::CommitCreated {
            ingest::commit_facts(
                payload
                    .get("parents")
                    .and_then(|p| p.as_array())
                    .map(|a| a.len())
                    .unwrap_or(1),
                false,
            )
        } else {
            devchievements_core::CommitFacts {
                counts_as_commit: false,
                is_merge: false,
                is_empty: false,
            }
        };

        let clock = SystemClock;
        let tz = BundledTimezoneProvider::new();
        let mut ids = UuidishIds::new();
        let cfg = StaticConfig;
        let ctx = InjectedContext {
            clock: &clock,
            timezone: &tz,
            ids: &mut ids,
            config: &cfg,
        };

        let _ = Engine; // the Engine is invoked inside ingest_commit

        // Spec 16.3: an event older than a day updates statistics silently. The policy
        // goes INTO storage so no outbox row is written at all -- a row that exists is
        // delivered, and suppressing only the count here used to leave stale
        // celebrations on screen while the drain report claimed "queued=0".
        //
        // "Digested" (10 minutes to 24 hours) still gets one full popup: the digest
        // presentation is not implemented, and inventing a half-version of it would
        // be worse than showing the celebration the user earned.
        let policy = match fresh {
            devchievements_notifications::FreshnessAction::Silent => {
                ingest::NotificationPolicy::Quiet
            }
            _ => ingest::NotificationPolicy::Inform,
        };

        match ingest::ingest_event_with_policy(
            &self.pool,
            &event,
            &facts,
            now_millis(),
            identity,
            &ctx,
            policy,
            &self.config.user_rules,
        )
        .await
        {
            Ok(IngestOutcome::Ingested {
                queued_notifications,
            }) => Ok(IngestResult::Ingested {
                notifications: queued_notifications.len(),
            }),
            Ok(IngestOutcome::IngestedSilently) => Ok(IngestResult::Ingested { notifications: 0 }),
            Ok(IngestOutcome::Duplicate) => Ok(IngestResult::Duplicate),
            Ok(IngestOutcome::Rejected(reason)) => Err(reason),
            Ok(IngestOutcome::Refused(_refusal)) => {
                // Spec 7 says refuse; it does not say "and forget it". A browser
                // report refused because the account is not linked is a setup step
                // the user can complete, so the report is parked and can be replayed.
                self.park_refused(&event, &payload, &record.payload).await;
                Ok(IngestResult::Refused)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Park a browser report the identity gate refused.
    ///
    /// Only browser reports are parked. A local commit refused for its author is a
    /// different problem -- the repository's `user.email` is wrong -- and replaying
    /// it would only produce the same refusal until that is fixed anyway.
    async fn park_refused(
        &self,
        event: &devchievements_events::Event,
        payload: &serde_json::Value,
        raw: &str,
    ) {
        if payload.get("kind").and_then(|k| k.as_str()) != Some("browser_github") {
            return;
        }
        let Some(login) = payload
            .get("author_login")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return;
        };

        // A parking failure must not turn a refusal into a storage failure: the
        // record is already excluded from statistics either way.
        let _ = devchievements_storage::refusals::park(
            &self.pool,
            event.event_id.as_str(),
            login,
            event.canonical_repository_id.as_str(),
            raw,
            now_millis(),
        )
        .await;
    }

    /// Build an event from what the Hook Helper wrote.
    /// Turn either record kind into an event.
    ///
    /// The kind is explicit in the payload rather than inferred from which fields are
    /// present: guessing would let a malformed hook record be read as a browser report.
    fn event_from_record_payload(&self, payload: &serde_json::Value) -> Option<Event> {
        match payload.get("kind").and_then(|k| k.as_str()) {
            Some("browser_github") => {
                let report = devchievements_git_hooks::browser_event::parse(payload)?;
                let clock = SystemClock;
                let tz = BundledTimezoneProvider::new();
                let mut ids = UuidishIds::new();
                let cfg = StaticConfig;
                let ctx = InjectedContext {
                    clock: &clock,
                    timezone: &tz,
                    ids: &mut ids,
                    config: &cfg,
                };
                devchievements_git_hooks::browser_event::to_event(&report, &ctx)
            }
            // Everything else keeps the original hook shape, including records written
            // before this field existed (they have no kind at all).
            _ => self.event_from_hook_payload(payload),
        }
    }

    fn event_from_hook_payload(&self, payload: &serde_json::Value) -> Option<Event> {
        let event_id = payload.get("event_id")?.as_str()?;
        let commit_sha = payload.get("commit_sha")?.as_str()?;
        let occurred_ms = payload.get("occurred_ms")?.as_i64()?;
        let git_dir = payload.get("repo_git_dir")?.as_str()?;
        let author_email = payload
            .get("author_email")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let hook = payload.get("hook")?.as_str()?;

        // Spec 6.1 priority: the remote decides identity, because the same repository
        // is cloned to different paths on different machines. The path is only a
        // fallback, and is kept distinguishable ("local:") so a split is visible in
        // diagnostics rather than looking like a second repository.
        let repository_id = CanonicalRepositoryId(
            match payload
                .get("repo_remote_url")
                .and_then(|v| v.as_str())
                .and_then(devchievements_events::repository::RepositoryIdentity::from_remote_url)
            {
                Some(identity) => identity.canonical_id,
                None => format!("local:{git_dir}"),
            },
        );

        let local_date = devchievements_core::local_date_at(
            occurred_ms,
            &self.config.timezone,
            devchievements_core::DayBoundary::from_minutes(self.config.day_start_minutes).ok()?,
        )?;

        let event_type = match hook {
            "post-merge" => EventType::LocalMerge,
            _ => EventType::CommitCreated,
        };

        Some(Event {
            event_id: EventId::new(event_id)?,
            event_type,
            source: EventSource::LocalGitHook,
            source_confidence: SourceConfidence::VerifiedLocal,
            canonical_repository_id: repository_id,
            external_id: None,
            commit_sha: Some(commit_sha.to_string()),
            occurred_at: occurred_ms,
            observed_at: occurred_ms,
            received_at: now_millis(),
            local_date: LocalDate::new(local_date)?,
            timezone: self.config.timezone.clone(),
            date_policy: self.config.day_start_minutes,
            payload: serde_json::json!({ "author_email": author_email }).to_string(),
            schema_version: SchemaVersion(1),
        })
    }

    /// Unconsumed outbox rows, oldest first.
    pub async fn pending_notifications(
        &self,
    ) -> Result<Vec<crate::delivery::PendingNotification>, String> {
        crate::delivery::pending(&self.pool)
            .await
            .map_err(|e| e.to_string())
    }

    /// Current counters, for diagnostics and tests.
    pub async fn statistics(&self, repository_id: &str) -> Result<(u32, u32), String> {
        let total = projections::repository_commit_count(&self.pool, repository_id)
            .await
            .map_err(|e| e.to_string())?;
        let events = ingest::event_count(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok((total, events as u32))
    }
}

enum IngestResult {
    Ingested { notifications: usize },
    Duplicate,
    Refused,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// Minimal injected dependencies for the headless loop. Deterministic ids are not
// required here because event ids come from the Hook, not from the Engine.
#[derive(Debug)]
struct SystemClock;
impl devchievements_core::ClockProvider for SystemClock {
    fn now_unix_millis(&self) -> i64 {
        now_millis()
    }
}

#[derive(Debug)]
struct UuidishIds(u64);
impl UuidishIds {
    fn new() -> Self {
        UuidishIds(0)
    }
}
impl devchievements_core::IdGenerator for UuidishIds {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("companion-{}", self.0)
    }
}

#[derive(Debug)]
struct StaticConfig;
impl devchievements_core::ConfigurationProvider for StaticConfig {
    fn config_version(&self) -> u64 {
        1
    }
}

/// Serve the live IPC channel forever, on a background thread.
///
/// Each accepted record is appended to the Spool BEFORE the acknowledgement is sent,
/// so an ack always means "durable". Ingestion then happens through the ordinary
/// drain path, which keeps exactly one place responsible for ordering and
/// idempotency.
///
/// Returns an error only if the channel could not be bound. That is reported rather
/// than silently disabling the fast path: a Companion nobody can reach makes every
/// commit take the Spool fallback.
pub fn serve_ipc_blocking(
    user_discriminator: String,
    spool_dir: PathBuf,
    on_record: impl Fn() + Send + 'static,
) -> Result<(), String> {
    use devchievements_git_hooks::ipc::Server;

    let server = Server::bind(&user_discriminator).map_err(|e| e.to_string())?;
    let spool = Spool::open(&spool_dir).map_err(|e| e.to_string())?;

    std::thread::spawn(move || loop {
        let mut accepted = false;
        let served = server.serve_one(|payload| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
                return false;
            };
            let Some(event_id) = value.get("event_id").and_then(|v| v.as_str()) else {
                return false;
            };
            match spool.append(event_id, payload) {
                Ok(()) => {
                    accepted = true;
                    true
                }
                Err(_) => false,
            }
        });

        if accepted {
            on_record();
        }
        if served.is_err() {
            // One bad connection must not stop the channel.
            continue;
        }
    });

    Ok(())
}

/// Whether a Spool directory exists yet.
pub fn spool_exists(dir: &Path) -> bool {
    dir.join("consumed").exists()
}

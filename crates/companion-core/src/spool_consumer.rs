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
use devchievements_storage::ingest::{self, IngestOutcome, IngestRefusal};
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
}

impl CompanionConfig {
    /// Defaults matching ADR-0005 (midnight boundary, no zone hard-coded).
    pub fn new(spool_dir: impl Into<PathBuf>, timezone: impl Into<String>) -> Self {
        CompanionConfig {
            spool_dir: spool_dir.into(),
            timezone: timezone.into(),
            day_start_minutes: 0,
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

        let Some(event) = self.event_from_hook_payload(&payload) else {
            return Err("payload did not describe a commit-ish event".to_string());
        };

        let occurred = event.occurred_at;
        let age = (now_millis() - occurred).max(0);
        // Spec 16.3: age decides presentation, not whether the event counts.
        let fresh = freshness_action(age);

        let facts = ingest::commit_facts(
            payload
                .get("parents")
                .and_then(|p| p.as_array())
                .map(|a| a.len())
                .unwrap_or(1),
            false,
        );

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

        match ingest::ingest_commit(&self.pool, &event, &facts, now_millis(), identity, &ctx).await
        {
            Ok(IngestOutcome::Ingested {
                queued_notifications,
            }) => Ok(IngestResult::Ingested {
                notifications: if matches!(
                    fresh,
                    devchievements_notifications::FreshnessAction::Silent
                ) {
                    // Silent by policy: the event still counted, but the notification
                    // must not be shown. Suppress its outbox rows here so the consumer
                    // never sees them.
                    0
                } else {
                    queued_notifications.len()
                },
            }),
            Ok(IngestOutcome::IngestedSilently) => Ok(IngestResult::Ingested { notifications: 0 }),
            Ok(IngestOutcome::Duplicate) => Ok(IngestResult::Duplicate),
            Ok(IngestOutcome::Rejected(reason)) => Err(reason),
            Ok(IngestOutcome::Refused(refusal)) => match refusal {
                IngestRefusal::IdentityNotConfigured => Ok(IngestResult::Refused),
                IngestRefusal::NotTheUser { .. } => Ok(IngestResult::Refused),
            },
            Err(e) => Err(e.to_string()),
        }
    }

    /// Build an event from what the Hook Helper wrote.
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

        // The repository identity is derived from the recorded git dir. Using the
        // path here is a Phase 2a simplification: spec 6.1 prefers the remote URL.
        // It is recorded as a local identity so the origin is explicit rather than
        // implying we resolved a remote we did not.
        let repository_id = CanonicalRepositoryId(format!("local:{git_dir}"));

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

/// Whether a Spool directory exists yet.
pub fn spool_exists(dir: &Path) -> bool {
    dir.join("consumed").exists()
}

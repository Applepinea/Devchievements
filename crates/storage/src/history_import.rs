//! History import (spec 11).
//!
//! # The failure mode this module exists to prevent
//!
//! Importing a repository's back catalogue naively produces a terrible experience:
//! hundreds of achievement popups and sounds for work done years ago. Spec 11.2 is
//! explicit that import is **silent**, that `local_date` comes from the commit's own
//! timestamp, and that the whole operation is **idempotent**.
//!
//! # Batching and resumability
//!
//! A large repository can have tens of thousands of commits. Import therefore runs
//! in batches, records each batch in `import_batches`, and can be resumed or
//! cancelled. Re-running a completed batch is a no-op.

use crate::StorageResult;
use devchievements_core::InjectedContext;
use devchievements_events::{Event, EventSource, SourceConfidence};
use sqlx::SqlitePool;

/// One commit to import, as read from the repository's history.
///
/// Deliberately minimal and clock-free: the caller supplies the commit's own time,
/// because spec 11.2 forbids using the import time as the event time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCommit {
    /// Commit sha.
    pub sha: String,
    /// Author email.
    pub author_email: String,
    /// Commit time in Unix epoch milliseconds.
    pub committed_at_ms: i64,
    /// Parent count, for the merge rule.
    pub parent_count: usize,
    /// Whether the commit's tree equals its first parent's tree.
    pub tree_equals_parent: bool,
}

/// Outcome of importing one batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchOutcome {
    /// Commits newly stored.
    pub imported: usize,
    /// Commits skipped because they were already present.
    pub duplicates: usize,
    /// Commits skipped because they do not belong to the configured identities.
    pub not_mine: usize,
    /// Commits skipped by the counting rules (merge commits, empty commits).
    pub excluded_by_rules: usize,
    /// Whether the batch was already marked complete.
    pub already_complete: bool,
}

/// Whether a commit belongs to the user (spec 7).
///
/// Email matching is an attribution heuristic, not authentication (spec 5.4); the
/// helper docstring says so because it is easy to over-trust.
pub fn author_matches(author_email: &str, identities: &[String]) -> bool {
    let needle = author_email.trim().to_ascii_lowercase();
    identities
        .iter()
        .any(|known| known.trim().to_ascii_lowercase() == needle)
}

/// Build the event for one imported commit.
///
/// `local_date` is computed from the COMMIT time using the configured zone and day
/// boundary, then frozen (spec 11.2 + 12).
pub fn build_import_event(
    repository_id: &str,
    commit: &ImportCommit,
    timezone: &str,
    day_start_minutes: u16,
    ctx: &InjectedContext<'_>,
) -> Option<Event> {
    let local_date =
        ctx.timezone
            .local_date(commit.committed_at_ms, timezone, day_start_minutes)?;
    let local_date = devchievements_events::LocalDate::new(local_date)?;

    Some(Event {
        event_id: devchievements_events::EventId::new(format!(
            "import:{repository_id}:{}",
            commit.sha
        ))?,
        event_type: devchievements_events::EventType::CommitCreated,
        source: EventSource::HistoryImport,
        // Spec 5.4: imported history is real but not observed live.
        source_confidence: SourceConfidence::Imported,
        canonical_repository_id: devchievements_events::CanonicalRepositoryId(
            repository_id.to_string(),
        ),
        external_id: None,
        commit_sha: Some(commit.sha.clone()),
        occurred_at: commit.committed_at_ms,
        observed_at: commit.committed_at_ms,
        received_at: commit.committed_at_ms,
        local_date,
        timezone: timezone.to_string(),
        date_policy: day_start_minutes,
        payload: format!("{{\"author_email\":\"{}\"}}", commit.author_email),
        schema_version: devchievements_events::SchemaVersion(1),
    })
}

/// Mark a batch as started (or return true if it had already completed).
pub async fn begin_batch(
    pool: &SqlitePool,
    batch_id: &str,
    repository_id: &str,
    now_ms: i64,
) -> StorageResult<bool> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT status FROM import_batches WHERE batch_id = ?")
            .bind(batch_id)
            .fetch_optional(pool)
            .await?;

    if let Some((status,)) = existing {
        if status == "completed" {
            return Ok(true);
        }
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO import_batches (batch_id, repository_id, started_at, finished_at, status,
            imported_events, skipped_duplicates)
         VALUES (?, ?, ?, NULL, 'pending', 0, 0)",
    )
    .bind(batch_id)
    .bind(repository_id)
    .bind(now_ms)
    .execute(pool)
    .await?;
    Ok(false)
}

/// Record the result of a batch.
pub async fn finish_batch(
    pool: &SqlitePool,
    batch_id: &str,
    outcome: &BatchOutcome,
    now_ms: i64,
) -> StorageResult<()> {
    sqlx::query(
        "UPDATE import_batches
         SET status = 'completed', finished_at = ?,
             imported_events = imported_events + ?,
             skipped_duplicates = skipped_duplicates + ?
         WHERE batch_id = ?",
    )
    .bind(now_ms)
    .bind(outcome.imported as i64)
    .bind(outcome.duplicates as i64)
    .bind(batch_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Import state of a batch, for resumability checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchState {
    /// pending | completed | cancelled
    pub status: String,
    /// Events imported so far.
    pub imported_events: i64,
    /// Duplicates skipped so far.
    pub skipped_duplicates: i64,
}

/// Read a batch's recorded state.
pub async fn batch_state(pool: &SqlitePool, batch_id: &str) -> StorageResult<Option<BatchState>> {
    let row = sqlx::query(
        "SELECT status, imported_events, skipped_duplicates FROM import_batches WHERE batch_id = ?",
    )
    .bind(batch_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| BatchState {
        status: sqlx::Row::get(&r, "status"),
        imported_events: sqlx::Row::get(&r, "imported_events"),
        skipped_duplicates: sqlx::Row::get(&r, "skipped_duplicates"),
    }))
}

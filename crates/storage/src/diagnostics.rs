//! Diagnostics snapshot (spec 32.1: minimal diagnostics view).
//!
//! # Why this is data, not a window
//!
//! Spec 32.1 asks for a minimal diagnostics view. The embedded webview cannot start on
//! this machine, so the UI half is blocked -- but the DATA half is not, and it is the
//! half that can silently be wrong. This module produces a plain snapshot; the CLI
//! prints it and the eventual window renders the same structure.
//!
//! # Privacy
//!
//! Spec 26.2 says privacy mode also applies to the diagnostics page, and that exports
//! default to redacted. Repository identifiers are therefore reduced to stable aliases
//! before they leave this module, and the caller redacts paths the same way.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::{ingest, projections, StorageResult};

/// One repository's contribution, identified by a stable alias.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySummary {
    /// Stable alias, never the raw identifier.
    pub alias: String,
    /// Valid commits counted for it.
    pub commits: u32,
}

/// Outbox rows in one status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxCount {
    /// Status name.
    pub status: String,
    /// How many rows are in it.
    pub count: i64,
}

/// A point-in-time snapshot of everything the diagnostics view needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    /// Schema version of the database.
    pub schema_version: i64,
    /// Total events recorded.
    pub events: i64,
    /// Repositories seen, aliased.
    pub repositories: Vec<RepositorySummary>,
    /// Unlocked achievements.
    pub unlocked_achievements: Vec<String>,
    /// Outbox rows grouped by status.
    pub outbox: Vec<OutboxCount>,
    /// Rows that reached a terminal state but were never shown.
    ///
    /// This is the number a user would complain about, so it is surfaced explicitly
    /// rather than left to be derived from the status table.
    pub notifications_never_shown: i64,
}

/// Collect a snapshot.
///
/// The alias callback maps a repository identifier to the string that may be shown.
/// Passing it in keeps this crate free of the privacy policy while still guaranteeing
/// that no raw identifier reaches the caller by accident.
pub async fn snapshot(
    pool: &SqlitePool,
    alias: impl Fn(&str) -> String,
) -> StorageResult<DiagnosticsSnapshot> {
    let events = ingest::event_count(pool).await?;

    let rows = sqlx::query(
        "SELECT canonical_repository_id, commit_count
         FROM repository_projection
         ORDER BY commit_count DESC, canonical_repository_id ASC",
    )
    .fetch_all(pool)
    .await?;

    let repositories = rows
        .into_iter()
        .map(|row| {
            let raw: String = row.get("canonical_repository_id");
            let count: i64 = row.get("commit_count");
            RepositorySummary {
                alias: alias(&raw),
                commits: count.max(0) as u32,
            }
        })
        .collect();

    let unlocked_achievements = projections::unlocked_achievement_ids(pool).await?;

    let outbox = ingest::outbox_counts(pool)
        .await?
        .into_iter()
        .map(|(status, count)| OutboxCount { status, count })
        .collect();

    // Spec 17.9: "never shown" is the failure mode users notice, so count it directly.
    let never_shown: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox
         WHERE status IN ('delivery_unknown','failed')",
    )
    .fetch_one(pool)
    .await?;

    // A database that has never been migrated has no recorded version; 0 says so
    // rather than failing the whole snapshot.
    let schema_version = ingest::recorded_schema_version(pool).await?.unwrap_or(0);

    Ok(DiagnosticsSnapshot {
        schema_version,
        events,
        repositories,
        unlocked_achievements,
        outbox,
        notifications_never_shown: never_shown,
    })
}

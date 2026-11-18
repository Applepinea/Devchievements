//! Storage adapter (SQLite + SQLx).
//!
//! # Boundary (AGENTS.md 6)
//!
//! Owns persistence: transactions, migrations, projections, the outbox table,
//! history import bookkeeping. Must NOT contain achievement judgement logic -- that
//! belongs to the Engine.
//!
//! # The one rule that shapes everything here
//!
//! The raw event, the projection updates it implies, and the notification intents it
//! produces commit in ONE transaction (spec 4.3, 16.1). Every public ingestion entry
//! point in this crate opens that transaction itself and never exposes a way to
//! write only part of it.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod backup;
pub mod diagnostics;
pub mod history_import;
pub mod history_scan;
pub mod indexeddb;
pub mod ingest;
pub mod observability;
pub mod projections;
pub mod read_models;
pub mod refusals;
pub mod schema;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Crate version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Errors surfaced by the storage layer.
#[derive(Debug)]
pub enum StorageError {
    /// Anything SQLx reported.
    Database(sqlx::Error),
    /// The database path or connection string was unusable.
    BadConnectionString(String),
    /// The containing directory could not be created.
    DirectoryUnavailable(String),
    /// The Engine emitted a projection this adapter does not know how to persist.
    ///
    /// Failing loudly is deliberate: silently dropping a state change would make
    /// the statistics quietly wrong, which is worse than an error.
    UnknownProjection(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Database(e) => write!(f, "database error: {e}"),
            StorageError::BadConnectionString(s) => write!(f, "bad connection string: {s}"),
            StorageError::DirectoryUnavailable(s) => write!(f, "cannot use directory: {s}"),
            StorageError::UnknownProjection(p) => {
                write!(f, "engine emitted unknown projection: {p}")
            }
        }
    }
}

impl std::error::Error for StorageError {}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        StorageError::Database(e)
    }
}

/// Convenience alias.
pub type StorageResult<T> = Result<T, StorageError>;

/// Open (and create if needed) a SQLite database at `path`.
pub async fn open(path: &std::path::Path) -> StorageResult<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| StorageError::DirectoryUnavailable(parent.display().to_string()))?;
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|e| StorageError::BadConnectionString(e.to_string()))?
        .create_if_missing(true)
        // WAL: the Hook Helper writes while the UI reads, and neither may block the
        // other.
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    Ok(pool)
}

#[cfg(test)]
mod tests;

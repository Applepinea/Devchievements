//! Storage adapter (SQLite + SQLx).
//!
//! # Boundary (AGENTS.md 6)
//!
//! This crate owns persistence: transactions, migrations, projections, the outbox
//! table. It must NOT contain achievement judgement logic -- that belongs to the
//! Engine. The adapter's job is to apply the Engine's `state_changes` and
//! `side_effect_intents` atomically (spec 4.3): one transaction, so an event and
//! the notifications it implies can never be half-written.
//!
//! # Phase 0 scope
//!
//! Only enough to prove the toolchain works end to end: open a database, create
//! the schema, write a row, read it back, roll back on error, cascade on delete.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Executor, SqlitePool};
use std::str::FromStr;

/// Crate version, recorded alongside persisted data so migrations and
/// diagnostics can attribute results to the code that produced them.
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
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Database(e) => write!(f, "database error: {e}"),
            StorageError::BadConnectionString(s) => write!(f, "bad connection string: {s}"),
            StorageError::DirectoryUnavailable(s) => write!(f, "cannot use directory: {s}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        StorageError::Database(e)
    }
}

/// Open (and create if needed) a SQLite database at `path`.
pub async fn open(path: &std::path::Path) -> Result<SqlitePool, StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| StorageError::DirectoryUnavailable(parent.display().to_string()))?;
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|e| StorageError::BadConnectionString(e.to_string()))?
        .create_if_missing(true)
        // WAL: readers must not block the writer, because the Hook Helper writes
        // while the UI reads.
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// Apply the Phase 0 schema.
///
/// Plain SQL rather than `sqlx::migrate!`: Phase 0 has no stable schema to
/// migrate from, and a compiled-in migration directory would imply stability we
/// have not earned yet.
pub async fn apply_initial_schema(pool: &SqlitePool) -> Result<(), StorageError> {
    pool.execute(SQLITE_SCHEMA).await?;
    Ok(())
}

/// Minimal Phase 0 schema: enough to prove round-trip and transactional
/// behaviour. The real data model is an ADR + Phase 1 deliverable.
///
/// The shape already honours two decisions that will not change:
/// - events and derived statistics are separate concerns (spec 4.1);
/// - the outbox lives in the same database, so a notification intent and its
///   event commit together (spec 16.1).
pub const SQLITE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    event_id        TEXT PRIMARY KEY NOT NULL,
    event_type      TEXT NOT NULL,
    source          TEXT NOT NULL,
    canonical_repository_id TEXT NOT NULL,
    commit_sha      TEXT,
    occurred_at     INTEGER NOT NULL,
    observed_at     INTEGER NOT NULL,
    received_at     INTEGER NOT NULL,
    local_date      TEXT NOT NULL,
    timezone        TEXT NOT NULL,
    date_policy     INTEGER NOT NULL,
    payload         TEXT NOT NULL,
    schema_version  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_repo_date
    ON events (canonical_repository_id, local_date);

CREATE TABLE IF NOT EXISTS outbox (
    notification_id TEXT PRIMARY KEY NOT NULL,
    event_id        TEXT NOT NULL,
    status          TEXT NOT NULL,
    owner_channel   TEXT,
    claimer_id      TEXT,
    offer_deadline  INTEGER,
    created_at      INTEGER NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    processed_at    INTEGER,
    FOREIGN KEY (event_id) REFERENCES events (event_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_outbox_status ON outbox (status);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_pool(name: &str) -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("devch-storage-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let pool = open(&dir.join("test.db")).await.expect("open must succeed");
        apply_initial_schema(&pool)
            .await
            .expect("schema must apply");
        pool
    }

    async fn insert_event(pool: &SqlitePool, id: &'static str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO events (event_id, event_type, source, canonical_repository_id,
                commit_sha, occurred_at, observed_at, received_at, local_date, timezone,
                date_policy, payload, schema_version)
             VALUES (?, 'commit_created', 'verified_local', 'repo-1',
                'c0ffee', 0, 1, 2, '2026-11-16', 'Asia/Taipei', 0, '{}', 1)",
        )
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
    }

    #[tokio::test]
    async fn writes_and_reads_an_event() {
        let pool = temp_pool("roundtrip").await;
        insert_event(&pool, "evt-1").await.expect("insert");

        let (count, local_date): (i64, String) =
            sqlx::query_as("SELECT COUNT(*), MAX(local_date) FROM events")
                .fetch_one(&pool)
                .await
                .expect("select");
        assert_eq!(count, 1);
        assert_eq!(local_date, "2026-11-16");
    }

    #[tokio::test]
    async fn duplicate_event_id_is_rejected() {
        // Event ids double as idempotency keys (spec 5.3): the primary key is the
        // mechanism that makes redelivery harmless.
        let pool = temp_pool("idempotent").await;
        insert_event(&pool, "evt-dup").await.expect("first insert");
        assert!(
            insert_event(&pool, "evt-dup").await.is_err(),
            "duplicate event id must be rejected, not silently accepted"
        );
    }

    #[tokio::test]
    async fn failed_transaction_leaves_no_partial_write() {
        // Spec 4.3 / 16.1: event, projection and outbox intent commit together or
        // not at all.
        let pool = temp_pool("txn").await;
        let mut tx = pool.begin().await.expect("begin");

        sqlx::query(
            "INSERT INTO events (event_id, event_type, source, canonical_repository_id,
                occurred_at, observed_at, received_at, local_date, timezone,
                date_policy, payload, schema_version)
             VALUES ('evt-tx', 'commit_created', 'verified_local', 'repo-1',
                0, 0, 0, '2026-11-16', 'UTC', 0, '{}', 1)",
        )
        .execute(&mut *tx)
        .await
        .expect("insert inside tx");

        // Foreign key violation: the referenced event does not exist.
        let bad = sqlx::query(
            "INSERT INTO outbox (notification_id, event_id, status, created_at)
             VALUES ('n-1', 'does-not-exist', 'created', 0)",
        )
        .execute(&mut *tx)
        .await;
        assert!(bad.is_err(), "foreign key violation must be reported");

        tx.rollback().await.expect("rollback");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(
            count.0, 0,
            "rolled back transaction must leave nothing behind"
        );
    }

    #[tokio::test]
    async fn deleting_an_event_cascades_to_its_outbox_rows() {
        let pool = temp_pool("cascade").await;
        insert_event(&pool, "evt-c").await.expect("insert event");
        sqlx::query(
            "INSERT INTO outbox (notification_id, event_id, status, created_at)
             VALUES ('n-c', 'evt-c', 'created', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert outbox");

        sqlx::query("DELETE FROM events WHERE event_id = 'evt-c'")
            .execute(&pool)
            .await
            .expect("delete event");

        let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM outbox")
            .fetch_one(&pool)
            .await
            .expect("count outbox");
        assert_eq!(remaining.0, 0, "outbox rows must not outlive their event");
    }

    #[tokio::test]
    async fn reopening_an_existing_database_keeps_data() {
        // Crash-recovery adjacent: the file must survive a pool drop.
        let dir = std::env::temp_dir().join(format!("devch-storage-reopen-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let path = dir.join("persist.db");

        {
            let pool = open(&path).await.expect("open");
            apply_initial_schema(&pool).await.expect("schema");
            insert_event(&pool, "evt-persist").await.expect("insert");
            pool.close().await;
        }

        let pool = open(&path).await.expect("reopen");
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count.0, 1, "data must survive closing and reopening");

        std::fs::remove_dir_all(&dir).ok();
    }
}

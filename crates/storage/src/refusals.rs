//! Browser reports refused because the GitHub account is not linked (spec 7).
//!
//! # Why a refused report is kept instead of dropped
//!
//! Spec 7 refuses to attribute work without a confirmed identity, which is correct:
//! counting someone else's pull request would inflate every statistic. But refusing
//! by *deleting* the report turns a fixable setup step into permanent data loss, and
//! turns "you have not linked your GitHub account" into "nothing happened".
//!
//! So a refused browser report is parked here -- with the raw payload, so it can be
//! replayed once the user links the account -- and the UI can say what is waiting.

use crate::StorageResult;
use sqlx::{Row, SqlitePool};

/// One refused report, kept verbatim for replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkedReport {
    /// The event id the report would have had; also the replay key.
    pub event_id: String,
    /// The GitHub login the page named.
    pub login: String,
    /// Which repository the activity happened in.
    pub repository_id: String,
    /// The original payload, byte for byte.
    pub payload: String,
    /// When the Companion refused it.
    pub received_at: i64,
}

/// One unlinked account, aggregated for display.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlinkedLogin {
    /// The GitHub login the page named.
    pub login: String,
    /// How many reports are waiting for this account.
    pub reports: i64,
    /// When the newest one arrived, Unix epoch milliseconds.
    pub last_received_at: i64,
}

/// Park a refused report. Repeats of the same event id are ignored.
pub async fn park(
    pool: &SqlitePool,
    event_id: &str,
    login: &str,
    repository_id: &str,
    payload: &str,
    now_ms: i64,
) -> StorageResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO browser_reports_rejected
            (event_id, login, repository_id, payload, received_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(event_id)
    .bind(login)
    .bind(repository_id)
    .bind(payload)
    .bind(now_ms)
    .execute(pool)
    .await?;
    Ok(())
}

/// Every unlinked account with something waiting, newest first.
pub async fn unlinked_logins(pool: &SqlitePool) -> StorageResult<Vec<UnlinkedLogin>> {
    let rows = sqlx::query(
        "SELECT login, COUNT(*) AS reports, MAX(received_at) AS newest
         FROM browser_reports_rejected
         GROUP BY login
         ORDER BY newest DESC, login ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| UnlinkedLogin {
            login: row.get("login"),
            reports: row.get("reports"),
            last_received_at: row.get("newest"),
        })
        .collect())
}

/// The reports waiting for one account, oldest first so replay keeps the order.
pub async fn parked_for_login(pool: &SqlitePool, login: &str) -> StorageResult<Vec<ParkedReport>> {
    let rows = sqlx::query(
        "SELECT event_id, login, repository_id, payload, received_at
         FROM browser_reports_rejected
         WHERE login = ?
         ORDER BY received_at ASC, event_id ASC",
    )
    .bind(login)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ParkedReport {
            event_id: row.get("event_id"),
            login: row.get("login"),
            repository_id: row.get("repository_id"),
            payload: row.get("payload"),
            received_at: row.get("received_at"),
        })
        .collect())
}

/// Drop parked reports, by event id.
pub async fn forget(pool: &SqlitePool, event_ids: &[String]) -> StorageResult<u64> {
    let mut removed = 0u64;
    for event_id in event_ids {
        let result = sqlx::query("DELETE FROM browser_reports_rejected WHERE event_id = ?")
            .bind(event_id)
            .execute(pool)
            .await?;
        removed += result.rows_affected();
    }
    Ok(removed)
}

/// How many reports are parked in total. Used by diagnostics.
pub async fn parked_count(pool: &SqlitePool) -> StorageResult<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM browser_reports_rejected")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

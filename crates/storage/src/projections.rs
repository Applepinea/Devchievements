//! Projection reads and writes.
//!
//! # What a projection is
//!
//! A cached aggregate derived from events. It exists so that evaluating one commit
//! costs O(1) reads rather than O(history) (spec 4.3). Because it is derived, it is
//! always rebuildable from `events`; that is the property that makes it safe to
//! change the derivation later.
//!
//! # Why every write records timezone and day boundary
//!
//! Spec 12 allows the user to change the timezone and the day boundary. If a
//! projection row did not record which policy produced it, "why did my daily count
//! change?" would be unanswerable. Recording it turns an argument into a query.

use crate::{StorageError, StorageResult};
use devchievements_core::StateChange;
use devchievements_daily_rank::DailyRank;
use devchievements_events::Event;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

/// One day's projection for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyProjection {
    /// Valid commits counted that day.
    pub commit_count: u32,
    /// The rank granted that day, if any.
    pub daily_rank: Option<DailyRank>,
    /// The timezone in force when the row was written.
    pub timezone: String,
    /// The day boundary in force.
    pub date_policy: u16,
}

/// Read one day's projection, if it exists.
pub async fn daily_projection(
    pool: &SqlitePool,
    repository_id: &str,
    local_date: &str,
) -> StorageResult<Option<DailyProjection>> {
    let row = sqlx::query(
        "SELECT commit_count, daily_rank, timezone, date_policy
         FROM daily_projection WHERE canonical_repository_id = ? AND local_date = ?",
    )
    .bind(repository_id)
    .bind(local_date)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let rank_text: Option<String> = r.get("daily_rank");
        DailyProjection {
            commit_count: r.get::<i64, _>("commit_count") as u32,
            daily_rank: rank_text.as_deref().and_then(parse_rank),
            timezone: r.get("timezone"),
            date_policy: r.get::<i64, _>("date_policy") as u16,
        }
    }))
}

/// The repository-wide commit total.
pub async fn repository_commit_count(pool: &SqlitePool, repository_id: &str) -> StorageResult<u32> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT commit_count FROM repository_projection WHERE canonical_repository_id = ?",
    )
    .bind(repository_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(n,)| n as u32).unwrap_or(0))
}

/// Every unlocked achievement id.
pub async fn unlocked_achievement_ids(pool: &SqlitePool) -> StorageResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT achievement_id FROM unlocked_achievements")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Apply the Engine's state changes inside an existing transaction.
///
/// The transaction is the caller's: the Engine output must commit together with the
/// event row and the outbox intents, never on its own (spec 4.3).
pub async fn apply_state_changes(
    tx: &mut Transaction<'_, Sqlite>,
    event: &Event,
    changes: &[StateChange],
    now_ms: i64,
) -> StorageResult<()> {
    for change in changes {
        match change.projection.as_str() {
            "repository_commit_count" => {
                let value: i64 = change.payload.parse().unwrap_or(0);
                sqlx::query(
                    "INSERT INTO repository_projection
                        (canonical_repository_id, commit_count, first_seen_at, updated_at)
                     VALUES (?, ?, ?, ?)
                     ON CONFLICT(canonical_repository_id) DO UPDATE SET
                        commit_count = excluded.commit_count,
                        updated_at   = excluded.updated_at",
                )
                .bind(event.canonical_repository_id.as_str())
                .bind(value)
                .bind(now_ms)
                .bind(now_ms)
                .execute(&mut **tx)
                .await?;
            }

            "daily_commit_count" => {
                let value: i64 = change.payload.parse().unwrap_or(0);
                sqlx::query(
                    "INSERT INTO daily_projection
                        (canonical_repository_id, local_date, commit_count, daily_rank,
                         timezone, date_policy, updated_at)
                     VALUES (?, ?, ?, NULL, ?, ?, ?)
                     ON CONFLICT(canonical_repository_id, local_date) DO UPDATE SET
                        commit_count = excluded.commit_count,
                        timezone     = excluded.timezone,
                        date_policy  = excluded.date_policy,
                        updated_at   = excluded.updated_at",
                )
                .bind(event.canonical_repository_id.as_str())
                .bind(event.local_date.as_str())
                .bind(value)
                .bind(&event.timezone)
                .bind(i64::from(event.date_policy))
                .bind(now_ms)
                .execute(&mut **tx)
                .await?;
            }

            "daily_rank_today" => {
                sqlx::query(
                    "UPDATE daily_projection SET daily_rank = ?, updated_at = ?
                     WHERE canonical_repository_id = ? AND local_date = ?",
                )
                .bind(&change.payload)
                .bind(now_ms)
                .bind(event.canonical_repository_id.as_str())
                .bind(event.local_date.as_str())
                .execute(&mut **tx)
                .await?;
            }

            other => {
                // An unknown projection means the Engine and the adapter disagree
                // about the data model. Failing loudly beats silently dropping a
                // state change.
                return Err(StorageError::UnknownProjection(other.to_string()));
            }
        }
    }
    Ok(())
}

/// Record a daily milestone, if not already recorded for that date.
pub async fn record_daily_milestone(
    tx: &mut Transaction<'_, Sqlite>,
    milestone_id: &str,
    local_date: &str,
    threshold: u32,
    reached_at: i64,
) -> StorageResult<bool> {
    let res = sqlx::query(
        "INSERT OR IGNORE INTO daily_milestones
            (milestone_id, local_date, threshold, reached_at, viewed_at)
         VALUES (?, ?, ?, ?, NULL)",
    )
    .bind(milestone_id)
    .bind(local_date)
    .bind(i64::from(threshold))
    .bind(reached_at)
    .execute(&mut **tx)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Rebuild every projection from the raw event log.
///
/// This is the property that justifies storing projections at all: they are
/// derived, so changing the derivation is always possible. It is also what spec 12
/// needs when the user asks to recompute history after a timezone change.
///
/// Returns the number of events replayed.
pub async fn rebuild_from_events(pool: &SqlitePool, now_ms: i64) -> StorageResult<i64> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM daily_projection")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM repository_projection")
        .execute(&mut *tx)
        .await?;

    // Ordered by occurrence so per-day counts accumulate the same way they would
    // have during live ingestion.
    let rows = sqlx::query(
        "SELECT canonical_repository_id, local_date, timezone, date_policy
         FROM events
         WHERE event_type = 'CommitCreated'
         ORDER BY occurred_at ASC, event_id ASC",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut replayed = 0i64;
    let mut repo_totals: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    let mut day_counts: std::collections::BTreeMap<(String, String), (i64, String, i64)> =
        std::collections::BTreeMap::new();

    for row in rows {
        let repo: String = row.get("canonical_repository_id");
        let date: String = row.get("local_date");
        let tz: String = row.get("timezone");
        let policy: i64 = row.get("date_policy");

        *repo_totals.entry(repo.clone()).or_insert(0) += 1;
        let entry = day_counts.entry((repo, date)).or_insert((0, tz, policy));
        entry.0 += 1;
        replayed += 1;
    }

    for (repo, total) in &repo_totals {
        sqlx::query(
            "INSERT INTO repository_projection
                (canonical_repository_id, commit_count, first_seen_at, updated_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(repo)
        .bind(total)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    }

    for ((repo, date), (count, tz, policy)) in &day_counts {
        let rank =
            devchievements_daily_rank::DailyRankThresholds::official().rank_for(*count as u32);
        sqlx::query(
            "INSERT INTO daily_projection
                (canonical_repository_id, local_date, commit_count, daily_rank,
                 timezone, date_policy, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(repo)
        .bind(date)
        .bind(count)
        .bind(rank.map(|r| r.as_str()))
        .bind(tz)
        .bind(policy)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(replayed)
}

/// Parse a persisted rank name.
pub fn parse_rank(text: &str) -> Option<DailyRank> {
    match text {
        "common" => Some(DailyRank::Common),
        "uncommon" => Some(DailyRank::Uncommon),
        "rare" => Some(DailyRank::Rare),
        "epic" => Some(DailyRank::Epic),
        "legendary" => Some(DailyRank::Legendary),
        _ => None,
    }
}

//! Read models for the desktop pages (spec 34.1).
//!
//! # Why the queries live here and not in the UI layer
//!
//! Every page must read the real database (spec 34.2). Keeping the SQL in one module
//! means a page cannot accidentally invent a number, and each query can be tested
//! against a real SQLite file rather than mocked.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::StorageResult;

/// One achievement as the gallery shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalleryEntry {
    /// Stable id.
    pub achievement_id: String,
    /// Whether it has been unlocked.
    pub unlocked: bool,
    /// When it unlocked, if it did.
    pub unlocked_at: Option<i64>,
    /// Where the unlock came from: realtime, delayed, backfill or import (spec 13.4).
    pub unlock_source: Option<String>,
    /// Rule version in force at unlock time.
    pub rule_version: Option<i64>,
    /// Whether the user has seen it. This drives the new marker (spec 34.2).
    pub viewed: bool,
    /// The event that triggered it, when there was one.
    pub triggering_event_id: Option<String>,
}

/// One day of activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityDay {
    /// Local date, already frozen at ingestion time (spec 12).
    pub local_date: String,
    /// Valid commits that day.
    pub commits: i64,
    /// Daily rank reached, if any.
    pub rank: Option<String>,
}

/// One repository in the manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryEntry {
    /// Stable alias, never the raw identifier (spec 26.2).
    pub alias: String,
    /// Valid commits counted.
    pub commits: i64,
    /// First time this repository contributed.
    pub first_seen_at: i64,
    /// Last time it contributed.
    pub updated_at: i64,
}

/// Aggregate statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statistics {
    /// Total events recorded.
    pub events: i64,
    /// Total valid commits across repositories.
    pub commits: i64,
    /// Number of repositories.
    pub repositories: i64,
    /// Number of unlocked achievements.
    pub achievements: i64,
    /// Achievements unlocked but not yet viewed.
    pub unviewed_achievements: i64,
    /// Longest run of consecutive active days.
    pub longest_streak: i64,
    /// Days with at least one valid commit.
    pub active_days: i64,
}

/// Health of the database file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageHealth {
    /// Path, already redacted by the caller.
    pub path: String,
    /// Size in bytes.
    pub bytes: i64,
    /// Schema version recorded in the database.
    pub schema_version: i64,
    /// Oldest event, so growth can be reasoned about.
    pub oldest_event_at: Option<i64>,
}

/// Every unlocked achievement plus the ones present in the catalogue but not unlocked.
///
/// The caller supplies the known ids so the gallery can show locked entries as goals;
/// the storage layer does not know the achievement rules and should not pretend to.
pub async fn gallery(pool: &SqlitePool, catalogue: &[String]) -> StorageResult<Vec<GalleryEntry>> {
    let rows = sqlx::query(
        "SELECT achievement_id, unlocked_at, unlock_source, rule_version,
                viewed_at, triggering_event_id
         FROM unlocked_achievements
         ORDER BY unlocked_at DESC, achievement_id ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut entries: Vec<GalleryEntry> = rows
        .into_iter()
        .map(|row| GalleryEntry {
            achievement_id: row.get("achievement_id"),
            unlocked: true,
            unlocked_at: Some(row.get("unlocked_at")),
            unlock_source: Some(row.get("unlock_source")),
            rule_version: Some(row.get("rule_version")),
            viewed: row.get::<Option<i64>, _>("viewed_at").is_some(),
            triggering_event_id: row.get("triggering_event_id"),
        })
        .collect();

    for id in catalogue {
        if !entries.iter().any(|e| &e.achievement_id == id) {
            entries.push(GalleryEntry {
                achievement_id: id.clone(),
                unlocked: false,
                unlocked_at: None,
                unlock_source: None,
                rule_version: None,
                viewed: false,
                triggering_event_id: None,
            });
        }
    }

    Ok(entries)
}

/// Mark achievements as seen, clearing their new marker.
pub async fn mark_viewed(pool: &SqlitePool, ids: &[String], now_ms: i64) -> StorageResult<u64> {
    let mut affected = 0u64;
    for id in ids {
        let res = sqlx::query(
            "UPDATE unlocked_achievements SET viewed_at = ? WHERE achievement_id = ? AND viewed_at IS NULL",
        )
        .bind(now_ms)
        .bind(id)
        .execute(pool)
        .await?;
        affected += res.rows_affected();
    }
    Ok(affected)
}

/// Recent activity, newest first.
pub async fn activity(pool: &SqlitePool, limit: i64) -> StorageResult<Vec<ActivityDay>> {
    let rows = sqlx::query(
        "SELECT local_date, SUM(commit_count) AS commits, MAX(daily_rank) AS rank
         FROM daily_projection
         GROUP BY local_date
         ORDER BY local_date DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ActivityDay {
            local_date: row.get("local_date"),
            commits: row.get::<i64, _>("commits"),
            rank: row.get("rank"),
        })
        .collect())
}

/// Repositories the companion has seen, aliased for display.
pub async fn repositories(
    pool: &SqlitePool,
    alias: impl Fn(&str) -> String,
) -> StorageResult<Vec<RepositoryEntry>> {
    let rows = sqlx::query(
        "SELECT canonical_repository_id, commit_count, first_seen_at, updated_at
         FROM repository_projection
         ORDER BY commit_count DESC, canonical_repository_id ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let raw: String = row.get("canonical_repository_id");
            RepositoryEntry {
                alias: alias(&raw),
                commits: row.get("commit_count"),
                first_seen_at: row.get("first_seen_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect())
}

/// Aggregate statistics for the dashboard and the statistics page.
pub async fn statistics(pool: &SqlitePool) -> StorageResult<Statistics> {
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await?;
    let commits: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(commit_count), 0) FROM repository_projection")
            .fetch_one(pool)
            .await?;
    let repositories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repository_projection")
        .fetch_one(pool)
        .await?;
    let achievements: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM unlocked_achievements")
        .fetch_one(pool)
        .await?;
    let unviewed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM unlocked_achievements WHERE viewed_at IS NULL")
            .fetch_one(pool)
            .await?;

    let dates: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT local_date FROM daily_projection WHERE commit_count > 0 ORDER BY local_date ASC",
    )
    .fetch_all(pool)
    .await?;

    let (active_days, longest_streak) = streak_stats(&dates);

    Ok(Statistics {
        events,
        commits,
        repositories,
        achievements,
        unviewed_achievements: unviewed,
        longest_streak,
        active_days,
    })
}

/// Count active days and the longest run of consecutive dates.
///
/// Kept separate and pure so the streak rule can be tested without a database. Dates
/// are ISO calendar dates (YYYY-MM-DD), which sort lexicographically.
fn streak_stats(sorted_dates: &[String]) -> (i64, i64) {
    if sorted_dates.is_empty() {
        return (0, 0);
    }

    let mut longest = 1i64;
    let mut current = 1i64;
    for pair in sorted_dates.windows(2) {
        let (Some(prev), Some(next)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        if prev == next {
            continue;
        }
        if is_next_day(prev, next) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 1;
        }
    }

    (sorted_dates.len() as i64, longest)
}

/// Whether next is the calendar day after prev.
fn is_next_day(prev: &str, next: &str) -> bool {
    let parse = |s: &str| -> Option<(i32, u32, u32)> {
        let mut parts = s.split('-');
        Some((
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        ))
    };
    let (Some(a), Some(b)) = (parse(prev), parse(next)) else {
        return false;
    };
    next_day(a) == b
}

/// The calendar day after the given date, computed without a date library.
fn next_day((year, month, day): (i32, u32, u32)) -> (i32, u32, u32) {
    const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let limit = if month == 2 && leap {
        29
    } else {
        MONTH_DAYS[(month as usize).saturating_sub(1).min(11)]
    };

    if day < limit {
        return (year, month, day + 1);
    }
    if month == 12 {
        return (year + 1, 1, 1);
    }
    (year, month + 1, 1)
}

/// Database file health.
pub async fn storage_health(pool: &SqlitePool, path: String) -> StorageResult<StorageHealth> {
    let schema_version = crate::ingest::recorded_schema_version(pool)
        .await?
        .unwrap_or(0);
    let oldest_event_at: Option<i64> = sqlx::query_scalar("SELECT MIN(occurred_at) FROM events")
        .fetch_one(pool)
        .await?;
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;

    Ok(StorageHealth {
        path,
        bytes: page_count.saturating_mul(page_size),
        schema_version,
        oldest_event_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn an_empty_history_has_no_streak() {
        assert_eq!(streak_stats(&[]), (0, 0));
    }

    #[test]
    fn a_single_day_is_a_streak_of_one() {
        assert_eq!(streak_stats(&days(&["2026-01-01"])), (1, 1));
    }

    #[test]
    fn consecutive_days_extend_the_streak() {
        assert_eq!(
            streak_stats(&days(&["2026-01-01", "2026-01-02", "2026-01-03"])),
            (3, 3)
        );
    }

    #[test]
    fn a_gap_resets_the_streak() {
        assert_eq!(
            streak_stats(&days(&["2026-01-01", "2026-01-02", "2026-01-05"])),
            (3, 2)
        );
    }

    #[test]
    fn the_longest_run_wins_not_the_last() {
        assert_eq!(
            streak_stats(&days(&[
                "2026-01-01",
                "2026-01-02",
                "2026-01-03",
                "2026-01-10",
                "2026-01-11",
            ])),
            (5, 3)
        );
    }

    #[test]
    fn month_end_rolls_over_correctly() {
        // A naive increment would treat 2026-01-31 to 2026-02-01 as a gap.
        assert_eq!(
            streak_stats(&days(&["2026-01-30", "2026-01-31", "2026-02-01"])),
            (3, 3)
        );
    }

    #[test]
    fn the_leap_day_is_handled() {
        assert_eq!(
            streak_stats(&days(&["2024-02-28", "2024-02-29", "2024-03-01"])),
            (3, 3)
        );
        assert!(is_next_day("2026-02-28", "2026-03-01"));
        assert!(!is_next_day("2026-02-28", "2026-02-29"));
    }

    #[test]
    fn year_end_rolls_over_correctly() {
        assert_eq!(streak_stats(&days(&["2025-12-31", "2026-01-01"])), (2, 2));
    }

    #[test]
    fn a_malformed_date_is_not_treated_as_consecutive() {
        assert!(!is_next_day("not-a-date", "2026-01-02"));
        assert!(!is_next_day("2026-01-01", "2026-13-40"));
    }
}

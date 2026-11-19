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

/// One day across every repository (spec 14.1, checklist T1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDay {
    /// Valid commits counted that day, across all repositories.
    pub commit_count: u32,
    /// The rank granted for that day, if any.
    pub daily_rank: Option<DailyRank>,
}

/// Read one day's GLOBAL projection, if any commit happened that day.
///
/// The daily rank is about the user's day, not one repository's (spec 14.1): three
/// commits in A plus two in B plus one in C is a six-commit day. Reading a single
/// repository's row is how that used to come out as Common.
pub async fn daily_global_projection(
    pool: &SqlitePool,
    local_date: &str,
) -> StorageResult<Option<GlobalDay>> {
    let row = sqlx::query(
        "SELECT commit_count, daily_rank FROM daily_global_projection WHERE local_date = ?",
    )
    .bind(local_date)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let rank_text: Option<String> = r.get("daily_rank");
        GlobalDay {
            commit_count: r.get::<i64, _>("commit_count") as u32,
            daily_rank: rank_text.as_deref().and_then(parse_rank),
        }
    }))
}

/// The sum of every repository's commit counter (spec 13.2, checklist T1).
///
/// Achievements about totals are about the user, so "Century" means a hundred commits
/// anywhere, not a hundred in one working copy.
pub async fn global_commit_total(pool: &SqlitePool) -> StorageResult<u32> {
    let row: (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(commit_count), 0) FROM repository_projection")
            .fetch_one(pool)
            .await?;
    Ok(row.0 as u32)
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

/// The user's GitHub totals across every tracked repository (spec 2.3).
///
/// Summed rather than stored on a single row: the achievements are about the user,
/// but each activity happened in one repository, and keeping the per-repository
/// figure is what makes "where did this come from?" answerable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BrowserTotals {
    /// Merged pull requests.
    pub pull_request_merged_count: u32,
    /// Releases published.
    pub release_count: u32,
    /// Issues opened.
    pub issue_opened_count: u32,
    /// Repositories created.
    pub repositories_created_count: u32,
    /// Stars given.
    pub star_given_count: u32,
}

impl BrowserTotals {
    /// Fill the browser fields of a snapshot.
    pub fn apply_to(self, snapshot: &mut devchievements_core::StateSnapshot) {
        snapshot.pull_request_merged_count = self.pull_request_merged_count;
        snapshot.release_count = self.release_count;
        snapshot.issue_opened_count = self.issue_opened_count;
        snapshot.repositories_created_count = self.repositories_created_count;
        snapshot.stars_given_count = self.star_given_count;
    }
}

/// Read the user's GitHub totals.
pub async fn browser_totals(pool: &SqlitePool) -> StorageResult<BrowserTotals> {
    let row = sqlx::query(
        "SELECT
            COALESCE(SUM(pull_request_merged_count), 0)   AS pr,
            COALESCE(SUM(release_count), 0)               AS releases,
            COALESCE(SUM(issue_opened_count), 0)          AS issues,
            COALESCE(SUM(repositories_created_count), 0)  AS repos,
            COALESCE(SUM(star_given_count), 0)            AS stars
         FROM repository_projection",
    )
    .fetch_one(pool)
    .await?;

    Ok(BrowserTotals {
        pull_request_merged_count: row.get::<i64, _>("pr") as u32,
        release_count: row.get::<i64, _>("releases") as u32,
        issue_opened_count: row.get::<i64, _>("issues") as u32,
        repositories_created_count: row.get::<i64, _>("repos") as u32,
        star_given_count: row.get::<i64, _>("stars") as u32,
    })
}

/// The column a browser-counter projection writes, if it is one.
fn browser_counter_column(projection: &str) -> Option<&'static str> {
    match projection {
        "repository_pull_request_merged_count" => Some("pull_request_merged_count"),
        "repository_release_count" => Some("release_count"),
        "repository_issue_opened_count" => Some("issue_opened_count"),
        "repository_repositories_created_count" => Some("repositories_created_count"),
        "repository_star_given_count" => Some("star_given_count"),
        _ => None,
    }
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
        // The browser counters share one shape: each is a per-repository column whose
        // new absolute value the Engine already computed, so one statement covers
        // them all rather than five near-identical arms.
        if let Some(column) = browser_counter_column(&change.projection) {
            let value: i64 = change.payload.parse().unwrap_or(0);
            let sql = format!(
                "INSERT INTO repository_projection
                    (canonical_repository_id, {column}, first_seen_at, updated_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(canonical_repository_id) DO UPDATE SET
                    {column}   = excluded.{column},
                    updated_at = excluded.updated_at"
            );
            sqlx::query(&sql)
                .bind(event.canonical_repository_id.as_str())
                .bind(value)
                .bind(now_ms)
                .bind(now_ms)
                .execute(&mut **tx)
                .await?;
            continue;
        }

        match change.projection.as_str() {
            // The payload is the USER's new total, which the Engine computed from the
            // global read. It is deliberately NOT written into the per-repository
            // column: that column means "commits in this repository", and writing a
            // global number into it made SUM(commit_count) count every commit once per
            // write. The per-repository column is incremented instead.
            "global_commit_total" => {
                sqlx::query(
                    "INSERT INTO repository_projection
                        (canonical_repository_id, commit_count, first_seen_at, updated_at)
                     VALUES (?, 1, ?, ?)
                     ON CONFLICT(canonical_repository_id) DO UPDATE SET
                        commit_count = commit_count + 1,
                        updated_at   = excluded.updated_at",
                )
                .bind(event.canonical_repository_id.as_str())
                .bind(now_ms)
                .bind(now_ms)
                .execute(&mut **tx)
                .await?;
            }

            // Kept for a replay of an event written by an older Engine, and for the
            // Golden Tests, which build state changes directly. Same meaning as
            // above: an absolute per-repository value.
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

                // Spec 14.1: the day is the user's day, not one repository's. The
                // Engine computed the per-repository value; the global row is one
                // increment of it, which is exactly what "this commit happened today"
                // means once and only once per event.
                sqlx::query(
                    "INSERT INTO daily_global_projection
                        (local_date, commit_count, daily_rank, timezone, date_policy, updated_at)
                     VALUES (?, 1, NULL, ?, ?, ?)
                     ON CONFLICT(local_date) DO UPDATE SET
                        commit_count = commit_count + 1,
                        timezone     = excluded.timezone,
                        date_policy  = excluded.date_policy,
                        updated_at   = excluded.updated_at",
                )
                .bind(event.local_date.as_str())
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

                sqlx::query(
                    "UPDATE daily_global_projection SET daily_rank = ?, updated_at = ?
                     WHERE local_date = ?",
                )
                .bind(&change.payload)
                .bind(now_ms)
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
    sqlx::query("DELETE FROM daily_global_projection")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM repository_projection")
        .execute(&mut *tx)
        .await?;

    // Every repository that produced any event keeps a row, including one known only
    // from browser activities: the Repositories page lists those too (spec 24.1),
    // and a repository with zero commits is a fact, not a missing row.
    sqlx::query(
        "INSERT INTO repository_projection
            (canonical_repository_id, first_seen_at, updated_at)
         SELECT canonical_repository_id, MIN(occurred_at), ?
         FROM events
         GROUP BY canonical_repository_id",
    )
    .bind(now_ms)
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

    // The row already exists (inserted above), so the commit total is written onto it
    // rather than inserted as a second row.
    for (repo, total) in &repo_totals {
        sqlx::query(
            "UPDATE repository_projection SET commit_count = ?, updated_at = ?
             WHERE canonical_repository_id = ?",
        )
        .bind(total)
        .bind(now_ms)
        .bind(repo)
        .execute(&mut *tx)
        .await?;
    }

    // Browser counters are recomputed from the event log in one statement each, which
    // is what keeps a rebuild equal to what live ingestion accumulated.
    for (event_type, column) in [
        ("PullRequestMerged", "pull_request_merged_count"),
        ("ReleasePublished", "release_count"),
        ("IssueOpened", "issue_opened_count"),
        ("RepositoryCreated", "repositories_created_count"),
        ("StarGiven", "star_given_count"),
    ] {
        let sql = format!(
            "UPDATE repository_projection SET {column} = (
                SELECT COUNT(*) FROM events e
                WHERE e.event_type = ?
                  AND e.canonical_repository_id = repository_projection.canonical_repository_id)"
        );
        sqlx::query(&sql).bind(event_type).execute(&mut *tx).await?;
    }

    // Events that only feed a browser counter still count as replayed, so the
    // caller's number keeps meaning "events read", not "commits read".
    let browser_only: Option<(i64,)> =
        sqlx::query_as("SELECT COUNT(*) FROM events WHERE event_type <> 'CommitCreated'")
            .fetch_optional(&mut *tx)
            .await?;
    replayed += browser_only.map(|(n,)| n).unwrap_or(0);

    // The global day is accumulated while the per-repository rows are written, so the
    // two can never disagree after a rebuild (checklist T1).
    let mut global_days: std::collections::BTreeMap<String, (i64, String, i64)> =
        std::collections::BTreeMap::new();

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

        let global = global_days
            .entry(date.clone())
            .or_insert((0, tz.clone(), *policy));
        global.0 += count;
    }

    for (date, (count, tz, policy)) in &global_days {
        let rank =
            devchievements_daily_rank::DailyRankThresholds::official().rank_for(*count as u32);
        sqlx::query(
            "INSERT INTO daily_global_projection
                (local_date, commit_count, daily_rank, timezone, date_policy, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
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

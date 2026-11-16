//! Outbox delivery: grouping, merging, claiming, acknowledging (spec 16.4, 16.6, 17.9).
//!
//! # Why delivery is separated from the state machine
//!
//! `devchievements-notifications` decides what is LEGAL. This module decides what
//! actually HAPPENS: which rows to pick up, how they group into one popup, in what
//! order, and what to persist before showing anything.
//!
//! # Grouping happens here, not in storage
//!
//! Storage writes one row per intent, because each row is an independently tracked
//! state machine entry. Merging is a PRESENTATION concern (spec 16.6), so it belongs
//! at delivery time: one event produced N rows, the user must see one popup.
//!
//! # The crash rule, restated because it is easy to get wrong
//!
//! Spec 17.9: persist `display_attempted_at` **before** the popup appears. If the
//! process dies after the attempt but before `completed`, recovery must NOT show it
//! again; it becomes `delivery_unknown` and relies on the gallery's new-unlock marker
//! so the user can still find it.

use devchievements_notifications::{present_event, NotificationItem, Presentation, Rarity};
use sqlx::{Row, SqlitePool};

/// An outbox row that is ready to be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNotification {
    /// Stable id.
    pub notification_id: String,
    /// The event behind it.
    pub event_id: String,
    /// What kind of notification.
    pub kind: String,
    /// JSON payload.
    pub payload: String,
    /// How many times delivery has been attempted.
    pub attempts: u32,
}

/// What the host (Tauri window, console, test) did with a presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayOutcome {
    /// It was shown (or its sound played).
    Shown,
    /// It could not be shown (no window, muted, OS refused).
    NotShown,
}

/// Whatever actually renders notifications.
///
/// Implementing this as a trait is what lets the same delivery logic drive a Tauri
/// window, a console printer, and a crash-recovery harness.
pub trait DeliverySink {
    /// Present one already-merged outcome. Called only after the attempt is persisted.
    fn present(&mut self, presentation: &Presentation) -> DisplayOutcome;
}

/// Why delivery failed.
#[derive(Debug)]
pub enum OutboxError {
    /// The database rejected an operation.
    Database(sqlx::Error),
}

impl std::fmt::Display for OutboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutboxError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for OutboxError {}

impl From<sqlx::Error> for OutboxError {
    fn from(e: sqlx::Error) -> Self {
        OutboxError::Database(e)
    }
}

/// Drives the outbox.
#[derive(Debug)]
pub struct OutboxProcessor {
    pool: SqlitePool,
}

impl OutboxProcessor {
    /// Wrap a pool.
    pub fn new(pool: SqlitePool) -> Self {
        OutboxProcessor { pool }
    }

    /// Deliver everything that is ready, one presentation per event.
    ///
    /// Returns how many presentations were shown and how many groups were abandoned
    /// as unknown. The caller decides cadence; a test calls this once per step.
    pub async fn deliver_pending(
        &self,
        sink: &mut dyn DeliverySink,
        max_attempts: u32,
    ) -> Result<(usize, usize), OutboxError> {
        let rows = self.take_ready().await?;

        let mut shown = 0usize;
        let mut unknown = 0usize;

        for group in group_by_event(rows) {
            let presentation = presentation_for(&group);
            if presentation == Presentation::Nothing {
                // Nothing to show, but the rows must not stay pending forever.
                for row in &group {
                    self.mark(&row.notification_id, "suppressed").await?;
                }
                continue;
            }

            // CRASH RULE: record the attempt on EVERY row before showing (spec 17.9).
            for row in &group {
                self.mark_attempt(&row.notification_id).await?;
            }

            match sink.present(&presentation) {
                DisplayOutcome::Shown => {
                    for row in &group {
                        self.mark(&row.notification_id, "completed").await?;
                    }
                    shown += 1;
                }
                DisplayOutcome::NotShown => {
                    let exhausted = group.iter().all(|r| r.attempts + 1 >= max_attempts);
                    for row in &group {
                        if exhausted {
                            // Never claim success we did not achieve.
                            self.mark(&row.notification_id, "delivery_unknown").await?;
                        } else {
                            self.requeue(&row.notification_id).await?;
                        }
                    }
                    if exhausted {
                        unknown += 1;
                    }
                }
            }
        }

        Ok((shown, unknown))
    }

    /// Recover rows left mid-flight by a crash.
    ///
    /// Any row that has a persisted display attempt but never reached a terminal
    /// state was interrupted between showing and recording. Spec 17.9 says do NOT
    /// show it again; mark it unknown so the gallery badge covers discovery.
    pub async fn recover_after_crash(&self) -> Result<usize, OutboxError> {
        let res = sqlx::query(
            "UPDATE outbox
             SET status = 'delivery_unknown', processed_at = CAST(strftime('%s','now') AS INTEGER) * 1000,
                 last_error = 'interrupted_after_display_attempt'
             WHERE display_attempted_at IS NOT NULL
               AND status NOT IN ('completed','delivery_unknown','failed','suppressed')",
        )
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() as usize)
    }

    async fn take_ready(&self) -> Result<Vec<PendingNotification>, OutboxError> {
        let rows = sqlx::query(
            "SELECT notification_id, event_id, kind, payload, attempts
             FROM outbox
             WHERE status IN ('created','desktop_pending')
               AND display_attempted_at IS NULL
             ORDER BY created_at ASC, notification_id ASC
             LIMIT 100",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PendingNotification {
                notification_id: row.get("notification_id"),
                event_id: row.get("event_id"),
                kind: row.get("kind"),
                payload: row.get("payload"),
                attempts: row.get::<i64, _>("attempts") as u32,
            })
            .collect())
    }

    async fn mark_attempt(&self, notification_id: &str) -> Result<(), OutboxError> {
        sqlx::query(
            "UPDATE outbox
             SET status = 'desktop_displayed',
                 display_attempted_at = CAST(strftime('%s','now') AS INTEGER) * 1000
             WHERE notification_id = ?",
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mark(&self, notification_id: &str, status: &str) -> Result<(), OutboxError> {
        sqlx::query(
            "UPDATE outbox SET status = ?, processed_at = CAST(strftime('%s','now') AS INTEGER) * 1000
             WHERE notification_id = ?",
        )
        .bind(status)
        .bind(notification_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn requeue(&self, notification_id: &str) -> Result<(), OutboxError> {
        sqlx::query(
            "UPDATE outbox SET status = 'created', attempts = attempts + 1, display_attempted_at = NULL
             WHERE notification_id = ?",
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Group ready rows by event, preserving arrival order.
///
/// A `Vec` of groups rather than a map, so the output order is deterministic and a
/// test can assert on it.
fn group_by_event(rows: Vec<PendingNotification>) -> Vec<Vec<PendingNotification>> {
    let mut groups: Vec<Vec<PendingNotification>> = Vec::new();
    for row in rows {
        match groups.iter_mut().find(|g| g[0].event_id == row.event_id) {
            Some(group) => group.push(row),
            None => groups.push(vec![row]),
        }
    }
    groups
}

/// Build the presentation for one event's rows.
pub fn presentation_for(group: &[PendingNotification]) -> Presentation {
    let mut items = Vec::new();
    let mut had_plain_commit = false;

    for row in group {
        match item_for(row) {
            Some(item) => items.push(item),
            None => {
                if row.kind == "commit_recorded" {
                    had_plain_commit = true;
                }
            }
        }
    }

    present_event(items, had_plain_commit)
}

/// Resolve an outbox row into a mergeable item, if it describes a reward.
///
/// Returns `None` for a plain commit: it is an acknowledgement, not a reward, and
/// must never take the popup's title or one of its three sub-line slots (spec 16.4).
fn item_for(row: &PendingNotification) -> Option<NotificationItem> {
    let payload: serde_json::Value = serde_json::from_str(&row.payload).ok()?;

    match row.kind.as_str() {
        "achievement_unlocked" => {
            let id = payload.get("achievement_id")?.as_str()?;
            let achievement = devchievements_achievements::builtin_achievements()
                .into_iter()
                .find(|a| a.achievement_id == id);
            match achievement {
                Some(a) => Some(NotificationItem {
                    id: a.achievement_id.clone(),
                    title: a.name.clone(),
                    rarity: a.rarity,
                    is_daily_rank: false,
                }),
                // An unknown id is still worth celebrating, but we cannot claim a
                // rarity we do not know; Common is the honest floor.
                None => Some(NotificationItem {
                    id: id.to_string(),
                    title: humanise(id),
                    rarity: Rarity::Common,
                    is_daily_rank: false,
                }),
            }
        }
        "daily_rank_upgrade" => {
            let rank = payload.get("rank")?.as_str()?;
            let rarity = rarity_for_rank(rank)?;
            Some(NotificationItem {
                id: format!("daily:{rank}"),
                title: format!("{} Day", humanise(rank)),
                rarity,
                is_daily_rank: true,
            })
        }
        _ => None,
    }
}

fn rarity_for_rank(rank: &str) -> Option<Rarity> {
    match rank {
        "common" => Some(Rarity::Common),
        "uncommon" => Some(Rarity::Uncommon),
        "rare" => Some(Rarity::Rare),
        "epic" => Some(Rarity::Epic),
        "legendary" => Some(Rarity::Legendary),
        _ => None,
    }
}

/// Turn a snake_case id into something readable.
fn humanise(id: &str) -> String {
    id.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read pending rows without consuming them (diagnostics and tests).
pub async fn pending(pool: &SqlitePool) -> Result<Vec<PendingNotification>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT notification_id, event_id, kind, payload, attempts FROM outbox
         WHERE status IN ('created','desktop_pending')
         ORDER BY created_at ASC, notification_id ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PendingNotification {
            notification_id: row.get("notification_id"),
            event_id: row.get("event_id"),
            kind: row.get("kind"),
            payload: row.get("payload"),
            attempts: row.get::<i64, _>("attempts") as u32,
        })
        .collect())
}

/// A sink that records instead of rendering, for tests and headless runs.
#[derive(Debug, Default)]
pub struct RecordingSink {
    /// Every presentation that was shown, in order.
    pub shown: Vec<Presentation>,
    /// Whether to report failure instead of success (for exercising retries).
    pub always_fail: bool,
}

impl DeliverySink for RecordingSink {
    fn present(&mut self, presentation: &Presentation) -> DisplayOutcome {
        if self.always_fail {
            return DisplayOutcome::NotShown;
        }
        self.shown.push(presentation.clone());
        DisplayOutcome::Shown
    }
}

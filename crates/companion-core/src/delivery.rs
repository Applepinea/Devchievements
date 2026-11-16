//! Outbox delivery: claiming, displaying, acknowledging (spec 16, 17.9, 32.1).
//!
//! # Why delivery is separated from the state machine
//!
//! `devchievements-notifications` decides what is LEGAL. This module decides what
//! actually HAPPENS: which rows to pick up, in what order, and what to persist
//! before showing anything.
//!
//! # The crash rule, restated because it is easy to get wrong
//!
//! Spec 17.9: persist `display_attempted_at` **before** the popup appears. If the
//! process dies after the attempt but before `completed`, recovery must NOT show it
//! again; it becomes `delivery_unknown` and relies on the gallery's new-unlock marker
//! so the user can still find it.
//!
//! The principle is "rather notify too little than notify twice", so every ambiguous
//! state resolves toward silence.

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

/// What the host (Tauri window, console, test) did with a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayOutcome {
    /// It was shown.
    Shown,
    /// It could not be shown (no window, muted, OS refused).
    NotShown,
}

/// Whatever actually renders notifications.
///
/// Implementing this as a trait is what lets the same delivery logic drive a Tauri
/// window, a console printer in tests, and a crash-recovery harness.
pub trait DeliverySink {
    /// Show one notification. Called only after the attempt has been persisted.
    fn show(&mut self, notification: &PendingNotification) -> DisplayOutcome;
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

    /// Deliver everything that is ready, in creation order.
    ///
    /// Returns how many were shown and how many were abandoned as unknown. The
    /// caller decides the cadence; a test calls this once per simulated step.
    pub async fn deliver_pending(
        &self,
        sink: &mut dyn DeliverySink,
        max_attempts: u32,
    ) -> Result<(usize, usize), OutboxError> {
        let rows = self.take_ready().await?;

        let mut shown = 0usize;
        let mut unknown = 0usize;

        for pending in rows {
            // CRASH RULE: record the attempt BEFORE showing (spec 17.9).
            self.mark_attempt(&pending.notification_id).await?;

            match sink.show(&pending) {
                DisplayOutcome::Shown => {
                    self.mark(&pending.notification_id, "completed").await?;
                    shown += 1;
                }
                DisplayOutcome::NotShown => {
                    if pending.attempts + 1 >= max_attempts {
                        // Retries exhausted: never claim success we did not achieve.
                        self.mark(&pending.notification_id, "delivery_unknown")
                            .await?;
                        unknown += 1;
                    } else {
                        // Put it back for another try.
                        self.requeue(&pending.notification_id).await?;
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

/// A notification as handed to the UI layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredNotification {
    /// Stable id.
    pub notification_id: String,
    /// Kind, for choosing an icon and sound.
    pub kind: String,
    /// Raw JSON.
    pub payload: String,
}

/// A sink that records instead of rendering, for tests and headless runs.
#[derive(Debug, Default)]
pub struct RecordingSink {
    /// Everything that was shown, in order.
    pub shown: Vec<DeliveredNotification>,
    /// Whether to report failure instead of success (for exercising retries).
    pub always_fail: bool,
}

impl DeliverySink for RecordingSink {
    fn show(&mut self, notification: &PendingNotification) -> DisplayOutcome {
        if self.always_fail {
            return DisplayOutcome::NotShown;
        }
        self.shown.push(DeliveredNotification {
            notification_id: notification.notification_id.clone(),
            kind: notification.kind.clone(),
            payload: notification.payload.clone(),
        });
        DisplayOutcome::Shown
    }
}

//! Outbox observability: latency distribution and reason codes (spec 34.1).
//!
//! # Why a bare count is not enough
//!
//! "3 notifications were never shown" says nothing actionable. Was it an expired
//! offer, a display attempt interrupted by a crash, or a sink that kept failing? The
//! fix is different in each case, so the diagnostics must distinguish them.
//!
//! # Why the buckets are coarse
//!
//! Notification latency is expected to be tens of milliseconds (local IPC or a spool
//! drain). Order-of-magnitude buckets are enough to spot "this got slower" without
//! implying a precision the data does not have.

use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::StorageResult;

/// Why a notification did not reach the user, in the terms the product uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// The display attempt was persisted, then the process died (spec 17.9).
    InterruptedAfterDisplayAttempt,
    /// Delivery outcome unknown for a reason the record does not name.
    DeliveryUnknown,
    /// The sink reported failure until attempts were exhausted.
    DeliveryFailed,
    /// Suppressed by policy: stale event, import, or an explicit rule.
    SuppressedByPolicy,
    /// Every connected channel declined the offer.
    AllChannelsDeclined,
    /// Anything else; kept so a new status cannot silently disappear from the report.
    Other,
}

impl ReasonCode {
    /// Stable identifier used in diagnostics output.
    pub fn as_str(self) -> &'static str {
        match self {
            ReasonCode::InterruptedAfterDisplayAttempt => "interrupted_after_display_attempt",
            ReasonCode::DeliveryUnknown => "delivery_unknown",
            ReasonCode::DeliveryFailed => "delivery_failed",
            ReasonCode::SuppressedByPolicy => "suppressed_by_policy",
            ReasonCode::AllChannelsDeclined => "all_channels_declined",
            ReasonCode::Other => "other",
        }
    }

    /// Classify an outbox row from its status and recorded error.
    ///
    /// The mapping lives here rather than in the UI so every consumer agrees on what a
    /// status means.
    pub fn classify(status: &str, last_error: Option<&str>) -> Self {
        match status {
            "delivery_unknown" => match last_error {
                Some(e) if e.contains("interrupted_after_display_attempt") => {
                    ReasonCode::InterruptedAfterDisplayAttempt
                }
                _ => ReasonCode::DeliveryUnknown,
            },
            "failed" => ReasonCode::DeliveryFailed,
            "suppressed" => ReasonCode::SuppressedByPolicy,
            "declined" | "expired" => ReasonCode::AllChannelsDeclined,
            _ => ReasonCode::Other,
        }
    }
}

/// One reason and how many rows carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasonCount {
    /// The reason.
    pub reason: String,
    /// How many notifications ended this way.
    pub count: i64,
}

/// Coarse latency buckets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyBuckets {
    /// Under 100 ms.
    pub under_100ms: i64,
    /// 100 ms to under 1 s.
    pub under_1s: i64,
    /// 1 s to under 10 s.
    pub under_10s: i64,
    /// 10 s to under 1 minute.
    pub under_1min: i64,
    /// A minute or more.
    pub over_1min: i64,
}

/// Latency summary for delivered notifications.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Latency {
    /// How many rows the distribution covers.
    pub samples: i64,
    /// Median, in milliseconds.
    pub p50_ms: i64,
    /// 90th percentile, in milliseconds.
    pub p90_ms: i64,
    /// 99th percentile, in milliseconds.
    pub p99_ms: i64,
    /// Slowest observed.
    pub max_ms: i64,
    /// Coarse buckets.
    pub buckets: LatencyBuckets,
}

/// The full outbox health picture.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxHealth {
    /// Rows in total.
    pub total: i64,
    /// Rows that have not reached a terminal state.
    pub pending: i64,
    /// Age in milliseconds of the oldest pending row, if any.
    ///
    /// A growing number here means the drain loop is not keeping up, which no per-event
    /// metric would reveal.
    pub oldest_pending_ms: Option<i64>,
    /// Non-terminal rows grouped by status.
    pub reasons: Vec<ReasonCount>,
    /// Delivery latency.
    pub latency: Latency,
}

/// Compute the health picture.
pub async fn outbox_health(pool: &SqlitePool, now_ms: i64) -> StorageResult<OutboxHealth> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox")
        .fetch_one(pool)
        .await?;

    // Spec 17.2's terminal states. Anything else is still in flight.
    let terminal = "('completed','delivery_unknown','failed','declined','suppressed')";
    let pending: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM outbox WHERE status NOT IN {terminal}"
    ))
    .fetch_one(pool)
    .await?;

    let oldest: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT MIN(created_at) FROM outbox WHERE status NOT IN {terminal}"
    ))
    .fetch_optional(pool)
    .await?
    .flatten();
    let oldest_pending_ms = oldest.map(|created| (now_ms - created).max(0));

    // Reason codes come from the rows that did NOT reach the user; a completed row is
    // the success case and has no reason.
    let reason_rows = sqlx::query(&format!(
        "SELECT status, last_error FROM outbox WHERE status NOT IN ('completed') AND status IN {terminal}"
    ))
    .fetch_all(pool)
    .await?;

    let mut counts: std::collections::BTreeMap<&'static str, i64> =
        std::collections::BTreeMap::new();
    for row in reason_rows {
        let status: String = row.get("status");
        let last_error: Option<String> = row.get("last_error");
        let code = ReasonCode::classify(&status, last_error.as_deref());
        *counts.entry(code.as_str()).or_insert(0) += 1;
    }
    let reasons = counts
        .into_iter()
        .map(|(reason, count)| ReasonCount {
            reason: reason.to_string(),
            count,
        })
        .collect();

    let latency = collect_latency(pool).await?;

    Ok(OutboxHealth {
        total,
        pending,
        oldest_pending_ms,
        reasons,
        latency,
    })
}

/// Read the latencies of rows that were actually presented.
async fn collect_latency(pool: &SqlitePool) -> StorageResult<Latency> {
    let rows = sqlx::query(
        "SELECT created_at, processed_at FROM outbox
         WHERE processed_at IS NOT NULL AND status = 'completed'",
    )
    .fetch_all(pool)
    .await?;

    let mut samples: Vec<i64> = rows
        .into_iter()
        .map(|row| {
            let created: i64 = row.get("created_at");
            let processed: i64 = row.get("processed_at");
            (processed - created).max(0)
        })
        .collect();

    if samples.is_empty() {
        return Ok(Latency::default());
    }

    samples.sort_unstable();
    let n = samples.len();
    let at = |p: f64| -> i64 {
        let idx = ((p * n as f64).ceil() as usize)
            .saturating_sub(1)
            .min(n - 1);
        samples[idx]
    };

    let mut buckets = LatencyBuckets::default();
    for ms in &samples {
        match ms {
            0..=99 => buckets.under_100ms += 1,
            100..=999 => buckets.under_1s += 1,
            1_000..=9_999 => buckets.under_10s += 1,
            10_000..=59_999 => buckets.under_1min += 1,
            _ => buckets.over_1min += 1,
        }
    }

    Ok(Latency {
        samples: n as i64,
        p50_ms: at(0.50),
        p90_ms: at(0.90),
        p99_ms: at(0.99),
        max_ms: *samples.last().unwrap_or(&0),
        buckets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupted_display_attempt_is_its_own_reason() {
        // Spec 17.9 distinguishes "we tried and lost the process" from "delivery
        // failed"; conflating them hides a crash.
        assert_eq!(
            ReasonCode::classify(
                "delivery_unknown",
                Some("interrupted_after_display_attempt")
            ),
            ReasonCode::InterruptedAfterDisplayAttempt
        );
    }

    #[test]
    fn an_unnamed_unknown_stays_generic() {
        assert_eq!(
            ReasonCode::classify("delivery_unknown", None),
            ReasonCode::DeliveryUnknown
        );
        assert_eq!(
            ReasonCode::classify("delivery_unknown", Some("something else")),
            ReasonCode::DeliveryUnknown
        );
    }

    #[test]
    fn every_terminal_failure_status_maps_to_a_reason() {
        assert_eq!(
            ReasonCode::classify("failed", None),
            ReasonCode::DeliveryFailed
        );
        assert_eq!(
            ReasonCode::classify("suppressed", None),
            ReasonCode::SuppressedByPolicy
        );
        assert_eq!(
            ReasonCode::classify("declined", None),
            ReasonCode::AllChannelsDeclined
        );
        assert_eq!(
            ReasonCode::classify("expired", None),
            ReasonCode::AllChannelsDeclined
        );
    }

    #[test]
    fn an_unknown_status_is_reported_rather_than_dropped() {
        // A new status added later must show up as "other", not vanish from the report.
        assert_eq!(
            ReasonCode::classify("something_new", None),
            ReasonCode::Other
        );
    }

    #[test]
    fn reason_names_are_stable_and_distinct() {
        let all = [
            ReasonCode::InterruptedAfterDisplayAttempt,
            ReasonCode::DeliveryUnknown,
            ReasonCode::DeliveryFailed,
            ReasonCode::SuppressedByPolicy,
            ReasonCode::AllChannelsDeclined,
            ReasonCode::Other,
        ];
        let names: std::collections::BTreeSet<&str> = all.iter().map(|r| r.as_str()).collect();
        assert_eq!(names.len(), all.len(), "reason codes must be distinct");
    }

    #[test]
    fn the_health_picture_survives_a_json_round_trip() {
        let health = OutboxHealth {
            total: 4,
            pending: 1,
            oldest_pending_ms: Some(250),
            reasons: vec![ReasonCount {
                reason: "delivery_failed".to_string(),
                count: 2,
            }],
            latency: Latency {
                samples: 3,
                p50_ms: 10,
                p90_ms: 20,
                p99_ms: 30,
                max_ms: 30,
                buckets: LatencyBuckets {
                    under_100ms: 3,
                    ..LatencyBuckets::default()
                },
            },
        };
        let json = serde_json::to_string(&health).expect("serialise");
        let back: OutboxHealth = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(health, back);
    }

    /// Create the event row an outbox row must reference (the foreign key is real).
    async fn insert_event(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO events (event_id, event_type, source, source_confidence,
                 canonical_repository_id, external_id, commit_sha, occurred_at, observed_at,
                 received_at, local_date, timezone, date_policy, payload, schema_version,
                 ingest_batch)
             VALUES (?, 'commit_created', 'local_git_hook', 'verified_local',
                 'local:test', NULL, NULL, 0, 0, 0, '2026-01-01', 'UTC', 0, '{}', 1, NULL)",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("insert event row");
    }

    /// Insert one outbox row directly, so the SQL and the percentile maths are
    /// exercised without going through ingestion.
    async fn insert_row(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        created: i64,
        processed: Option<i64>,
        last_error: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO outbox (notification_id, event_id, status, created_at, processed_at, last_error, kind, payload)
             VALUES (?, 'evt', ?, ?, ?, ?, 'commit_recorded', '{}')",
        )
        .bind(id)
        .bind(status)
        .bind(created)
        .bind(processed)
        .bind(last_error)
        .execute(pool)
        .await
        .expect("insert outbox row");
    }

    #[tokio::test]
    async fn health_reports_reasons_and_latency_from_real_rows() {
        let dir = std::env::temp_dir().join(format!("devch-obs-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let pool = crate::open(&dir.join("t.db")).await.expect("open");
        crate::ingest::migrate(&pool).await.expect("migrate");
        insert_event(&pool, "evt").await;

        // Two delivered quickly, one slow, plus three failures with distinct reasons.
        insert_row(&pool, "n1", "completed", 1_000, Some(1_050), None).await;
        insert_row(&pool, "n2", "completed", 2_000, Some(2_080), None).await;
        insert_row(&pool, "n3", "completed", 3_000, Some(33_000), None).await;
        insert_row(
            &pool,
            "n4",
            "failed",
            4_000,
            Some(4_010),
            Some("sink refused"),
        )
        .await;
        insert_row(
            &pool,
            "n5",
            "delivery_unknown",
            5_000,
            Some(5_010),
            Some("interrupted_after_display_attempt"),
        )
        .await;
        insert_row(&pool, "n6", "suppressed", 6_000, Some(6_010), None).await;
        // Still in flight: must count as pending and not appear in the reasons list.
        insert_row(&pool, "n7", "created", 7_000, None, None).await;

        let health = outbox_health(&pool, 10_000).await.expect("health");

        assert_eq!(health.total, 7);
        assert_eq!(health.pending, 1, "one row is still in flight");
        assert_eq!(
            health.oldest_pending_ms,
            Some(3_000),
            "the pending row was created at 7000, now is 10000"
        );

        let by_reason: std::collections::BTreeMap<&str, i64> = health
            .reasons
            .iter()
            .map(|r| (r.reason.as_str(), r.count))
            .collect();
        assert_eq!(by_reason.get("delivery_failed"), Some(&1));
        assert_eq!(by_reason.get("interrupted_after_display_attempt"), Some(&1));
        assert_eq!(by_reason.get("suppressed_by_policy"), Some(&1));
        assert_eq!(by_reason.len(), 3, "only failures carry a reason");

        // Latencies are 50, 80 and 30000.
        assert_eq!(health.latency.samples, 3);
        assert_eq!(health.latency.max_ms, 30_000);
        assert_eq!(health.latency.p50_ms, 80, "median of 50/80/30000");
        assert_eq!(health.latency.buckets.under_100ms, 2);
        assert_eq!(health.latency.buckets.under_1min, 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}

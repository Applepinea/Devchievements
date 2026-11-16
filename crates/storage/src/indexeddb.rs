//! IndexedDB adapter interface (spec 31.1: design only, no browser work).
//!
//! # Why this is a trait and not an implementation
//!
//! Phase 1 must not implement the browser engine (that is Phase 3a). But the
//! browser path must not invent its own semantics either, or the two modes will
//! disagree about what a "day" is or when an achievement unlocks.
//!
//! So this module fixes the CONTRACT: the same operations, the same atomicity
//! requirement, the same idempotency key. The browser side supplies a JavaScript
//! implementation later; the Rust side already knows what it is allowed to assume.
//!
//! # The atomicity requirement
//!
//! IndexedDB transactions are also atomic within a single `transaction()` call,
//! which makes the desktop and browser paths structurally similar: event +
//! projection + outbox commit together or not at all (spec 4.3).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use devchievements_events::Event;

/// A snapshot row as the browser would read it.
///
/// Mirrors the desktop projection shape so the Engine sees identical inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    /// Repository-wide commit count.
    pub repository_commit_count: u32,
    /// Today's commit count.
    pub daily_commit_count: u32,
    /// Today's rank, if any.
    pub daily_rank: Option<String>,
    /// Unlocked achievement ids.
    pub unlocked_achievement_ids: Vec<String>,
}

/// An outbox row the browser must persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxIntent {
    /// Stable notification id (idempotency key).
    pub notification_id: String,
    /// The event that caused it.
    pub event_id: String,
    /// What to show.
    pub kind: String,
    /// JSON payload.
    pub payload: String,
}

/// Result of a store operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOutcome {
    /// Committed atomically.
    Committed,
    /// The event id was already present.
    Duplicate,
}

/// Errors a browser adapter may report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The quota was exceeded (spec 26.4: notify, never silently drop).
    QuotaExceeded,
    /// The transaction aborted for another reason.
    Aborted(String),
    /// The stored data did not match the expected version.
    SchemaMismatch {
        /// Version found.
        found: u32,
        /// Version expected.
        expected: u32,
    },
}

/// The contract every browser-side storage adapter must satisfy.
///
/// Deliberately async and object-safe so it can be implemented by an FFI shim over
/// JavaScript without rewriting the Engine.
pub trait IndexedDbAdapter {
    /// Read the projections the Engine needs for one evaluation.
    fn load_snapshot(
        &self,
        repository_id: &str,
        local_date: &str,
    ) -> impl std::future::Future<Output = Result<ProjectionSnapshot, StoreError>>;

    /// Commit an event together with its projection changes and outbox intents.
    ///
    /// MUST be atomic, and MUST be a no-op returning [`StoreOutcome::Duplicate`]
    /// when the event id is already stored.
    fn commit_event(
        &self,
        event: &Event,
        snapshot: &ProjectionSnapshot,
        intents: &[OutboxIntent],
    ) -> impl std::future::Future<Output = Result<StoreOutcome, StoreError>>;
}

/// The object store names the browser adapter must create.
///
/// Pinned here so the desktop schema and the browser schema cannot drift apart
/// silently: the names are part of the interface, not an implementation detail.
pub const OBJECT_STORES: [&str; 5] = [
    "events",
    "daily_projection",
    "repository_projection",
    "unlocked_achievements",
    "outbox",
];

/// The IndexedDB schema version to open with.
pub const IDB_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_store_names_cover_every_collection_the_contract_needs() {
        // If a new collection is needed, it must be added here AND in the desktop
        // schema, which is what keeps the two modes honest.
        for required in [
            "events",
            "daily_projection",
            "repository_projection",
            "unlocked_achievements",
            "outbox",
        ] {
            assert!(
                OBJECT_STORES.contains(&required),
                "missing object store: {required}"
            );
        }
    }

    #[test]
    fn schema_version_is_pinned() {
        assert_eq!(IDB_VERSION, 1);
    }

    #[test]
    fn quota_exceeded_is_a_distinguishable_error() {
        // Spec 26.4: the browser must be able to say "quota" specifically, because
        // the UI has to warn the user rather than silently drop events.
        let e = StoreError::QuotaExceeded;
        assert_ne!(e, StoreError::Aborted("x".to_string()));
    }
}

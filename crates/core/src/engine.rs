//! Engine entry point.
//!
//! Phase 0 scaffold: the types and the call shape are fixed here so that later
//! phases cannot drift into putting IO inside the Engine. Projection and rule
//! evaluation arrive in Phase 1.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use crate::context::InjectedContext;

/// A read-only view of the projections a rule needs.
///
/// # Read-set discipline (spec 4.3)
///
/// Every rule declares a read-set. The storage adapter loads only the
/// projections inside that read-set. Commits must never trigger a scan of all
/// historical raw events or of every repository's statistics.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StateSnapshot {
    /// Monotonic snapshot version, used to detect stale optimistic reads.
    pub version: u64,
}

/// A single projection mutation the Engine wants applied.
///
/// The Engine does not write anything; it returns these and the storage adapter
/// applies them inside one transaction together with the outbox intents.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StateChange {
    /// Logical projection name (for example `daily_commit_count`).
    pub projection: String,
    /// Opaque, versioned payload.
    pub payload: String,
}

/// What the Engine produced for one event.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EngineOutput {
    /// Projection mutations to apply atomically.
    pub state_changes: Vec<StateChange>,
    /// Side effects the consumer may perform after the transaction commits.
    pub side_effect_intents: Vec<SideEffectIntent>,
}

/// An intent, not an action.
///
/// The Engine never plays a sound or shows a popup; it records that one is
/// wanted. Consumers decide whether the event is fresh enough to actually
/// deliver it (spec 16.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SideEffectIntent {
    /// Notification identity, stable across retries.
    pub notification_id: String,
    /// Reason the notification exists.
    pub kind: SideEffectKind,
}

/// Why a notification intent was produced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SideEffectKind {
    /// A permanent achievement was unlocked.
    AchievementUnlocked {
        /// Stable achievement identifier.
        achievement_id: String,
    },
    /// The daily rank improved.
    DailyRankUpgrade {
        /// Rank identifier (Common..= Legendary).
        rank: String,
    },
    /// A plain commit happened (light sound only by default).
    CommitRecorded,
}

/// The Engine itself.
///
/// Stateless by design: all state lives in [`StateSnapshot`] and all environment
/// access goes through [`InjectedContext`].
#[derive(Debug, Default, Clone, Copy)]
pub struct Engine;

impl Engine {
    /// Evaluate one normalized event.
    ///
    /// Returns deterministic output for identical inputs. Implemented in Phase 1.
    pub fn evaluate(
        &self,
        _snapshot: &StateSnapshot,
        _event: &str,
        _ctx: &mut InjectedContext<'_>,
    ) -> EngineOutput {
        EngineOutput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_engine_output_is_empty() {
        let out = EngineOutput::default();
        assert!(out.state_changes.is_empty());
        assert!(out.side_effect_intents.is_empty());
    }
}

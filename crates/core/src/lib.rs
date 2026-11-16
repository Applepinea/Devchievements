//! Pure-logic core engine.
//!
//! # Contract (spec 4.2)
//!
//! ```text
//! Engine(state_snapshot, event, injected_context)
//!   -> { state_changes, side_effect_intents }
//! ```
//!
//! This crate is a pure logic layer. It must never read the clock, generate
//! random IDs, touch a database, the filesystem or the network, or emit OS
//! notifications. Every external dependency is injected via [`context`] so
//! Golden Tests can substitute them deterministically.
//!
//! # Determinism rules
//!
//! - No `SystemTime::now()`, no `rand`, no `std::env` lookups at decision time.
//! - No wall-clock timeouts anywhere in rule evaluation (spec 22): step budgets
//!   are counted, not timed, so results are reproducible.
//! - Iteration order over collections must be explicit and stable.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod context;
pub mod engine;
pub mod wasm_entry;
pub mod wasm_probe;

pub use context::{
    ClockProvider, ConfigurationProvider, FixedClock, IdGenerator, InjectedContext,
    TimezoneProvider,
};
pub use engine::{
    Engine, EngineOutput, SideEffectIntent, SideEffectKind, StateChange, StateSnapshot,
};

/// Crate version, recorded alongside persisted data so that rule attribution and
/// migrations can tell which code produced a result (spec 13.3 `rule_version`).
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn core_version_is_not_empty() {
        assert!(!super::CORE_VERSION.is_empty());
    }

    /// Proves the serde dependency is real and that EngineOutput round-trips
    /// through JSON identically on native and (later) WASM builds.
    #[test]
    fn engine_output_round_trips_through_json() {
        let original = super::EngineOutput {
            state_changes: vec![super::StateChange {
                projection: "daily_commit_count".to_string(),
                payload: "{\"value\":1}".to_string(),
            }],
            side_effect_intents: vec![super::SideEffectIntent {
                notification_id: "n-1".to_string(),
                kind: super::SideEffectKind::CommitRecorded,
            }],
        };
        let encoded =
            serde_json::to_string(&original).expect("serializing EngineOutput must succeed");
        let decoded: super::EngineOutput =
            serde_json::from_str(&encoded).expect("deserializing EngineOutput must succeed");
        assert_eq!(original, decoded);
    }
}

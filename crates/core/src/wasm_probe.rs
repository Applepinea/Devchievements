//! WASM probe (ADR-0001 evidence).
//!
//! This module exists ONLY to answer the Phase 0 question: can the Core Engine be
//! compiled to `wasm32-unknown-unknown` and produce output identical to native?
//!
//! It deliberately uses no wasm-bindgen / wasm-pack. The reason is the same as
//! ADR-0002's: fewer moving parts, and a browser MV3 CSP can instantiate a raw
//! module without any JS glue. Later phases may add a thin binding layer if the
//! extension needs richer marshalling.
//!
//! Nothing here is business logic; it serialises a known `EngineOutput` so the
//! native and wasm builds can be compared byte for byte.

use crate::engine::{EngineOutput, SideEffectIntent, SideEffectKind, StateChange};

/// Build a deterministic sample `EngineOutput`.
///
/// Used by both the native test and the wasm runner so the comparison is exact.
pub fn sample_output() -> EngineOutput {
    EngineOutput {
        state_changes: vec![
            StateChange {
                projection: "daily_commit_count".to_string(),
                payload: "{\"local_date\":\"2026-11-16\",\"value\":1}".to_string(),
            },
            StateChange {
                projection: "repository_commit_total".to_string(),
                payload: "{\"repository_id\":\"repo-1\",\"value\":42}".to_string(),
            },
        ],
        side_effect_intents: vec![SideEffectIntent {
            notification_id: "notif-0001".to_string(),
            kind: SideEffectKind::AchievementUnlocked {
                achievement_id: "first_commit".to_string(),
                source: crate::UnlockSourceTag::Realtime,
            },
        }],
    }
}

/// Serialise the sample output to canonical JSON.
pub fn sample_json() -> String {
    serde_json::to_string(&sample_output()).unwrap_or_else(|_| String::from("{}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_is_deterministic() {
        assert_eq!(super::sample_json(), super::sample_json());
    }

    #[test]
    fn sample_contains_expected_markers() {
        let json = super::sample_json();
        assert!(json.contains("first_commit"));
        assert!(json.contains("daily_commit_count"));
    }
}

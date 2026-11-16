//! Golden Tests support: load recorded Git hook traces and replay them.
//!
//! # Why fixtures instead of live experiments
//!
//! Phase 0 recorded what Git 2.54.0 actually does on Windows (15 scenarios in
//! `packages/golden-tests/fixtures/git-traces/`). Those traces are the contract:
//! if a later refactor changes how an amend or a rebase is classified, replaying
//! the SAME recorded hook sequence must expose it. Re-running live Git would make
//! the test depend on the developer's Git version.
//!
//! # What is asserted
//!
//! Only facts the trace actually contains (spec 9.7), plus the product rules that
//! are supposed to follow from them. We do not invent outcomes the recording cannot
//! support.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod trace;

/// Crate version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Directory that holds the recorded fixtures, relative to the workspace root.
pub const FIXTURE_SUBDIR: &str = "packages/golden-tests/fixtures/git-traces";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_not_empty() {
        assert!(!super::CRATE_VERSION.is_empty());
    }
}

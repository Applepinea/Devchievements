//! Storage adapter (SQLite + SQLx) -- Phase 1 deliverable.
//!
//! # Boundary (AGENTS.md 6)
//!
//! Owns persistence: transactions, migrations, projections, the outbox table.
//! Must NOT contain achievement judgement logic.
//!
//! # Why this is empty
//!
//! The Phase 0 spike could not be compiled on the current development host:
//! `sqlx` pulls in `windows-sys`, which needs `dlltool` -> `as.exe`, and this
//! machine's GNU toolchain has no assembler. See ADR-0012.
//!
//! Per AGENTS.md 10, code that has not compiled must not be presented as done, so
//! the spike was removed rather than left here half-verified. Phase 1 reimplements
//! it against a real MSVC or CI environment.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Crate version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_not_empty() {
        assert!(!super::CRATE_VERSION.is_empty());
    }
}

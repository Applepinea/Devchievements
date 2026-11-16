//! User event filters and statistics scope.
//!
//! Phase 0 scaffold. See AGENTS.md section 6 for this crate's module boundary.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Crate version, recorded alongside persisted data so migrations and
/// diagnostics can attribute results to the code that produced them.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_not_empty() {
        assert!(!super::CRATE_VERSION.is_empty());
    }
}

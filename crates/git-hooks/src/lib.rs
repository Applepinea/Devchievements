//! Git hook helper: repository capture, hook install/chaining/uninstall,
//! IPC and Spool delivery.
//!
//! Phase 0 delivers the `gitread` module only: the measured, validated way to
//! read commit facts without spawning `git` (ADR-0002). Hook installation,
//! chaining, IPC and Spool arrive in Phase 2a.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod gitread;
pub mod hooks;
pub mod spool;

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

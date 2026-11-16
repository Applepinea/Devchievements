//! Property tests live in `tests/`; this crate exists only to host them.
//!
//! # Why a separate crate
//!
//! Property tests are slow and numerous. Keeping them here means `cargo test -p
//! devchievements-property-tests` can be run deliberately (and in CI with a higher
//! case count) without slowing the inner development loop of every other crate.

/// Crate version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

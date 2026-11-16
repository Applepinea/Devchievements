//! The Companion's headless data loop (spec 32.1).
//!
//! # Why this is a library and not the Tauri app
//!
//! Everything spec 32.1 asks for except the popup itself is pure data movement:
//!
//! ```text
//! Spool -> validate -> ingest (SQLite + Engine) -> Outbox -> deliver
//! ```
//!
//! Making that a library has two concrete benefits, not just tidiness:
//!
//! 1. **It can be tested for real.** A windowed app cannot be driven from a test
//!    harness in this environment (no MSVC, no browser), but a headless loop can run
//!    end to end against a real SQLite file and a real Spool directory.
//! 2. **Crash recovery becomes measurable.** Spec 32.5 requires killing the process
//!    at six specific moments. Killing a headless binary is a normal test operation;
//!    killing a GUI and asserting on its internal state is not.
//!
//! The Tauri layer, when it exists, becomes a thin caller of [Companion] plus a
//! channel that renders [DeliveredNotification] values.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod delivery;
pub mod spool_consumer;

pub use delivery::{presentation_for, DeliverySink, DisplayOutcome, OutboxError, OutboxProcessor};
pub use spool_consumer::{serve_ipc_blocking, Companion, CompanionConfig, DrainReport};

/// Crate version.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

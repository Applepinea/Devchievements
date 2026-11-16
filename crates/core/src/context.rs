//! Injected external dependencies.
//!
//! Nothing in this crate may reach out to the real world. Everything the Engine
//! needs comes through these traits, and every trait has a deterministic test
//! implementation.
//!
//! Every provider is `Send + Sync`. They are stateless or internally synchronised, and
//! requiring it keeps the Engine usable from any executor: without it the future
//! produced by ingestion is not `Send`, and a Tauri command cannot await it.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use core::fmt;

/// Wall-clock abstraction.
///
/// Unix epoch milliseconds, UTC. The Engine must never call `SystemTime::now()`
/// directly: Golden Tests replay traces with a fixed clock.
pub trait ClockProvider: fmt::Debug + Send + Sync {
    /// Milliseconds since the Unix epoch, UTC.
    fn now_unix_millis(&self) -> i64;
}

/// IANA time zone resolution and local-date computation.
///
/// The Engine asks for a local date and a time zone; it never computes one from
/// an offset it guessed itself (spec 12).
pub trait TimezoneProvider: fmt::Debug + Send + Sync {
    /// Resolve an IANA time zone name, returning a canonical identifier.
    fn canonicalize(&self, iana_name: &str) -> Option<String>;

    /// Compute the local date (`YYYY-MM-DD`) for an instant in a zone, given the
    /// configured day boundary in minutes from local midnight (0 or 240 for
    /// 00:00 / 04:00).
    fn local_date(
        &self,
        unix_millis: i64,
        iana_name: &str,
        day_start_minutes: u16,
    ) -> Option<String>;
}

/// Deterministic ID generation.
///
/// Event IDs double as idempotency keys (spec 5.3), so tests need them to be
/// reproducible. Production uses a seeded UUIDv7-style generator; tests use a
/// counter.
pub trait IdGenerator: fmt::Debug + Send + Sync {
    /// Generate the next identifier. Stable given the same internal state.
    fn next_id(&mut self) -> String;
}

/// Access to the effective configuration version.
///
/// Rules record the configuration version they ran under, so changing a user
/// filter or a daily-rank threshold never rewrites history (spec 13.3).
pub trait ConfigurationProvider: fmt::Debug + Send + Sync {
    /// Monotonic configuration version.
    fn config_version(&self) -> u64;
}

/// Everything the Engine may see besides the state snapshot and the event.
pub struct InjectedContext<'a> {
    /// Wall clock.
    pub clock: &'a dyn ClockProvider,
    /// IANA time zone handling.
    pub timezone: &'a dyn TimezoneProvider,
    /// Identifier generation.
    pub ids: &'a mut dyn IdGenerator,
    /// Effective configuration.
    pub config: &'a dyn ConfigurationProvider,
}

impl fmt::Debug for InjectedContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InjectedContext")
            .field("now", &self.clock.now_unix_millis())
            .field("config_version", &self.config.config_version())
            .finish_non_exhaustive()
    }
}

/// A clock that always returns the same instant. Test-only.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub i64);

impl ClockProvider for FixedClock {
    fn now_unix_millis(&self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_constant() {
        let c = FixedClock(1_700_000_000_000);
        assert_eq!(c.now_unix_millis(), 1_700_000_000_000);
    }
}

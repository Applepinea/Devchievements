//! Standard event model (spec 5).
//!
//! # Why this crate is pure data + validation
//!
//! The event is the unit that everything else agrees on: the Hook Helper produces
//! one, the storage adapter persists it, the Engine consumes it, Golden Tests
//! replay it. If the shape or the validation rules live in more than one place,
//! native and browser builds drift. So the model and its rules live here, with no
//! IO and no clock.
//!
//! # The three timestamps (spec 5.3)
//!
//! - `occurred_at`: when the thing actually happened (author/committer time, or
//!   the commit's own timestamp);
//! - `observed_at`: when a detector saw it (Hook fired, page changed);
//! - `received_at`: when this system accepted it.
//!
//! These are NOT interchangeable. Freshness policy (spec 16.3) is computed from
//! `occurred_at`, while diagnostics and Spool replay care about the other two.
//! Collapsing them is how "a week-old event celebrates as if it just happened".

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod event;
pub mod identity;
pub mod repository;
pub mod validation;

pub use event::{
    CanonicalRepositoryId, Event, EventId, EventSource, EventType, LocalDate, SchemaVersion,
    SourceConfidence,
};
pub use identity::{EmailOrigin, GitHubLoginMapping, Identity, IdentityError, KnownEmail};
pub use repository::{RepositoryIdentity, RepositoryIdentityError};
pub use validation::{EventValidationError, RequiredField};

/// Crate version, recorded alongside persisted data so migrations and
/// diagnostics can attribute results to the code that produced them.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The event schema version this crate currently emits.
///
/// Bump this whenever the persisted shape changes in a way that needs a
/// migration; readers must accept older versions rather than rejecting them.
pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion(1);

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_not_empty() {
        assert!(!super::CRATE_VERSION.is_empty());
    }
}

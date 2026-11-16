//! Event validation (spec 5.1, 5.3).
//!
//! # Why validation lives here and not in the database
//!
//! The same rules must run for native hooks, imported history, browser events and
//! Golden Test fixtures. Putting them in SQL would mean the browser path
//! (IndexedDB) silently accepts events the desktop path rejects.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use crate::event::{Event, EventType, SourceConfidence};

/// A field the event type requires but which was missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredField {
    /// `commit_sha`
    CommitSha,
    /// `external_id`
    ExternalId,
}

impl RequiredField {
    /// The field's persisted name.
    pub fn as_str(self) -> &'static str {
        match self {
            RequiredField::CommitSha => "commit_sha",
            RequiredField::ExternalId => "external_id",
        }
    }
}

/// Why an event was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventValidationError {
    /// A field this event type requires is absent.
    MissingRequiredField(RequiredField),
    /// The `local_date` shape is invalid.
    BadLocalDate(String),
    /// The timezone name is empty.
    EmptyTimezone,
    /// Timestamps are inconsistent with each other.
    InconsistentTimestamps {
        /// The timestamp that broke the ordering rule.
        field: &'static str,
    },
    /// The payload is not valid JSON.
    MalformedPayload(String),
    /// A commit sha is not a 40- or 64-character hex string.
    BadCommitSha(String),
}

impl std::fmt::Display for EventValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventValidationError::MissingRequiredField(field) => {
                write!(f, "missing required field: {}", field.as_str())
            }
            EventValidationError::BadLocalDate(d) => write!(f, "invalid local_date: {d}"),
            EventValidationError::EmptyTimezone => write!(f, "timezone must not be empty"),
            EventValidationError::InconsistentTimestamps { field } => {
                write!(f, "inconsistent timestamp: {field}")
            }
            EventValidationError::MalformedPayload(e) => {
                write!(f, "payload is not valid JSON: {e}")
            }
            EventValidationError::BadCommitSha(s) => write!(f, "invalid commit sha: {s}"),
        }
    }
}

impl std::error::Error for EventValidationError {}

/// Which fields an event type requires beyond the common set (spec 5.3).
pub fn required_fields_for(event_type: EventType) -> &'static [RequiredField] {
    match event_type {
        EventType::CommitCreated | EventType::LocalMerge => &[RequiredField::CommitSha],
        EventType::ReleasePublished
        | EventType::PullRequestOpened
        | EventType::PullRequestMerged
        | EventType::IssueOpened
        | EventType::IssueClosed
        | EventType::RepositoryCreated => &[RequiredField::ExternalId],
        EventType::StarGiven | EventType::StarReceived => &[RequiredField::ExternalId],
    }
}

/// Validate an event before it is persisted.
///
/// Checks are ordered cheapest-first, and each failure names the exact field so
/// diagnostics can be actionable instead of "invalid event".
pub fn validate(event: &Event) -> Result<(), EventValidationError> {
    if event.timezone.trim().is_empty() {
        return Err(EventValidationError::EmptyTimezone);
    }

    if crate::event::LocalDate::new(event.local_date.as_str()).is_none() {
        return Err(EventValidationError::BadLocalDate(
            event.local_date.0.clone(),
        ));
    }

    // occurred_at <= observed_at <= received_at.
    //
    // This catches the classic bug of stamping a replayed Spool event with the
    // consumption time, which would make week-old events look brand new and
    // trigger full notifications (spec 16.3).
    if event.observed_at < event.occurred_at {
        return Err(EventValidationError::InconsistentTimestamps {
            field: "observed_at < occurred_at",
        });
    }
    if event.received_at < event.observed_at {
        return Err(EventValidationError::InconsistentTimestamps {
            field: "received_at < observed_at",
        });
    }

    for field in required_fields_for(event.event_type) {
        let present = match field {
            RequiredField::CommitSha => event.commit_sha.as_deref().is_some_and(|s| !s.is_empty()),
            RequiredField::ExternalId => {
                event.external_id.as_deref().is_some_and(|s| !s.is_empty())
            }
        };
        if !present {
            return Err(EventValidationError::MissingRequiredField(*field));
        }
    }

    if let Some(sha) = event.commit_sha.as_deref() {
        if !sha.is_empty() && !is_hex_sha(sha) {
            return Err(EventValidationError::BadCommitSha(sha.to_string()));
        }
    }

    // Payload must be JSON, but an empty payload is not an error: some events have
    // no details worth persisting.
    if !event.payload.trim().is_empty()
        && serde_json::from_str::<serde_json::Value>(&event.payload).is_err()
    {
        return Err(EventValidationError::MalformedPayload(
            event.payload.clone(),
        ));
    }

    Ok(())
}

/// Whether a string looks like a Git object id (40 hex chars) or SHA-256 (64).
fn is_hex_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Should this event be recorded at all, given its confidence (spec 5.4)?
///
/// Kept separate from `validate`: an unverified event is still a valid event, it
/// just must not move official statistics.
pub fn is_recordable(event: &Event) -> bool {
    event.source_confidence.counts_toward_official_statistics()
        || matches!(event.source_confidence, SourceConfidence::Imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CanonicalRepositoryId, EventId, EventSource, LocalDate, SchemaVersion};

    fn sample(event_type: EventType) -> Event {
        Event {
            event_id: EventId::new("evt-1").unwrap(),
            event_type,
            source: EventSource::LocalGitHook,
            source_confidence: SourceConfidence::VerifiedLocal,
            canonical_repository_id: CanonicalRepositoryId("github:o/r".to_string()),
            external_id: Some("42".to_string()),
            commit_sha: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            occurred_at: 1_700_000_000_000,
            observed_at: 1_700_000_000_010,
            received_at: 1_700_000_000_020,
            local_date: LocalDate::new("2026-11-16").unwrap(),
            timezone: "Asia/Taipei".to_string(),
            date_policy: 0,
            payload: "{}".to_string(),
            schema_version: SchemaVersion(1),
        }
    }

    #[test]
    fn a_well_formed_commit_event_passes() {
        assert!(validate(&sample(EventType::CommitCreated)).is_ok());
    }

    #[test]
    fn commit_without_sha_is_rejected() {
        let mut e = sample(EventType::CommitCreated);
        e.commit_sha = None;
        assert_eq!(
            validate(&e),
            Err(EventValidationError::MissingRequiredField(
                RequiredField::CommitSha
            ))
        );
    }

    #[test]
    fn release_without_external_id_is_rejected() {
        let mut e = sample(EventType::ReleasePublished);
        e.external_id = None;
        assert_eq!(
            validate(&e),
            Err(EventValidationError::MissingRequiredField(
                RequiredField::ExternalId
            ))
        );
    }

    #[test]
    fn replay_stamped_with_consumption_time_is_rejected() {
        // The exact bug spec 16.3 warns about: using received_at as occurred_at
        // makes an old event look fresh and triggers a full notification.
        let mut e = sample(EventType::CommitCreated);
        e.occurred_at = 1_700_000_000_500; // later than observed_at
        assert!(matches!(
            validate(&e),
            Err(EventValidationError::InconsistentTimestamps { .. })
        ));
    }

    #[test]
    fn empty_timezone_is_rejected() {
        let mut e = sample(EventType::CommitCreated);
        e.timezone = "  ".to_string();
        assert_eq!(validate(&e), Err(EventValidationError::EmptyTimezone));
    }

    #[test]
    fn malformed_payload_is_rejected() {
        let mut e = sample(EventType::CommitCreated);
        e.payload = "{not json".to_string();
        assert!(matches!(
            validate(&e),
            Err(EventValidationError::MalformedPayload(_))
        ));
    }

    #[test]
    fn empty_payload_is_allowed() {
        let mut e = sample(EventType::CommitCreated);
        e.payload = String::new();
        assert!(validate(&e).is_ok());
    }

    #[test]
    fn bad_commit_sha_length_is_rejected() {
        let mut e = sample(EventType::CommitCreated);
        e.commit_sha = Some("abc".to_string());
        assert!(matches!(
            validate(&e),
            Err(EventValidationError::BadCommitSha(_))
        ));
    }

    #[test]
    fn sha256_length_commit_ids_are_accepted() {
        // Forward-compatible with SHA-256 repositories (spec 6.2 notes the risk).
        let mut e = sample(EventType::CommitCreated);
        e.commit_sha = Some("a".repeat(64));
        assert!(validate(&e).is_ok());
    }

    #[test]
    fn unverified_events_validate_but_are_not_recordable() {
        let mut e = sample(EventType::CommitCreated);
        e.source_confidence = SourceConfidence::Unverified;
        assert!(validate(&e).is_ok(), "shape is fine");
        assert!(!is_recordable(&e), "but it must not count");
    }

    #[test]
    fn imported_events_are_recordable() {
        let mut e = sample(EventType::CommitCreated);
        e.source_confidence = SourceConfidence::Imported;
        assert!(is_recordable(&e));
    }
}

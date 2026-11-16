//! The standard event (spec 5.3).
//!
//! Every field here exists because a specific downstream decision needs it. That
//! is deliberate: an event model that grows "just in case" fields becomes a place
//! where privacy leakage hides.

use serde::{Deserialize, Serialize};

/// Stable event identifier. Doubles as the idempotency key (spec 5.3), which is
/// what makes at-least-once delivery from the Spool harmless.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

impl EventId {
    /// Construct from a string, rejecting empty ids.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(EventId(value))
        }
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Canonical repository identifier (spec 6.1).
///
/// Kept as an opaque string in the event so the event does not depend on how the
/// identity happens to be derived today.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CanonicalRepositoryId(pub String);

impl CanonicalRepositoryId {
    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Local calendar date (`YYYY-MM-DD`) fixed at ingestion time (spec 12).
///
/// Stored as a string rather than a date type on purpose: this value is never
/// recomputed from a clock. It is a frozen historical fact about which "day" the
/// event belonged to under the timezone and day boundary in force at that moment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LocalDate(pub String);

impl LocalDate {
    /// Validate the `YYYY-MM-DD` shape and basic range.
    ///
    /// Full calendar validation (leap years, month lengths) is intentionally NOT
    /// duplicated here: the timezone provider produces this string, and a second
    /// implementation would be a second source of truth. We only reject values
    /// that could not possibly be a date.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let v = value.into();
        let b = v.as_bytes();
        if b.len() != 10 {
            return None;
        }
        if b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let year: u32 = v[0..4].parse().ok()?;
        let month: u32 = v[5..7].parse().ok()?;
        let day: u32 = v[8..10].parse().ok()?;
        if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day)
        {
            return None;
        }
        Some(LocalDate(v))
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Persisted schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaVersion(pub u32);

/// Which detector produced the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// A Git hook on the local machine.
    LocalGitHook,
    /// The browser extension observing GitHub.
    BrowserExtension,
    /// Bulk history import from an existing repository.
    HistoryImport,
}

/// How much this event can be trusted (spec 5.4).
///
/// Ordering matters: achievement rules must treat `Unverified` as not counting
/// toward official statistics by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceConfidence {
    /// Imported from pre-existing history; real but not observed live.
    Imported,
    /// Asserted by a source we could not verify.
    Unverified,
    /// Confirmed inside the browser against a logged-in GitHub session.
    BrowserConfirmed,
    /// Re-verified against the local repository.
    VerifiedLocal,
}

impl SourceConfidence {
    /// Whether events with this confidence count toward official statistics.
    ///
    /// Spec 5.4: `unverified` is excluded by default.
    pub fn counts_toward_official_statistics(self) -> bool {
        !matches!(self, SourceConfidence::Unverified)
    }
}

/// The event types supported at launch, plus the P2 addition (spec 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// A commit was created locally or on GitHub.
    CommitCreated,
    /// A release was published.
    ReleasePublished,
    /// A pull request was opened.
    PullRequestOpened,
    /// A pull request was merged.
    PullRequestMerged,
    /// An issue was opened.
    IssueOpened,
    /// An issue was closed.
    IssueClosed,
    /// A repository was created.
    RepositoryCreated,
    /// A star was given.
    StarGiven,
    /// A local (non-PR) merge. Deliberately distinct from `PullRequestMerged`
    /// because spec 2.3 forbids conflating them.
    LocalMerge,
    /// A star was received. P2: requires GitHub API authorisation.
    StarReceived,
}

impl EventType {
    /// Whether this event type is part of the launch set.
    pub fn is_launch_scope(self) -> bool {
        !matches!(self, EventType::StarReceived)
    }

    /// Whether counting this event requires the browser extension (spec 2.3).
    ///
    /// Used by the UI to show "needs browser extension" instead of a permanently
    /// zero progress bar.
    pub fn requires_browser_capability(self) -> bool {
        !matches!(self, EventType::CommitCreated | EventType::LocalMerge)
    }

    /// The wire/storage name.
    pub fn as_str(self) -> &'static str {
        match self {
            EventType::CommitCreated => "commit_created",
            EventType::ReleasePublished => "release_published",
            EventType::PullRequestOpened => "pull_request_opened",
            EventType::PullRequestMerged => "pull_request_merged",
            EventType::IssueOpened => "issue_opened",
            EventType::IssueClosed => "issue_closed",
            EventType::RepositoryCreated => "repository_created",
            EventType::StarGiven => "star_given",
            EventType::LocalMerge => "local_merge",
            EventType::StarReceived => "star_received",
        }
    }
}

/// A standardised event (spec 5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Idempotency key.
    pub event_id: EventId,
    /// What happened.
    pub event_type: EventType,
    /// Which detector saw it.
    pub source: EventSource,
    /// How much to trust it.
    pub source_confidence: SourceConfidence,
    /// Canonical repository identity (spec 6.1).
    pub canonical_repository_id: CanonicalRepositoryId,
    /// Provider-specific identifier (GitHub release id, PR number, ...).
    pub external_id: Option<String>,
    /// Commit SHA when the event refers to a commit.
    pub commit_sha: Option<String>,
    /// When it actually happened, Unix epoch milliseconds UTC.
    pub occurred_at: i64,
    /// When a detector observed it.
    pub observed_at: i64,
    /// When this system accepted it.
    pub received_at: i64,
    /// Frozen local date (spec 12).
    pub local_date: LocalDate,
    /// IANA zone used to compute `local_date`.
    pub timezone: String,
    /// Day boundary in minutes from local midnight that was in force.
    pub date_policy: u16,
    /// Event-specific data. Must never contain secrets (AGENTS.md 7).
    pub payload: String,
    /// Persisted shape version.
    pub schema_version: SchemaVersion,
}

impl Event {
    /// The primary deduplication key for commit-like events (spec 6.2).
    ///
    /// `canonical_repository_id + commit_sha`. Patch id is deliberately NOT used
    /// as an absolute key: the same patch can legitimately exist on two branches.
    pub fn commit_dedup_key(&self) -> Option<String> {
        let sha = self.commit_sha.as_deref()?;
        if sha.is_empty() {
            return None;
        }
        Some(format!("{}:{}", self.canonical_repository_id.as_str(), sha))
    }

    /// Age of the event relative to a reference instant, in milliseconds.
    ///
    /// Negative ages are clamped to zero: a clock skew must not make an event
    /// look "in the future and therefore fresh forever".
    pub fn age_ms(&self, now_ms: i64) -> i64 {
        (now_ms - self.occurred_at).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Event {
        Event {
            event_id: EventId::new("evt-1").expect("non-empty"),
            event_type: EventType::CommitCreated,
            source: EventSource::LocalGitHook,
            source_confidence: SourceConfidence::VerifiedLocal,
            canonical_repository_id: CanonicalRepositoryId("repo-1".to_string()),
            external_id: None,
            commit_sha: Some("c0ffee".to_string()),
            occurred_at: 1_700_000_000_000,
            observed_at: 1_700_000_000_100,
            received_at: 1_700_000_000_200,
            local_date: LocalDate::new("2026-11-16").expect("valid date"),
            timezone: "Asia/Taipei".to_string(),
            date_policy: 0,
            payload: "{}".to_string(),
            schema_version: super::super::CURRENT_SCHEMA_VERSION,
        }
    }

    #[test]
    fn empty_event_id_is_rejected() {
        assert!(EventId::new("").is_none());
        assert!(EventId::new("   ").is_none());
    }

    #[test]
    fn local_date_requires_the_wire_shape() {
        assert!(LocalDate::new("2026-11-16").is_some());
        assert!(
            LocalDate::new("2026-1-16").is_none(),
            "month must be zero padded"
        );
        assert!(LocalDate::new("2026/11/16").is_none());
        assert!(LocalDate::new("not-a-date").is_none());
        assert!(LocalDate::new("2026-13-01").is_none(), "month out of range");
        assert!(LocalDate::new("2026-00-01").is_none());
        assert!(LocalDate::new("1969-12-31").is_none(), "before the epoch");
    }

    #[test]
    fn unverified_events_do_not_count_by_default() {
        // Spec 5.4: 'unverified' is excluded from official statistics.
        assert!(!SourceConfidence::Unverified.counts_toward_official_statistics());
        assert!(SourceConfidence::VerifiedLocal.counts_toward_official_statistics());
        assert!(SourceConfidence::BrowserConfirmed.counts_toward_official_statistics());
        assert!(SourceConfidence::Imported.counts_toward_official_statistics());
    }

    #[test]
    fn dedup_key_is_repository_plus_sha() {
        let e = sample();
        assert_eq!(e.commit_dedup_key().as_deref(), Some("repo-1:c0ffee"));
    }

    #[test]
    fn dedup_key_is_absent_without_a_sha() {
        let mut e = sample();
        e.commit_sha = None;
        assert!(e.commit_dedup_key().is_none());
    }

    #[test]
    fn local_merge_and_pr_merge_are_distinct() {
        // Spec 2.3: local merge must never be conflated with PR merge.
        assert_ne!(EventType::LocalMerge, EventType::PullRequestMerged);
        assert_eq!(EventType::LocalMerge.as_str(), "local_merge");
        assert_eq!(EventType::PullRequestMerged.as_str(), "pull_request_merged");
    }

    #[test]
    fn desktop_only_capability_is_reported_for_browser_events() {
        // Spec 2.3: the UI must say "needs browser extension" rather than show 0.
        assert!(!EventType::CommitCreated.requires_browser_capability());
        assert!(!EventType::LocalMerge.requires_browser_capability());
        assert!(EventType::ReleasePublished.requires_browser_capability());
        assert!(EventType::PullRequestMerged.requires_browser_capability());
        assert!(EventType::StarGiven.requires_browser_capability());
    }

    #[test]
    fn star_received_is_out_of_launch_scope() {
        // Spec 5.1: Star Received is P2 and needs GitHub API authorisation.
        assert!(!EventType::StarReceived.is_launch_scope());
        assert!(EventType::CommitCreated.is_launch_scope());
    }

    #[test]
    fn age_is_clamped_for_future_timestamps() {
        // A clock skew must not make an event look eternally fresh.
        let e = sample();
        assert_eq!(e.age_ms(e.occurred_at + 5_000), 5_000);
        assert_eq!(e.age_ms(e.occurred_at - 5_000), 0);
    }

    #[test]
    fn event_round_trips_through_json() {
        let e = sample();
        let json = serde_json::to_string(&e).expect("serialise");
        let back: Event = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(e, back);
    }
}

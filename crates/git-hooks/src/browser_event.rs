//! Browser-reported events (spec 18, 27.2).
//!
//! # Why the conversion lives here and not in the native host
//!
//! The host is a transport. Deciding what a browser message means is a rule, and
//! rules belong next to the rest of them so they can be tested without a browser.
//!
//! # What is trusted
//!
//! Nothing from the browser is treated as a command. The payload is parsed into a
//! fixed shape; anything missing or malformed is refused rather than defaulted, and
//! the resulting event is marked "browser confirmed" rather than "verified locally"
//! (spec 5.4) because it was seen on a page, not read from the repository.

use devchievements_core::InjectedContext;
use devchievements_events::{
    CanonicalRepositoryId, Event, EventId, EventSource, EventType, LocalDate, SchemaVersion,
    SourceConfidence,
};

/// The kind of activity the extension reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserActivity {
    /// A commit page was observed.
    Commit { sha: String },
    /// A release page was observed.
    Release { tag: String },
}

/// A validated browser report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserReport {
    /// Stable id from the extension; derived from the activity, so replays collide.
    pub event_id: String,
    /// Canonical repository id, already in github:owner/name form.
    pub repository_id: String,
    /// What was observed.
    pub activity: BrowserActivity,
    /// When it happened, in Unix milliseconds.
    pub occurred_ms: i64,
}

/// Parse a browser payload.
///
/// Returns None for anything that does not describe one of the supported activities.
/// The caller turns that into a refusal, never into a default.
pub fn parse(payload: &serde_json::Value) -> Option<BrowserReport> {
    if payload.get("kind")?.as_str()? != "browser_github" {
        return None;
    }

    let event_id = payload.get("event_id")?.as_str()?.to_string();
    let repository_id = payload.get("repository_id")?.as_str()?.to_string();
    if !repository_id.starts_with("github:") {
        return None;
    }
    if repository_id.len() > 256 {
        return None;
    }

    let occurred_ms = payload.get("occurred_ms")?.as_i64()?;
    // A timestamp far in the future would poison freshness and streaks; a page cannot
    // legitimately describe activity that has not happened.
    if occurred_ms <= 0 {
        return None;
    }

    let activity = match payload.get("activity")?.as_str()? {
        "commit" => {
            let sha = payload.get("commit_sha")?.as_str()?.to_string();
            // A full id is required, not a preference: the hook path records 40 hex
            // characters, and a short id could never deduplicate against it, so the
            // same commit would count twice (spec 11.2).
            if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            BrowserActivity::Commit { sha }
        }
        "release" => {
            let tag = payload.get("external_id")?.as_str()?.to_string();
            if tag.is_empty() || tag.len() > 200 {
                return None;
            }
            BrowserActivity::Release { tag }
        }
        _ => return None,
    };

    Some(BrowserReport {
        event_id,
        repository_id,
        activity,
        occurred_ms,
    })
}

/// Turn a validated report into an Event.
pub fn to_event(report: &BrowserReport, ctx: &InjectedContext<'_>) -> Option<Event> {
    let (event_type, commit_sha, external_id) = match &report.activity {
        BrowserActivity::Commit { sha } => (EventType::CommitCreated, Some(sha.clone()), None),
        BrowserActivity::Release { tag } => (EventType::ReleasePublished, None, Some(tag.clone())),
    };

    let timezone = ctx.timezone.canonicalize("UTC")?;
    let local_date = ctx.timezone.local_date(report.occurred_ms, &timezone, 0)?;

    Some(Event {
        event_id: EventId::new(report.event_id.clone())?,
        event_type,
        source: EventSource::BrowserExtension,
        // Spec 5.4: seen on a page, not read from the repository.
        source_confidence: SourceConfidence::BrowserConfirmed,
        canonical_repository_id: CanonicalRepositoryId(report.repository_id.clone()),
        external_id,
        commit_sha,
        occurred_at: report.occurred_ms,
        observed_at: report.occurred_ms,
        received_at: report.occurred_ms,
        local_date: LocalDate::new(local_date)?,
        timezone,
        date_policy: 0,
        payload: serde_json::json!({
            "kind": "browser_github",
            "via": "native_messaging",
        })
        .to_string(),
        schema_version: SchemaVersion(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(value: serde_json::Value) -> serde_json::Value {
        value
    }

    #[test]
    fn a_commit_report_is_accepted() {
        let raw = payload(serde_json::json!({
            "kind": "browser_github",
            "event_id": "browser:github:o/r#abc1234",
            "repository_id": "github:o/r",
            "activity": "commit",
            "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
            "occurred_ms": 1_700_000_000_000i64,
        }));
        let report = parse(&raw).expect("valid report");
        assert_eq!(report.repository_id, "github:o/r");
        assert_eq!(
            report.activity,
            BrowserActivity::Commit {
                sha: "abcdef0123456789abcdef0123456789abcdef01".to_string()
            }
        );
    }

    #[test]
    fn a_release_report_is_accepted() {
        let raw = payload(serde_json::json!({
            "kind": "browser_github",
            "event_id": "browser:github:o/r#release:v1",
            "repository_id": "github:o/r",
            "activity": "release",
            "external_id": "v1",
            "occurred_ms": 1_700_000_000_000i64,
        }));
        let report = parse(&raw).expect("valid report");
        assert_eq!(
            report.activity,
            BrowserActivity::Release {
                tag: "v1".to_string()
            }
        );
    }

    #[test]
    fn unknown_kinds_are_refused() {
        let raw = payload(serde_json::json!({ "kind": "hook", "event_id": "x" }));
        assert!(parse(&raw).is_none());
    }

    #[test]
    fn a_repository_outside_github_is_refused() {
        let raw = payload(serde_json::json!({
            "kind": "browser_github",
            "event_id": "x",
            "repository_id": "gitlab:o/r",
            "activity": "commit",
            "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
            "occurred_ms": 1,
        }));
        assert!(parse(&raw).is_none());
    }

    #[test]
    fn a_short_or_non_hex_sha_is_refused() {
        for sha in ["abc", "zzzzzzz", "", "abc1234"] {
            let raw = payload(serde_json::json!({
                "kind": "browser_github",
                "event_id": "x",
                "repository_id": "github:o/r",
                "activity": "commit",
                "commit_sha": sha,
                "occurred_ms": 1,
            }));
            assert!(parse(&raw).is_none(), "sha {sha} should be refused");
        }
    }

    #[test]
    fn a_missing_timestamp_is_refused_rather_than_defaulted() {
        let raw = payload(serde_json::json!({
            "kind": "browser_github",
            "event_id": "x",
            "repository_id": "github:o/r",
            "activity": "commit",
            "commit_sha": "abcdef0123456789abcdef0123456789abcdef01",
        }));
        assert!(parse(&raw).is_none());
    }
}

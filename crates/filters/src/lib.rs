//! User event filters and statistics scope (spec 15).
//!
//! # The distinction this crate exists to enforce
//!
//! There are two different ways to count something, and conflating them is a spec
//! violation with visible consequences:
//!
//! - **Official profile**: fixed counting rules used by built-in permanent
//!   achievements. A user who excludes a repository from their dashboard must NOT
//!   thereby un-count the commits behind "100 Commits" (spec 13.3).
//! - **User-filtered profile**: whatever the dashboard and ordinary statistics
//!   respect (spec 15).
//!
//! So a filter set is a thing you *apply*, not a global setting that silently
//! rewrites everything. [`StatisticsScope`] makes that explicit at every call site.
//!
//! # Why filters are data and not closures
//!
//! Filters are persisted with the configuration and exported/imported. They must
//! therefore be serialisable, comparable and inspectable, and they must produce the
//! same answer after a round trip through JSON. A closure would be none of those.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use devchievements_events::{Event, EventType, SourceConfidence};
use serde::{Deserialize, Serialize};

/// Which statistics profile a number belongs to.
///
/// Every place that displays a count must say which one it is using (spec 15, 24.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsScope {
    /// Built-in achievements: filters do not apply.
    Official,
    /// Dashboard and ordinary statistics: filters apply.
    UserFiltered,
}

impl StatisticsScope {
    /// A human-facing label for the UI, because spec 15 requires the interface to
    /// state which counting rules produced a number.
    pub fn label(self) -> &'static str {
        match self {
            StatisticsScope::Official => "Fixed official rules",
            StatisticsScope::UserFiltered => "User filter settings",
        }
    }

    /// Whether a filter set should be consulted under this scope.
    pub fn applies_filters(self) -> bool {
        matches!(self, StatisticsScope::UserFiltered)
    }
}

/// How a repository selector treats its list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RepositorySelection {
    /// No restriction.
    All,
    /// Only these canonical repository ids.
    Only {
        /// Canonical ids to include.
        ids: Vec<String>,
    },
    /// Everything except these canonical repository ids.
    Except {
        /// Canonical ids to exclude.
        ids: Vec<String>,
    },
}

impl RepositorySelection {
    /// Whether a repository passes this selector.
    pub fn allows(&self, repository_id: &str) -> bool {
        match self {
            RepositorySelection::All => true,
            RepositorySelection::Only { ids } => ids.iter().any(|id| id == repository_id),
            RepositorySelection::Except { ids } => !ids.iter().any(|id| id == repository_id),
        }
    }
}

/// A closed time range, in Unix epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Inclusive start.
    pub from_ms: i64,
    /// Inclusive end.
    pub to_ms: i64,
}

impl TimeRange {
    /// Whether an instant falls inside.
    pub fn contains(&self, instant_ms: i64) -> bool {
        instant_ms >= self.from_ms && instant_ms <= self.to_ms
    }
}

/// The full filter configuration (spec 15 conditions).
///
/// Every field is optional or defaults to "no restriction", which matters: a
/// freshly installed app must count everything, not nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterSet {
    /// Which repositories count.
    pub repositories: RepositorySelection,
    /// Only count events attributed to these author emails. Empty means no limit.
    pub author_emails: Vec<String>,
    /// Whether merge commits count (spec 8.1 groups them separately).
    pub include_merges: bool,
    /// Whether imported history counts.
    pub include_imported: bool,
    /// Confidence levels that count. Empty means "all except unverified".
    pub allowed_confidence: Vec<SourceConfidence>,
    /// Optional time window.
    pub time_range: Option<TimeRange>,
    /// Event types that count. Empty means "all supported types".
    pub event_types: Vec<EventType>,
}

impl Default for FilterSet {
    /// The default is deliberately permissive: no repository restriction, no author
    /// restriction, merges excluded (spec 8.1), imported history included (the user
    /// imported it on purpose), unverified excluded (spec 5.4).
    fn default() -> Self {
        FilterSet {
            repositories: RepositorySelection::All,
            author_emails: Vec::new(),
            include_merges: false,
            include_imported: true,
            allowed_confidence: vec![
                SourceConfidence::VerifiedLocal,
                SourceConfidence::BrowserConfirmed,
                SourceConfidence::Imported,
            ],
            time_range: None,
            event_types: Vec::new(),
        }
    }
}

/// Why an event was or was not counted, for the "why is this number different?"
/// question the UI has to answer (spec 15, 24.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionReason {
    /// The repository is filtered out.
    Repository,
    /// The author is not in the allowed list.
    Author,
    /// Merge events are excluded by configuration.
    Merge,
    /// Imported history is excluded by configuration.
    Imported,
    /// The confidence level is not allowed.
    Confidence,
    /// Outside the configured time range.
    TimeRange,
    /// The event type is not in the allowed list.
    EventType,
}

impl FilterSet {
    /// Decide whether an event counts under this filter set.
    ///
    /// Returns `Ok(())` when it counts, otherwise the first reason it was excluded.
    /// Only the first reason is reported: showing a user six simultaneous reasons
    /// for one excluded commit is noise, and the first one is the actionable one.
    pub fn evaluate(&self, event: &Event) -> Result<(), ExclusionReason> {
        if !self
            .repositories
            .allows(event.canonical_repository_id.as_str())
        {
            return Err(ExclusionReason::Repository);
        }

        if !self.event_types.is_empty() && !self.event_types.contains(&event.event_type) {
            return Err(ExclusionReason::EventType);
        }

        // Merge commits are a separate classification (spec 8.1). The Engine marks
        // them by event type; a commit event that reached here as a merge would have
        // been filtered upstream, so this checks the event type instead.
        if !self.include_merges && matches!(event.event_type, EventType::LocalMerge) {
            return Err(ExclusionReason::Merge);
        }

        if !self.include_imported && event.source_confidence == SourceConfidence::Imported {
            return Err(ExclusionReason::Imported);
        }

        if !self.allowed_confidence.is_empty()
            && !self.allowed_confidence.contains(&event.source_confidence)
        {
            return Err(ExclusionReason::Confidence);
        }

        if !self.author_emails.is_empty() {
            let email = extract_author_email(&event.payload);
            match email {
                Some(email) if author_matches(&email, &self.author_emails) => {}
                _ => return Err(ExclusionReason::Author),
            }
        }

        if let Some(range) = &self.time_range {
            if !range.contains(event.occurred_at) {
                return Err(ExclusionReason::TimeRange);
            }
        }

        Ok(())
    }

    /// Whether an event counts.
    pub fn counts(&self, event: &Event) -> bool {
        self.evaluate(event).is_ok()
    }
}

/// Pull `author_email` out of an event payload.
///
/// Returns `None` when absent or malformed; a filter that needs an author must not
/// silently match an event that has none.
fn extract_author_email(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    value.get("author_email")?.as_str().map(|s| s.to_string())
}

/// Case-insensitive author comparison (spec 7: emails are an attribution heuristic).
fn author_matches(candidate: &str, allowed: &[String]) -> bool {
    let needle = candidate.trim().to_ascii_lowercase();
    allowed
        .iter()
        .any(|a| a.trim().to_ascii_lowercase() == needle)
}

/// Count events under a given scope.
///
/// This is the function the dashboard should use, and it forces the caller to say
/// which scope it wants. Under [`StatisticsScope::Official`] the filter set is
/// ignored entirely, which is what keeps built-in achievements stable.
pub fn count<'a, I>(events: I, filters: &FilterSet, scope: StatisticsScope) -> usize
where
    I: IntoIterator<Item = &'a Event>,
{
    if !scope.applies_filters() {
        // Official scope: count every event of a counting type, ignoring filters.
        return events
            .into_iter()
            .filter(|e| {
                !matches!(e.event_type, EventType::LocalMerge)
                    && e.source_confidence != SourceConfidence::Unverified
            })
            .count();
    }
    events.into_iter().filter(|e| filters.counts(e)).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devchievements_events::{
        CanonicalRepositoryId, EventId, EventSource, LocalDate, SchemaVersion,
    };

    fn event(repo: &str, email: &str, confidence: SourceConfidence) -> Event {
        Event {
            event_id: EventId::new("e1").expect("id"),
            event_type: EventType::CommitCreated,
            source: EventSource::LocalGitHook,
            source_confidence: confidence,
            canonical_repository_id: CanonicalRepositoryId(repo.to_string()),
            external_id: None,
            commit_sha: Some("a".repeat(40)),
            occurred_at: 1_794_787_200_000,
            observed_at: 1_794_787_200_000,
            received_at: 1_794_787_200_000,
            local_date: LocalDate::new("2026-11-16").expect("date"),
            timezone: "UTC".to_string(),
            date_policy: 0,
            payload: format!("{{\"author_email\":\"{email}\"}}"),
            schema_version: SchemaVersion(1),
        }
    }

    fn mine() -> Event {
        event(
            "github:o/r",
            "me@example.com",
            SourceConfidence::VerifiedLocal,
        )
    }

    #[test]
    fn the_default_filter_counts_everything_permissible() {
        let f = FilterSet::default();
        assert!(f.counts(&mine()));
    }

    #[test]
    fn a_fresh_install_counts_rather_than_excludes() {
        // A filter that defaults to excluding would show an empty dashboard on day
        // one and look broken.
        let f = FilterSet::default();
        assert!(f.counts(&event(
            "github:a/b",
            "x@y.z",
            SourceConfidence::BrowserConfirmed
        )));
        assert!(f.counts(&event(
            "github:c/d",
            "other@x.y",
            SourceConfidence::Imported
        )));
    }

    #[test]
    fn unverified_events_never_count_by_default() {
        // Spec 5.4.
        let f = FilterSet::default();
        assert_eq!(
            f.evaluate(&event(
                "github:o/r",
                "me@example.com",
                SourceConfidence::Unverified
            )),
            Err(ExclusionReason::Confidence)
        );
    }

    #[test]
    fn only_selection_includes_listed_repositories_and_excludes_others() {
        let f = FilterSet {
            repositories: RepositorySelection::Only {
                ids: vec!["github:o/r".to_string()],
            },
            ..Default::default()
        };
        assert!(f.counts(&mine()));
        assert_eq!(
            f.evaluate(&event(
                "github:x/y",
                "me@example.com",
                SourceConfidence::VerifiedLocal
            )),
            Err(ExclusionReason::Repository)
        );
    }

    #[test]
    fn except_selection_excludes_listed_repositories() {
        let f = FilterSet {
            repositories: RepositorySelection::Except {
                ids: vec!["github:o/r".to_string()],
            },
            ..Default::default()
        };
        assert_eq!(f.evaluate(&mine()), Err(ExclusionReason::Repository));
        assert!(f.counts(&event(
            "github:x/y",
            "me@example.com",
            SourceConfidence::VerifiedLocal
        )));
    }

    #[test]
    fn author_filter_is_case_insensitive() {
        let f = FilterSet {
            author_emails: vec!["ME@example.com".to_string()],
            ..Default::default()
        };
        assert!(f.counts(&event(
            "github:o/r",
            "me@EXAMPLE.com",
            SourceConfidence::VerifiedLocal
        )));
    }

    #[test]
    fn an_author_filter_rejects_events_with_no_author_information() {
        // Silently matching "no author" would let unrelated commits through a filter
        // the user set precisely to exclude them.
        let f = FilterSet {
            author_emails: vec!["me@example.com".to_string()],
            ..Default::default()
        };
        let mut e = mine();
        e.payload = "{}".to_string();
        assert_eq!(f.evaluate(&e), Err(ExclusionReason::Author));
    }

    #[test]
    fn merge_events_are_excluded_by_default_and_includable() {
        let mut e = mine();
        e.event_type = EventType::LocalMerge;

        let strict = FilterSet::default();
        assert_eq!(strict.evaluate(&e), Err(ExclusionReason::Merge));

        let permissive = FilterSet {
            include_merges: true,
            ..Default::default()
        };
        assert!(permissive.counts(&e));
    }

    #[test]
    fn imported_history_can_be_excluded() {
        let f = FilterSet {
            include_imported: false,
            ..Default::default()
        };
        assert_eq!(
            f.evaluate(&event(
                "github:o/r",
                "me@example.com",
                SourceConfidence::Imported
            )),
            Err(ExclusionReason::Imported)
        );
    }

    #[test]
    fn time_range_is_inclusive_on_both_ends() {
        let range = TimeRange {
            from_ms: 100,
            to_ms: 200,
        };
        assert!(range.contains(100), "start is inclusive");
        assert!(range.contains(200), "end is inclusive");
        assert!(!range.contains(99));
        assert!(!range.contains(201));
    }

    #[test]
    fn a_time_range_excludes_events_outside_it() {
        let f = FilterSet {
            time_range: Some(TimeRange {
                from_ms: 0,
                to_ms: 1_000,
            }),
            ..Default::default()
        };
        assert_eq!(f.evaluate(&mine()), Err(ExclusionReason::TimeRange));
    }

    #[test]
    fn event_type_list_restricts_counting() {
        let f = FilterSet {
            event_types: vec![EventType::ReleasePublished],
            ..Default::default()
        };
        assert_eq!(f.evaluate(&mine()), Err(ExclusionReason::EventType));
    }

    #[test]
    fn official_scope_ignores_filters_entirely() {
        // THE critical guarantee (spec 13.3): excluding a repository from the
        // dashboard must not reduce built-in achievement progress.
        let events = [mine()];
        let excludes_everything = FilterSet {
            repositories: RepositorySelection::Only { ids: Vec::new() },
            ..Default::default()
        };

        assert_eq!(
            count(
                events.iter(),
                &excludes_everything,
                StatisticsScope::UserFiltered
            ),
            0,
            "the user filter really does exclude it"
        );
        assert_eq!(
            count(
                events.iter(),
                &excludes_everything,
                StatisticsScope::Official
            ),
            1,
            "but official counting must be unaffected"
        );
    }

    #[test]
    fn official_scope_still_excludes_unverified_events() {
        // Ignoring user filters does not mean ignoring the source-confidence rule.
        let events = [event(
            "github:o/r",
            "me@example.com",
            SourceConfidence::Unverified,
        )];
        assert_eq!(
            count(
                events.iter(),
                &FilterSet::default(),
                StatisticsScope::Official
            ),
            0
        );
    }

    #[test]
    fn official_scope_still_excludes_merges_from_commit_counts() {
        let mut e = mine();
        e.event_type = EventType::LocalMerge;
        let events = [e];
        assert_eq!(
            count(
                events.iter(),
                &FilterSet::default(),
                StatisticsScope::Official
            ),
            0
        );
    }

    #[test]
    fn scopes_carry_distinguishable_labels_for_the_ui() {
        // Spec 15 requires the interface to state the counting rules in force.
        assert_ne!(
            StatisticsScope::Official.label(),
            StatisticsScope::UserFiltered.label()
        );
        assert!(StatisticsScope::Official.label().contains("official"));
    }

    #[test]
    fn filter_sets_survive_a_json_round_trip() {
        // Filters are exported with the configuration, so equality after a round trip
        // is a functional requirement, not a nicety.
        let f = FilterSet {
            repositories: RepositorySelection::Only {
                ids: vec!["github:o/r".to_string()],
            },
            author_emails: vec!["me@example.com".to_string()],
            include_merges: true,
            include_imported: false,
            allowed_confidence: vec![SourceConfidence::VerifiedLocal],
            time_range: Some(TimeRange {
                from_ms: 1,
                to_ms: 2,
            }),
            event_types: vec![EventType::CommitCreated],
        };
        let json = serde_json::to_string(&f).expect("serialise");
        let back: FilterSet = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(f, back);
    }

    #[test]
    fn only_the_first_exclusion_reason_is_reported() {
        // Reporting every simultaneous reason would be noise; the first is the
        // actionable one.
        let f = FilterSet {
            repositories: RepositorySelection::Only { ids: vec![] },
            include_merges: false,
            ..Default::default()
        };
        let mut e = mine();
        e.event_type = EventType::LocalMerge;
        assert_eq!(f.evaluate(&e), Err(ExclusionReason::Repository));
    }
}

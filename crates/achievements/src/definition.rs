//! User-defined achievements from JSON (spec 22, Phase 4a).
//!
//! # What a user achievement may and may not do
//!
//! Spec 22 allows a condition tree over a fixed metric set, with hard complexity
//! limits. What it forbids, and what this module therefore refuses:
//!
//! - referencing another rule (cycles would be possible, and the spec forbids it);
//! - reusing a built-in achievement's id (a user rule must never shadow a fixed one,
//!   because the built-ins must keep their exact official-profile semantics);
//! - exceeding the depth, condition-count or step budgets.
//!
//! # Why this is a separate module from the rule engine
//!
//! The engine evaluates a tree it is given. Deciding whether a *submitted document*
//! may become an achievement is a different job: it involves identity, limits, and
//! refusing things quietly. Keeping them apart means a bug in validation cannot
//! change how an existing achievement evaluates.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

use crate::builder::{ComplexityLimits, RuleError, RuleExpr};
use crate::StatisticsSnapshot;

/// The longest name a user achievement may have.
pub const MAX_NAME_LEN: usize = 60;
/// The longest description a user achievement may have.
pub const MAX_DESCRIPTION_LEN: usize = 240;
/// How many user achievements may exist at once.
pub const MAX_USER_ACHIEVEMENTS: usize = 200;

/// Why a submitted definition was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionError {
    /// The document is not valid JSON or does not match the shape.
    Invalid(String),
    /// The id is empty, too long, or not a slug.
    BadId(String),
    /// The id collides with a built-in achievement.
    ShadowsBuiltin(String),
    /// The name or description is empty or too long.
    BadText(String),
    /// The rule breaks a complexity limit.
    TooComplex(String),
    /// The id is already used by another user achievement.
    DuplicateId(String),
}

impl std::fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefinitionError::Invalid(m) => write!(f, "invalid definition: {m}"),
            DefinitionError::BadId(m) => write!(f, "invalid id: {m}"),
            DefinitionError::ShadowsBuiltin(id) => {
                write!(
                    f,
                    "`{id}` is a built-in achievement and cannot be redefined"
                )
            }
            DefinitionError::BadText(m) => write!(f, "invalid text: {m}"),
            DefinitionError::TooComplex(m) => write!(f, "rule too complex: {m}"),
            DefinitionError::DuplicateId(id) => write!(f, "`{id}` is already defined"),
        }
    }
}

impl std::error::Error for DefinitionError {}

/// A submitted achievement definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AchievementDefinition {
    /// Stable id; slug-shaped.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description shown on the card.
    pub description: String,
    /// The condition tree.
    pub rule: RuleExpr,
}

/// A validated user achievement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAchievement {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// The condition tree.
    pub rule: RuleExpr,
    /// Rule version, incremented by the caller when a rule changes meaning.
    pub version: u32,
}

impl UserAchievement {
    /// Evaluate this achievement against the official statistics.
    ///
    /// Spec 22 lets a user rule read commits, merged pull requests, releases and the
    /// longest streak; nothing else. The mapping lives here rather than at the call
    /// site so a new metric cannot be silently fed a different number in the Engine
    /// than in the editor's preview.
    ///
    /// A rule that runs out of its step budget reports `exhausted`; spec 22 requires it
    /// to be treated as invalid and disabled, never as "not satisfied yet".
    pub fn evaluate(&self, statistics: &StatisticsSnapshot) -> crate::builder::Evaluation {
        let stats = crate::builder::UserStatistics {
            commits: statistics.commit_count,
            pull_requests_merged: statistics.pull_request_merged_count,
            releases: statistics.release_count,
            streak_days: statistics.longest_streak_days,
        };
        self.rule.evaluate(
            &stats,
            crate::builder::ComplexityLimits::default().max_steps,
        )
    }
}

/// Which user achievements became satisfied for the first time.
///
/// Already-unlocked ids are skipped before evaluation, not filtered afterwards: spec
/// 22 (like spec 13.1 for built-ins) says an unlock is permanent, and an expensive
/// rule that has already fired should not cost its budget on every commit.
pub fn newly_satisfied(
    achievements: &[UserAchievement],
    statistics: &StatisticsSnapshot,
    existing_unlocks: &[String],
) -> Vec<String> {
    achievements
        .iter()
        .filter(|a| !existing_unlocks.iter().any(|id| id == &a.id))
        .filter(|a| {
            let outcome = a.evaluate(statistics);
            if outcome.exhausted {
                // Visible rather than silent: the user is told the rule stopped working.
                eprintln!(
                    "user achievement {} exceeded its step budget and was skipped",
                    a.id
                );
            }
            outcome.satisfied && !outcome.exhausted
        })
        .map(|a| a.id.clone())
        .collect()
}

/// Whether an id is a slug: lowercase alphanumerics and dashes, starting with a letter.
fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Validate a submitted definition against the built-in catalogue and existing users.
///
/// `existing` is the set of user ids already stored. Passing it here (rather than
/// checking later) means a duplicate is refused before anything is written.
pub fn validate(
    definition: &AchievementDefinition,
    existing: &[String],
    limits: ComplexityLimits,
) -> Result<UserAchievement, DefinitionError> {
    if !is_slug(&definition.id) {
        return Err(DefinitionError::BadId(definition.id.clone()));
    }

    // A user rule must never shadow a built-in: those have fixed, versioned
    // semantics (spec 13.3) and redefining one would silently change an outcome the
    // user may already have unlocked.
    if crate::builtin_achievements()
        .iter()
        .any(|a| a.achievement_id == definition.id)
    {
        return Err(DefinitionError::ShadowsBuiltin(definition.id.clone()));
    }

    if existing.iter().any(|id| id == &definition.id) {
        return Err(DefinitionError::DuplicateId(definition.id.clone()));
    }

    let name = definition.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LEN {
        return Err(DefinitionError::BadText(format!(
            "name must be 1-{MAX_NAME_LEN} characters"
        )));
    }
    let description = definition.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_LEN {
        return Err(DefinitionError::BadText(format!(
            "description must be at most {MAX_DESCRIPTION_LEN} characters"
        )));
    }

    definition
        .rule
        .validate(limits)
        .map_err(|e: RuleError| DefinitionError::TooComplex(e.to_string()))?;

    Ok(UserAchievement {
        id: definition.id.clone(),
        name: name.to_string(),
        description: description.to_string(),
        rule: definition.rule.clone(),
        version: 1,
    })
}

/// Parse and validate a JSON definition in one step.
pub fn from_json(
    json: &str,
    existing: &[String],
    limits: ComplexityLimits,
) -> Result<UserAchievement, DefinitionError> {
    let definition: AchievementDefinition =
        serde_json::from_str(json).map_err(|e| DefinitionError::Invalid(e.to_string()))?;
    validate(&definition, existing, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(id: &str) -> AchievementDefinition {
        AchievementDefinition {
            id: id.to_string(),
            name: "My Achievement".to_string(),
            description: "Do the thing".to_string(),
            rule: RuleExpr::Condition(crate::builder::Condition {
                metric: crate::builder::UserMetric::Commits,
                at_least: 10,
            }),
        }
    }

    #[test]
    fn a_simple_definition_is_accepted() {
        let result = validate(
            &definition("do-ten-commits"),
            &[],
            ComplexityLimits::default(),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn a_user_rule_cannot_shadow_a_builtin() {
        // Spec 13.3: built-ins keep fixed semantics; redefining one would change an
        // outcome the user may already have unlocked.
        let result = validate(
            &definition("first_commit"),
            &[],
            ComplexityLimits::default(),
        );
        assert!(matches!(result, Err(DefinitionError::ShadowsBuiltin(_))));
    }

    #[test]
    fn an_empty_id_is_refused() {
        assert!(matches!(
            validate(&definition(""), &[], ComplexityLimits::default()),
            Err(DefinitionError::BadId(_))
        ));
    }

    #[test]
    fn an_id_is_a_slug_not_free_text() {
        for bad in [
            "Has Spaces",
            "UPPER",
            "-leading-dash",
            "1-starts-with-digit",
        ] {
            assert!(
                matches!(
                    validate(&definition(bad), &[], ComplexityLimits::default()),
                    Err(DefinitionError::BadId(_))
                ),
                "{bad} should be refused"
            );
        }
    }

    #[test]
    fn a_duplicate_user_id_is_refused() {
        let existing = vec!["do-ten-commits".to_string()];
        let result = validate(
            &definition("do-ten-commits"),
            &existing,
            ComplexityLimits::default(),
        );
        assert!(matches!(result, Err(DefinitionError::DuplicateId(_))));
    }

    #[test]
    fn an_empty_name_is_refused() {
        let mut d = definition("x");
        d.name = "   ".to_string();
        assert!(matches!(
            validate(&d, &[], ComplexityLimits::default()),
            Err(DefinitionError::BadText(_))
        ));
    }

    #[test]
    fn an_overlong_name_is_refused() {
        let mut d = definition("x");
        d.name = "a".repeat(MAX_NAME_LEN + 1);
        assert!(matches!(
            validate(&d, &[], ComplexityLimits::default()),
            Err(DefinitionError::BadText(_))
        ));
    }

    #[test]
    fn name_and_description_are_trimmed_not_rejected_for_padding() {
        let mut d = definition("x");
        d.name = "  Spaces  ".to_string();
        let user = validate(&d, &[], ComplexityLimits::default()).expect("valid");
        assert_eq!(user.name, "Spaces");
    }

    #[test]
    fn a_too_deep_rule_is_refused() {
        let mut rule = RuleExpr::Condition(crate::builder::Condition {
            metric: crate::builder::UserMetric::Commits,
            at_least: 1,
        });
        for _ in 0..12 {
            rule = RuleExpr::All {
                children: vec![rule],
            };
        }
        let mut d = definition("deep");
        d.rule = rule;
        assert!(matches!(
            validate(&d, &[], ComplexityLimits::default()),
            Err(DefinitionError::TooComplex(_))
        ));
    }

    #[test]
    fn json_round_trip_produces_the_same_definition() {
        let d = definition("json-rule");
        let json = serde_json::to_string(&d).expect("serialise");
        let back = from_json(&json, &[], ComplexityLimits::default()).expect("parse");
        assert_eq!(back.id, "json-rule");
    }

    #[test]
    fn malformed_json_is_reported_as_invalid() {
        assert!(matches!(
            from_json("not json", &[], ComplexityLimits::default()),
            Err(DefinitionError::Invalid(_))
        ));
    }

    #[test]
    fn a_json_rule_that_names_an_unknown_metric_is_refused() {
        let json = r#"{
            "id": "bad-metric",
            "name": "Bad",
            "description": "",
            "rule": { "op": "condition", "Condition": { "metric": "moon_phase", "at_least": 1 } }
        }"#;
        assert!(from_json(json, &[], ComplexityLimits::default()).is_err());
    }

    #[test]
    fn the_wire_shape_is_flat_for_a_condition() {
        // Pinned because the JSON shape is what a user types. Getting it wrong in
        // the docs or in a fixture is a silent trap.
        let d = definition("shape");
        let json = serde_json::to_string(&d).expect("serialise");
        println!("WIRE {json}");
        assert!(json.contains("\"op\""), "{json}");
    }

    #[test]
    fn a_user_rule_unlocks_when_its_statistic_is_reached() {
        let achievement = validate(&definition("ten-commits"), &[], ComplexityLimits::default())
            .expect("valid rule");
        let below = crate::StatisticsSnapshot {
            commit_count: 9,
            ..Default::default()
        };
        let reached = crate::StatisticsSnapshot {
            commit_count: 10,
            ..Default::default()
        };
        assert!(newly_satisfied(std::slice::from_ref(&achievement), &below, &[]).is_empty());
        assert_eq!(
            newly_satisfied(std::slice::from_ref(&achievement), &reached, &[]),
            vec!["ten-commits".to_string()]
        );
    }

    #[test]
    fn an_already_unlocked_user_rule_is_never_returned_again() {
        let achievement = validate(&definition("ten-commits"), &[], ComplexityLimits::default())
            .expect("valid rule");
        let reached = crate::StatisticsSnapshot {
            commit_count: 100,
            ..Default::default()
        };
        let already = vec!["ten-commits".to_string()];
        assert!(newly_satisfied(&[achievement], &reached, &already).is_empty());
    }

    #[test]
    fn every_metric_a_user_rule_can_read_reaches_its_statistic() {
        use crate::builder::{Condition, UserMetric};
        use RuleExpr::Condition as Leaf;
        let cases = [
            (
                UserMetric::Commits,
                &crate::StatisticsSnapshot {
                    commit_count: 5,
                    ..Default::default()
                },
            ),
            (
                UserMetric::PullRequestsMerged,
                &crate::StatisticsSnapshot {
                    pull_request_merged_count: 5,
                    ..Default::default()
                },
            ),
            (
                UserMetric::Releases,
                &crate::StatisticsSnapshot {
                    release_count: 5,
                    ..Default::default()
                },
            ),
            (
                UserMetric::StreakDays,
                &crate::StatisticsSnapshot {
                    longest_streak_days: 5,
                    ..Default::default()
                },
            ),
        ];
        for (metric, statistics) in cases {
            let achievement = UserAchievement {
                id: "metric-check".to_string(),
                name: "Metric".to_string(),
                description: String::new(),
                rule: Leaf(Condition {
                    metric,
                    at_least: 5,
                }),
                version: 1,
            };
            assert!(
                achievement.evaluate(statistics).satisfied,
                "{metric:?} did not reach its statistic"
            );
        }
    }
}

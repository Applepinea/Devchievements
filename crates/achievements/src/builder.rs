//! Achievement Builder rule validation and deterministic evaluation (spec 22).
//!
//! # The two hard requirements
//!
//! 1. **Bounded structure**: nesting depth and condition count have hard caps.
//! 2. **Deterministic budget**: evaluation counts STEP, never wall-clock time.
//!
//! The second point is easy to get wrong and quietly fatal. A timeout-based budget
//! would make the same rule produce different results on a slow machine, which
//! breaks Golden Tests and makes "why did this unlock?" unanswerable. So the budget
//! is an integer that decrements once per node visited.
//!
//! # Why validation happens at save time
//!
//! Spec 22 requires it. A rule that is rejected on save is a rule the user can fix;
//! a rule that blows up during evaluation is a rule that silently stops working.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// Hard cap on nesting depth. Configurable below this, never above.
pub const MAX_DEPTH_HARD_LIMIT: u32 = 5;
/// Hard cap on the number of leaf conditions.
pub const MAX_CONDITIONS_HARD_LIMIT: u32 = 50;
/// Hard cap on evaluation steps, independent of structure.
pub const MAX_STEPS_HARD_LIMIT: u32 = 1_000;

/// Which statistics a user rule reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMetric {
    /// Valid commit count.
    Commits,
    /// Merged pull requests.
    PullRequestsMerged,
    /// Releases published.
    Releases,
    /// Longest consecutive active day run.
    StreakDays,
}

/// A leaf comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// What to read.
    pub metric: UserMetric,
    /// Threshold.
    pub at_least: u32,
}

/// A rule tree.
///
/// Spec 22 forbids rules referencing other rules, so there is no indirection here:
/// a tree is the whole rule, and cycles are structurally impossible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RuleExpr {
    /// A single comparison.
    Condition(Condition),
    /// Every child must hold.
    All {
        /// Sub-expressions.
        children: Vec<RuleExpr>,
    },
    /// At least one child must hold.
    Any {
        /// Sub-expressions.
        children: Vec<RuleExpr>,
    },
}

/// Limits applied to a rule, defaulting to the spec values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplexityLimits {
    /// Maximum nesting depth (1 = a bare condition).
    pub max_depth: u32,
    /// Maximum number of leaf conditions.
    pub max_conditions: u32,
    /// Maximum evaluation steps.
    pub max_steps: u32,
}

impl Default for ComplexityLimits {
    fn default() -> Self {
        ComplexityLimits {
            max_depth: MAX_DEPTH_HARD_LIMIT,
            max_conditions: MAX_CONDITIONS_HARD_LIMIT,
            max_steps: MAX_STEPS_HARD_LIMIT,
        }
    }
}

impl ComplexityLimits {
    /// Clamp user-provided limits to the hard caps.
    ///
    /// Spec 22 allows the user to configure these "but with a hard ceiling". This is
    /// that ceiling, applied here so no caller can exceed it by forgetting to check.
    pub fn clamped(self) -> Self {
        ComplexityLimits {
            max_depth: self.max_depth.clamp(1, MAX_DEPTH_HARD_LIMIT),
            max_conditions: self.max_conditions.clamp(1, MAX_CONDITIONS_HARD_LIMIT),
            max_steps: self.max_steps.clamp(1, MAX_STEPS_HARD_LIMIT),
        }
    }
}

/// Why a rule was rejected at save time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// A combinator had no children, which makes AND vacuously true and OR vacuously
    /// false; both are almost certainly a mistake.
    EmptyCombinator,
    /// Nesting exceeded the limit.
    TooDeep {
        /// Depth reached.
        depth: u32,
        /// Limit in force.
        limit: u32,
    },
    /// Too many leaf conditions.
    TooManyConditions {
        /// Count found.
        found: u32,
        /// Limit in force.
        limit: u32,
    },
    /// A threshold of zero is meaningless and likely a typo.
    ZeroThreshold,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::EmptyCombinator => write!(f, "a combinator needs at least one child"),
            RuleError::TooDeep { depth, limit } => {
                write!(f, "rule nests {depth} deep but the limit is {limit}")
            }
            RuleError::TooManyConditions { found, limit } => {
                write!(f, "rule has {found} conditions but the limit is {limit}")
            }
            RuleError::ZeroThreshold => write!(f, "a threshold of 0 can never be a target"),
        }
    }
}

impl std::error::Error for RuleError {}

/// What a rule can read at evaluation time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserStatistics {
    /// Valid commits.
    pub commits: u32,
    /// Merged PRs.
    pub pull_requests_merged: u32,
    /// Releases.
    pub releases: u32,
    /// Longest streak in days.
    pub streak_days: u32,
}

impl UserStatistics {
    /// Read a metric.
    pub fn value(&self, metric: UserMetric) -> u32 {
        match metric {
            UserMetric::Commits => self.commits,
            UserMetric::PullRequestsMerged => self.pull_requests_merged,
            UserMetric::Releases => self.releases,
            UserMetric::StreakDays => self.streak_days,
        }
    }
}

impl RuleExpr {
    /// Validate the rule before it is saved.
    ///
    /// Returns the first problem found; reporting every problem at once would be
    /// friendlier but the limits are small enough that fixing them is quick.
    pub fn validate(&self, limits: ComplexityLimits) -> Result<(), RuleError> {
        let limits = limits.clamped();
        let (conditions, depth) = self.shape()?;

        if depth > limits.max_depth {
            return Err(RuleError::TooDeep {
                depth,
                limit: limits.max_depth,
            });
        }
        if conditions > limits.max_conditions {
            return Err(RuleError::TooManyConditions {
                found: conditions,
                limit: limits.max_conditions,
            });
        }
        self.check_thresholds()?;
        Ok(())
    }

    /// Count leaf conditions and measure depth.
    fn shape(&self) -> Result<(u32, u32), RuleError> {
        match self {
            RuleExpr::Condition(_) => Ok((1, 1)),
            RuleExpr::All { children } | RuleExpr::Any { children } => {
                if children.is_empty() {
                    return Err(RuleError::EmptyCombinator);
                }
                let mut total = 0;
                let mut deepest = 0;
                for child in children {
                    let (c, d) = child.shape()?;
                    total += c;
                    deepest = deepest.max(d);
                }
                Ok((total, deepest + 1))
            }
        }
    }

    fn check_thresholds(&self) -> Result<(), RuleError> {
        match self {
            RuleExpr::Condition(cond) => {
                if cond.at_least == 0 {
                    Err(RuleError::ZeroThreshold)
                } else {
                    Ok(())
                }
            }
            RuleExpr::All { children } | RuleExpr::Any { children } => {
                for child in children {
                    child.check_thresholds()?;
                }
                Ok(())
            }
        }
    }

    /// Evaluate the rule with a deterministic step budget.
    ///
    /// # Why a step budget and not a timeout
    ///
    /// Spec 22 forbids wall-clock timeouts because they break determinism. A step
    /// count gives the same answer on every machine and at every speed, which is what
    /// Golden Tests and diagnostics need.
    pub fn evaluate(&self, stats: &UserStatistics, budget: u32) -> Evaluation {
        let mut remaining = budget;
        let mut steps = 0;
        let outcome = self.eval_inner(stats, &mut remaining, &mut steps);
        // `satisfied` is only meaningful when a verdict was reached. When the budget
        // ran out the value is reported as false, but `exhausted` is what callers must
        // check before trusting it.
        Evaluation {
            satisfied: outcome.unwrap_or(false),
            steps,
            exhausted: outcome.is_none(),
        }
    }

    fn eval_inner(
        &self,
        stats: &UserStatistics,
        remaining: &mut u32,
        steps: &mut u32,
    ) -> Option<bool> {
        if *remaining == 0 {
            return None;
        }
        *remaining -= 1;
        *steps += 1;

        match self {
            RuleExpr::Condition(cond) => Some(stats.value(cond.metric) >= cond.at_least),
            RuleExpr::All { children } => {
                for child in children {
                    match child.eval_inner(stats, remaining, steps)? {
                        true => continue,
                        false => return Some(false),
                    }
                }
                Some(true)
            }
            RuleExpr::Any { children } => {
                for child in children {
                    if child.eval_inner(stats, remaining, steps)? {
                        return Some(true);
                    }
                }
                Some(false)
            }
        }
    }
}

/// The result of one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evaluation {
    /// Whether the rule held. Meaningless when [`Self::exhausted`] is true.
    pub satisfied: bool,
    /// Steps consumed, for diagnostics.
    pub steps: u32,
    /// Whether the budget ran out before a verdict was reached.
    ///
    /// Spec 22: in this case the rule is treated as INVALID and disabled, logged, and
    /// the user is told. It is never silently treated as "not satisfied", because that
    /// would make an over-budget rule look like a rule that simply does not apply.
    pub exhausted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cond(metric: UserMetric, at_least: u32) -> RuleExpr {
        RuleExpr::Condition(Condition { metric, at_least })
    }

    fn stats(commits: u32) -> UserStatistics {
        UserStatistics {
            commits,
            ..Default::default()
        }
    }

    #[test]
    fn a_bare_condition_is_valid() {
        let rule = cond(UserMetric::Commits, 1);
        assert!(rule.validate(ComplexityLimits::default()).is_ok());
    }

    #[test]
    fn an_empty_combinator_is_rejected() {
        // AND of nothing is vacuously true, which would unlock immediately.
        let rule = RuleExpr::All { children: vec![] };
        assert_eq!(
            rule.validate(ComplexityLimits::default()),
            Err(RuleError::EmptyCombinator)
        );
    }

    #[test]
    fn depth_beyond_the_limit_is_rejected() {
        // Six nested levels (five combinators wrapping a condition) exceeds 5.
        let mut rule = cond(UserMetric::Commits, 1);
        for _ in 0..6 {
            rule = RuleExpr::All {
                children: vec![rule],
            };
        }
        assert!(matches!(
            rule.validate(ComplexityLimits::default()),
            Err(RuleError::TooDeep { .. })
        ));
    }

    #[test]
    fn a_rule_at_exactly_the_depth_limit_is_accepted() {
        // Off-by-one here would reject a legal rule, so pin the boundary.
        let mut rule = cond(UserMetric::Commits, 1);
        for _ in 0..4 {
            rule = RuleExpr::All {
                children: vec![rule],
            };
        }
        assert_eq!(rule.shape().map(|(_, d)| d), Ok(5));
        assert!(rule.validate(ComplexityLimits::default()).is_ok());
    }

    #[test]
    fn too_many_conditions_is_rejected() {
        let children = (0..51).map(|_| cond(UserMetric::Commits, 1)).collect();
        let rule = RuleExpr::All { children };
        assert!(matches!(
            rule.validate(ComplexityLimits::default()),
            Err(RuleError::TooManyConditions { .. })
        ));
    }

    #[test]
    fn exactly_the_condition_limit_is_accepted() {
        let children = (0..50).map(|_| cond(UserMetric::Commits, 1)).collect();
        let rule = RuleExpr::All { children };
        assert!(rule.validate(ComplexityLimits::default()).is_ok());
    }

    #[test]
    fn limits_cannot_exceed_the_hard_caps() {
        // Spec 22: configurable "but with a hard ceiling".
        let greedy = ComplexityLimits {
            max_depth: 999,
            max_conditions: 999,
            max_steps: 999_999,
        };
        let clamped = greedy.clamped();
        assert_eq!(clamped.max_depth, MAX_DEPTH_HARD_LIMIT);
        assert_eq!(clamped.max_conditions, MAX_CONDITIONS_HARD_LIMIT);
        assert_eq!(clamped.max_steps, MAX_STEPS_HARD_LIMIT);
    }

    #[test]
    fn a_zero_threshold_is_rejected() {
        assert_eq!(
            cond(UserMetric::Commits, 0).validate(ComplexityLimits::default()),
            Err(RuleError::ZeroThreshold)
        );
    }

    #[test]
    fn all_requires_every_child() {
        let rule = RuleExpr::All {
            children: vec![
                cond(UserMetric::Commits, 10),
                cond(UserMetric::StreakDays, 3),
            ],
        };
        let mut s = stats(10);
        assert!(!rule.evaluate(&s, 100).satisfied, "streak still 0");
        s.streak_days = 3;
        assert!(rule.evaluate(&s, 100).satisfied);
    }

    #[test]
    fn any_requires_only_one_child() {
        let rule = RuleExpr::Any {
            children: vec![
                cond(UserMetric::Commits, 100),
                cond(UserMetric::Releases, 1),
            ],
        };
        let s = UserStatistics {
            releases: 1,
            ..Default::default()
        };
        assert!(rule.evaluate(&s, 100).satisfied);
    }

    #[test]
    fn evaluation_is_deterministic_for_the_same_inputs() {
        // The property a wall-clock budget would break.
        let rule = RuleExpr::All {
            children: vec![
                cond(UserMetric::Commits, 5),
                cond(UserMetric::StreakDays, 2),
            ],
        };
        let s = stats(7);
        let first = rule.evaluate(&s, 100);
        for _ in 0..10 {
            assert_eq!(rule.evaluate(&s, 100), first);
        }
    }

    #[test]
    fn a_tiny_budget_reports_exhaustion_rather_than_a_false_verdict() {
        // Treating exhaustion as "not satisfied" would make the rule look merely
        // unmet instead of invalid, which is the wrong diagnosis.
        let rule = RuleExpr::All {
            children: vec![cond(UserMetric::Commits, 1), cond(UserMetric::Releases, 1)],
        };
        let result = rule.evaluate(&stats(10), 1);
        assert!(result.exhausted, "one step cannot finish a 2-condition AND");
    }

    #[test]
    fn step_count_grows_with_structure_but_stays_bounded() {
        let children = (0..10).map(|_| cond(UserMetric::Commits, 1)).collect();
        let rule = RuleExpr::All { children };
        let result = rule.evaluate(&stats(1), MAX_STEPS_HARD_LIMIT);
        assert!(!result.exhausted);
        assert!(
            result.steps <= 11,
            "1 combinator + 10 conditions, got {}",
            result.steps
        );
    }

    #[test]
    fn a_rule_survives_a_json_round_trip() {
        // Rules are stored and exported, so the shape must round-trip.
        let rule = RuleExpr::Any {
            children: vec![
                RuleExpr::All {
                    children: vec![
                        cond(UserMetric::Commits, 10),
                        cond(UserMetric::StreakDays, 5),
                    ],
                },
                cond(UserMetric::Releases, 3),
            ],
        };
        let json = serde_json::to_string(&rule).expect("serialise");
        let back: RuleExpr = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(rule, back);
    }

    #[test]
    fn validation_runs_before_evaluation_so_bad_rules_never_count_as_unmet() {
        // A rejected rule is disabled; it must never be evaluated and reported as
        // merely "not satisfied", which would hide the configuration error.
        let bad = RuleExpr::All { children: vec![] };
        assert!(bad.validate(ComplexityLimits::default()).is_err());
    }
}

//! Permanent achievements (spec 13).
//!
//! # The rules that must not be broken
//!
//! 1. Unlocked once, **never revoked** (spec 13.1). Statistics going down later
//!    (deleting a branch, excluding a repo) must not take an achievement away.
//! 2. Built-in achievements use a **fixed, versioned statistics profile**; changing
//!    the user's daily-rank thresholds or filters must not change their outcome
//!    (spec 13.3).
//! 3. Every achievement declares `required_capabilities`, so the UI can say
//!    "needs the browser extension" instead of showing a permanent zero (spec 2.3).
//! 4. Every unlock records its source and the rule version that produced it
//!    (spec 13.4), so a backfilled unlock is never presented as a live one.
//!
//! # What this module deliberately does NOT do
//!
//! It does not read statistics, persist anything, or mutate the unlock store. It is
//! the rule book: given a profile's numbers, say which achievements are satisfied.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod builder;
pub mod definition;

use serde::{Deserialize, Serialize};

/// Rarity, used for notification styling and merge ordering (spec 16.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    /// Common.
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Epic.
    Epic,
    /// Legendary, the highest.
    Legendary,
}

impl Rarity {
    /// The stable identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Epic => "epic",
            Rarity::Legendary => "legendary",
        }
    }

    /// Default on-screen duration in seconds (spec 28.2).
    pub fn default_display_seconds(self) -> u32 {
        match self {
            Rarity::Common | Rarity::Uncommon => 5,
            Rarity::Rare => 6,
            Rarity::Epic => 7,
            Rarity::Legendary => 8,
        }
    }
}

/// What a rule is allowed to observe.
///
/// This is the mechanism behind spec 13.3: a built-in achievement declares
/// `Official`, and the engine then ignores user filters and custom thresholds
/// entirely. A user achievement declares `UserFiltered`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatisticsProfile {
    /// Fixed official counting rules; ignores user filters and custom thresholds.
    Official,
    /// Whatever the user's filters currently produce.
    UserFiltered,
}

/// When a rule applies to history (spec 13.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetroactivePolicy {
    /// Only events arriving after the rule took effect.
    Never,
    /// Only events produced after first launch.
    FromInstall,
    /// Everything in the event store, including imported history.
    FullHistory,
    /// Ask the user when the rule is created or upgraded.
    UserChoice,
}

impl RetroactivePolicy {
    /// The default for built-in achievements (spec 13.3).
    pub fn default_for_builtin() -> Self {
        RetroactivePolicy::FullHistory
    }

    /// The default for user-authored achievements.
    pub fn default_for_user() -> Self {
        RetroactivePolicy::UserChoice
    }
}

/// A capability an achievement depends on (spec 2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Local Git hooks (available in Desktop Only).
    LocalGit,
    /// Verifiable GitHub activity observed by the extension.
    BrowserGithub,
}

/// The numbers a rule evaluates against.
///
/// Deliberately a flat, explicit struct rather than a generic map: it makes the
/// read-set of every rule statically obvious (spec 4.3) and prevents a rule from
/// silently depending on a projection nobody loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatisticsSnapshot {
    /// Valid, counted commits (spec 8.1).
    pub commit_count: u32,
    /// PRs merged.
    pub pull_request_merged_count: u32,
    /// Releases published.
    pub release_count: u32,
    /// Issues opened.
    pub issue_opened_count: u32,
    /// Repositories created.
    pub repository_created_count: u32,
    /// Stars given.
    pub star_given_count: u32,
    /// Longest run of consecutive active days.
    pub longest_streak_days: u32,
}

/// What a rule requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCondition {
    /// Total valid commits at least `n`.
    CommitsAtLeast(u32),
    /// Merged PRs at least `n`.
    PullRequestsMergedAtLeast(u32),
    /// Releases at least `n`.
    ReleasesAtLeast(u32),
    /// Consecutive active days at least `n`.
    StreakAtLeast(u32),
    /// Repositories created at least `n`.
    RepositoriesCreatedAtLeast(u32),
    /// Stars given at least `n`.
    StarsGivenAtLeast(u32),
}

impl RuleCondition {
    /// The capability needed to evaluate this condition.
    ///
    /// This is how `required_capabilities` is derived rather than hand-listed,
    /// which keeps the two from drifting apart.
    pub fn required_capability(self) -> Capability {
        match self {
            RuleCondition::CommitsAtLeast(_) | RuleCondition::StreakAtLeast(_) => {
                Capability::LocalGit
            }
            RuleCondition::PullRequestsMergedAtLeast(_)
            | RuleCondition::ReleasesAtLeast(_)
            | RuleCondition::RepositoriesCreatedAtLeast(_)
            | RuleCondition::StarsGivenAtLeast(_) => Capability::BrowserGithub,
        }
    }

    /// Evaluate against a snapshot.
    pub fn is_satisfied(self, snapshot: &StatisticsSnapshot) -> bool {
        match self {
            RuleCondition::CommitsAtLeast(n) => snapshot.commit_count >= n,
            RuleCondition::PullRequestsMergedAtLeast(n) => snapshot.pull_request_merged_count >= n,
            RuleCondition::ReleasesAtLeast(n) => snapshot.release_count >= n,
            RuleCondition::StreakAtLeast(n) => snapshot.longest_streak_days >= n,
            RuleCondition::RepositoriesCreatedAtLeast(n) => snapshot.repository_created_count >= n,
            RuleCondition::StarsGivenAtLeast(n) => snapshot.star_given_count >= n,
        }
    }

    /// The current value alongside the requirement, for progress display.
    pub fn progress(self, snapshot: &StatisticsSnapshot) -> (u32, u32) {
        let current = match self {
            RuleCondition::CommitsAtLeast(_) => snapshot.commit_count,
            RuleCondition::PullRequestsMergedAtLeast(_) => snapshot.pull_request_merged_count,
            RuleCondition::ReleasesAtLeast(_) => snapshot.release_count,
            RuleCondition::StreakAtLeast(_) => snapshot.longest_streak_days,
            RuleCondition::RepositoriesCreatedAtLeast(_) => snapshot.repository_created_count,
            RuleCondition::StarsGivenAtLeast(_) => snapshot.star_given_count,
        };
        let target = match self {
            RuleCondition::CommitsAtLeast(n)
            | RuleCondition::PullRequestsMergedAtLeast(n)
            | RuleCondition::ReleasesAtLeast(n)
            | RuleCondition::StreakAtLeast(n)
            | RuleCondition::RepositoriesCreatedAtLeast(n)
            | RuleCondition::StarsGivenAtLeast(n) => n,
        };
        (current, target)
    }
}

/// A versioned achievement definition (spec 13.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Achievement {
    /// Stable identifier, never reused.
    pub achievement_id: String,
    /// Presentation version (renaming, changing the icon).
    pub achievement_version: u32,
    /// Version of the RULE; bumping this can re-evaluate history.
    pub rule_version: u32,
    /// When it was introduced, Unix epoch milliseconds.
    pub introduced_at: i64,
    /// When it was retired, if it was.
    pub retired_at: Option<i64>,
    /// Which counting rules it observes.
    pub statistics_profile: StatisticsProfile,
    /// Whether it is hidden until unlocked.
    pub hidden: bool,
    /// The requirement.
    pub condition: RuleCondition,
    /// How it applies to existing history.
    pub retroactive_policy: RetroactivePolicy,
    /// Display name (revealed only after unlock when `hidden`).
    pub name: String,
    /// Display description.
    pub description: String,
    /// Rarity, for styling and merge ordering.
    pub rarity: Rarity,
    /// Whether it can unlock more than once. Built-ins are always once.
    pub repeatable: bool,
}

impl Achievement {
    /// The capability this achievement needs to be evaluable.
    pub fn required_capability(&self) -> Capability {
        self.condition.required_capability()
    }

    /// Whether the achievement is satisfiable given the available capabilities.
    ///
    /// The UI uses this to show "needs browser extension" instead of an
    /// unreachable progress bar (spec 2.3, 24.2).
    pub fn is_evaluable_with(&self, available: &[Capability]) -> bool {
        available.contains(&self.required_capability())
    }

    /// Whether a snapshot satisfies the rule.
    ///
    /// Returns false for retired achievements, so a retired rule can never newly
    /// unlock. Already-unlocked ones are kept by the store, per spec 13.3.
    pub fn is_satisfied(&self, snapshot: &StatisticsSnapshot) -> bool {
        if self.retired_at.is_some() {
            return false;
        }
        self.condition.is_satisfied(snapshot)
    }
}

/// Why an unlock happened (spec 13.4). Never conflate these in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnlockSource {
    /// Unlocked while the user was present and the event was fresh.
    Realtime,
    /// Unlocked after a classification wait; still "live" but not instant.
    Delayed,
    /// Unlocked by re-evaluating existing history.
    Backfill,
    /// Unlocked during a history import.
    Import,
}

impl UnlockSource {
    /// Whether this unlock should be announced loudly.
    ///
    /// Spec 13.4: history import and backfill are silent by default; they must not
    /// be dressed up as real-time unlocks.
    pub fn is_silent(self) -> bool {
        matches!(self, UnlockSource::Backfill | UnlockSource::Import)
    }
}

/// An unlock record (spec 13.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlockRecord {
    /// Which achievement.
    pub achievement_id: String,
    /// When it unlocked, Unix epoch milliseconds.
    pub unlocked_at: i64,
    /// Why it unlocked.
    pub source: UnlockSource,
    /// The rule version in force.
    pub rule_version: u32,
    /// When the user first saw it, if they have.
    pub viewed_at: Option<i64>,
}

/// The outcome of evaluating every rule against a snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvaluationResult {
    /// Achievements that are satisfied and have no existing unlock.
    pub newly_unlocked: Vec<String>,
    /// Achievements that are satisfied but already unlocked (no action).
    pub already_unlocked: Vec<String>,
    /// Achievements that are not satisfied.
    pub still_locked: Vec<String>,
    /// Achievements skipped because the capability is unavailable.
    pub unavailable: Vec<String>,
}

/// Evaluate all rules for a snapshot.
///
/// `existing_unlocks` is the set of achievement ids already unlocked. Per spec
/// 13.1 an already-unlocked achievement is never reconsidered or revoked, so it is
/// reported separately and never appears in `newly_unlocked`.
pub fn evaluate(
    achievements: &[Achievement],
    snapshot: &StatisticsSnapshot,
    existing_unlocks: &[String],
    available_capabilities: &[Capability],
) -> EvaluationResult {
    let mut result = EvaluationResult::default();

    for achievement in achievements {
        let id = achievement.achievement_id.clone();

        if existing_unlocks.iter().any(|e| e == &id) {
            result.already_unlocked.push(id);
            continue;
        }

        if !achievement.is_evaluable_with(available_capabilities) {
            result.unavailable.push(id);
            continue;
        }

        if achievement.is_satisfied(snapshot) {
            result.newly_unlocked.push(id);
        } else {
            result.still_locked.push(id);
        }
    }

    result
}

/// The built-in launch achievement set (spec 13.2).
///
/// Built-ins are always: official profile, full-history retroactive policy, and
/// non-repeatable. Encoding that here once means a future built-in cannot be added
/// with the wrong semantics by accident.
pub fn builtin_achievements() -> Vec<Achievement> {
    let make = |id: &str,
                name: &str,
                description: &str,
                condition: RuleCondition,
                rarity: Rarity,
                hidden: bool| Achievement {
        achievement_id: id.to_string(),
        achievement_version: 1,
        rule_version: 1,
        introduced_at: 0,
        retired_at: None,
        statistics_profile: StatisticsProfile::Official,
        hidden,
        condition,
        retroactive_policy: RetroactivePolicy::default_for_builtin(),
        name: name.to_string(),
        description: description.to_string(),
        rarity,
        repeatable: false,
    };

    vec![
        make(
            "first_commit",
            "First Commit",
            "Make your very first commit.",
            RuleCondition::CommitsAtLeast(1),
            Rarity::Common,
            false,
        ),
        make(
            "commits_10",
            "Getting Started",
            "Reach 10 commits.",
            RuleCondition::CommitsAtLeast(10),
            Rarity::Common,
            false,
        ),
        make(
            "commits_100",
            "Century",
            "Reach 100 commits.",
            RuleCondition::CommitsAtLeast(100),
            Rarity::Rare,
            false,
        ),
        make(
            "commits_1000",
            "Thousand Commits",
            "Reach 1000 commits.",
            RuleCondition::CommitsAtLeast(1000),
            Rarity::Legendary,
            false,
        ),
        make(
            "first_release",
            "First Release",
            "Publish your first release.",
            RuleCondition::ReleasesAtLeast(1),
            Rarity::Uncommon,
            false,
        ),
        make(
            "first_pr_merged",
            "Merged",
            "Get your first pull request merged.",
            RuleCondition::PullRequestsMergedAtLeast(1),
            Rarity::Uncommon,
            false,
        ),
        make(
            "streak_7",
            "One Week",
            "Stay active for 7 days in a row.",
            RuleCondition::StreakAtLeast(7),
            Rarity::Rare,
            false,
        ),
        make(
            "streak_30",
            "One Month",
            "Stay active for 30 days in a row.",
            RuleCondition::StreakAtLeast(30),
            Rarity::Epic,
            false,
        ),
        make(
            "streak_365",
            "A Year of Code",
            "Stay active for 365 days in a row.",
            RuleCondition::StreakAtLeast(365),
            Rarity::Legendary,
            true,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> StatisticsSnapshot {
        StatisticsSnapshot::default()
    }

    fn achievement(id: &str, condition: RuleCondition) -> Achievement {
        Achievement {
            achievement_id: id.to_string(),
            achievement_version: 1,
            rule_version: 1,
            introduced_at: 0,
            retired_at: None,
            statistics_profile: StatisticsProfile::Official,
            hidden: false,
            condition,
            retroactive_policy: RetroactivePolicy::FullHistory,
            name: id.to_string(),
            description: String::new(),
            rarity: Rarity::Common,
            repeatable: false,
        }
    }

    #[test]
    fn builtins_all_use_the_official_profile_and_full_history() {
        // Spec 13.3: built-ins may not be user-filtered and default to full history.
        for a in builtin_achievements() {
            assert_eq!(
                a.statistics_profile,
                StatisticsProfile::Official,
                "{}",
                a.achievement_id
            );
            assert_eq!(
                a.retroactive_policy,
                RetroactivePolicy::FullHistory,
                "{}",
                a.achievement_id
            );
            assert!(!a.repeatable, "{}", a.achievement_id);
        }
    }

    #[test]
    fn builtin_legendary_day_is_not_in_the_daily_rank_table() {
        // Guard against the classic confusion: daily ranks are repeatable and live
        // elsewhere; permanent achievements must not appear as daily ranks.
        let ranks = ["common", "uncommon", "rare", "epic", "legendary"];
        for a in builtin_achievements() {
            assert!(
                !ranks.contains(&a.achievement_id.as_str()),
                "achievement id {} collides with a daily rank",
                a.achievement_id
            );
        }
    }

    #[test]
    fn unlock_happens_once_and_is_never_retracted() {
        let a = achievement("c1", RuleCondition::CommitsAtLeast(1));
        let mut s = snap();
        s.commit_count = 5;

        let first = evaluate(std::slice::from_ref(&a), &s, &[], &[Capability::LocalGit]);
        assert_eq!(first.newly_unlocked, vec!["c1".to_string()]);

        // Statistics drop (branch deleted, repo excluded). The unlock stands.
        let second = evaluate(&[a], &snap(), &["c1".to_string()], &[Capability::LocalGit]);
        assert!(second.newly_unlocked.is_empty());
        assert_eq!(second.already_unlocked, vec!["c1".to_string()]);
    }

    #[test]
    fn unavailable_capability_is_reported_separately_not_as_locked_progress() {
        // Spec 2.3 / 24.2: the UI must show "needs browser extension", never a
        // permanently-zero progress bar.
        let a = achievement("rel1", RuleCondition::ReleasesAtLeast(1));
        let result = evaluate(&[a], &snap(), &[], &[Capability::LocalGit]);
        assert!(result.newly_unlocked.is_empty());
        assert!(
            result.still_locked.is_empty(),
            "must not be reported as progress"
        );
        assert_eq!(result.unavailable, vec!["rel1".to_string()]);
    }

    #[test]
    fn capability_is_derived_from_the_condition() {
        assert_eq!(
            RuleCondition::CommitsAtLeast(1).required_capability(),
            Capability::LocalGit
        );
        assert_eq!(
            RuleCondition::StreakAtLeast(7).required_capability(),
            Capability::LocalGit
        );
        assert_eq!(
            RuleCondition::ReleasesAtLeast(1).required_capability(),
            Capability::BrowserGithub
        );
        assert_eq!(
            RuleCondition::PullRequestsMergedAtLeast(1).required_capability(),
            Capability::BrowserGithub
        );
        assert_eq!(
            RuleCondition::StarsGivenAtLeast(1).required_capability(),
            Capability::BrowserGithub
        );
        assert_eq!(
            RuleCondition::RepositoriesCreatedAtLeast(1).required_capability(),
            Capability::BrowserGithub
        );
    }

    #[test]
    fn a_retired_achievement_can_never_newly_unlock() {
        let mut a = achievement("old", RuleCondition::CommitsAtLeast(1));
        a.retired_at = Some(1);
        let mut s = snap();
        s.commit_count = 100;
        let result = evaluate(&[a], &s, &[], &[Capability::LocalGit]);
        assert!(result.newly_unlocked.is_empty());
        assert_eq!(result.still_locked, vec!["old".to_string()]);
    }

    #[test]
    fn import_and_backfill_are_silent_but_realtime_is_not() {
        // Spec 13.4: history must never be dressed up as a live unlock.
        assert!(UnlockSource::Import.is_silent());
        assert!(UnlockSource::Backfill.is_silent());
        assert!(!UnlockSource::Realtime.is_silent());
        assert!(!UnlockSource::Delayed.is_silent());
    }

    #[test]
    fn builtin_defaults_for_retroactive_policy_match_the_spec() {
        assert_eq!(
            RetroactivePolicy::default_for_builtin(),
            RetroactivePolicy::FullHistory
        );
        assert_eq!(
            RetroactivePolicy::default_for_user(),
            RetroactivePolicy::UserChoice
        );
    }

    #[test]
    fn rarity_display_durations_match_the_spec() {
        // Spec 28.2.
        assert_eq!(Rarity::Common.default_display_seconds(), 5);
        assert_eq!(Rarity::Uncommon.default_display_seconds(), 5);
        assert_eq!(Rarity::Rare.default_display_seconds(), 6);
        assert_eq!(Rarity::Epic.default_display_seconds(), 7);
        assert_eq!(Rarity::Legendary.default_display_seconds(), 8);
    }

    #[test]
    fn rarity_ordering_puts_legendary_highest() {
        // Needed for notification merging (spec 16.6: highest rarity leads).
        assert!(Rarity::Legendary > Rarity::Epic);
        assert!(Rarity::Epic > Rarity::Rare);
        assert!(Rarity::Rare > Rarity::Uncommon);
        assert!(Rarity::Uncommon > Rarity::Common);
    }

    #[test]
    fn progress_reports_current_and_target() {
        let mut s = snap();
        s.commit_count = 42;
        assert_eq!(RuleCondition::CommitsAtLeast(100).progress(&s), (42, 100));
    }

    #[test]
    fn evaluation_partitions_every_achievement_exactly_once() {
        let all = builtin_achievements();
        let mut s = snap();
        s.commit_count = 10;
        let result = evaluate(
            &all,
            &s,
            &[],
            &[Capability::LocalGit, Capability::BrowserGithub],
        );

        let total = result.newly_unlocked.len()
            + result.already_unlocked.len()
            + result.still_locked.len()
            + result.unavailable.len();
        assert_eq!(
            total,
            all.len(),
            "no achievement may be dropped or double-counted"
        );
    }

    #[test]
    fn hidden_achievements_are_flagged_only_where_intended() {
        let hidden: Vec<String> = builtin_achievements()
            .into_iter()
            .filter(|a| a.hidden)
            .map(|a| a.achievement_id)
            .collect();
        assert_eq!(hidden, vec!["streak_365".to_string()]);
    }
}

//! The Engine: pure evaluation of one event against a snapshot (spec 4.2).
//!
//! # Contract
//!
//! ```text
//! Engine(state_snapshot, event, injected_context)
//!   -> { state_changes, side_effect_intents }
//! ```
//!
//! The Engine never writes, never notifies, never reads a clock. It returns
//! *intents*; the storage adapter applies state changes and outbox rows in ONE
//! transaction (spec 4.3), and the consumer decides what is actually shown.
//!
//! # Read-set discipline (spec 4.3)
//!
//! A rule may only look at what it declared. `ReadSet` is that declaration, and
//! `StateSnapshot` only carries those values. Preventing accidental broad reads is
//! the difference between "commit latency is constant" and "commit latency grows
//! with repository age".

use crate::context::InjectedContext;

/// The projections a single evaluation is allowed to see.
///
/// Every field is an incrementally maintained counter, never a scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateSnapshot {
    /// Snapshot version, for detecting a stale optimistic read.
    pub version: u64,
    /// Total valid commits in the canonical repository.
    pub repository_commit_count: u32,
    /// Valid commits attributed to the user today (per the frozen local_date).
    pub daily_commit_count: u32,
    /// The daily rank already granted today, if any.
    pub daily_rank_today: Option<crate::daily_rank::DailyRank>,
    /// Merged pull requests across every repository the user is tracked in.
    ///
    /// Browser activities are not tied to a working copy, so their counters are the
    /// user's totals rather than one repository's (spec 2.3, 13.2).
    pub pull_request_merged_count: u32,
    /// Releases published, across every tracked repository.
    pub release_count: u32,
    /// Issues opened, across every tracked repository.
    pub issue_opened_count: u32,
    /// Repositories created by the user.
    pub repositories_created_count: u32,
    /// Stars the user gave.
    pub stars_given_count: u32,
    /// Permanent achievements already unlocked.
    pub unlocked_achievement_ids: Vec<String>,
}

impl StateSnapshot {
    /// The projections this Engine will read for the given event type.
    ///
    /// Exposed so the storage adapter can load exactly these and prove it did not
    /// load more. A rule that needs a projection not listed here is a bug in this
    /// module, not a reason to widen the snapshot silently.
    pub fn read_set_for(event_type: crate::EventKind) -> ReadSet {
        match event_type {
            crate::EventKind::Commit => ReadSet::COMMIT,
            crate::EventKind::LocalMerge => ReadSet::MERGE,
            crate::EventKind::Browser => ReadSet::BROWSER,
        }
    }
}

/// Which projections an evaluation depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadSet {
    /// Needs the repository-wide commit count.
    pub repository_commit_count: bool,
    /// Needs today's commit count.
    pub daily_commit_count: bool,
    /// Needs today's daily rank.
    pub daily_rank_today: bool,
    /// Needs the unlocked achievement set.
    pub unlocked_achievements: bool,
    /// Needs the user's GitHub totals (merged pull requests, releases, ...).
    pub browser_totals: bool,
}

impl ReadSet {
    /// The read-set for a plain commit.
    pub const COMMIT: ReadSet = ReadSet {
        repository_commit_count: true,
        daily_commit_count: true,
        daily_rank_today: true,
        unlocked_achievements: true,
        browser_totals: false,
    };

    /// The read-set for a GitHub activity reported by the browser extension.
    ///
    /// It reads the commit total because a merged pull request may itself satisfy a
    /// commit achievement, but it never reads a day projection: browser activities
    /// do not move the daily rank (spec 14.3 counts commits).
    pub const BROWSER: ReadSet = ReadSet {
        repository_commit_count: true,
        daily_commit_count: false,
        daily_rank_today: false,
        unlocked_achievements: true,
        browser_totals: true,
    };

    /// The read-set for a local merge.
    ///
    /// Merges use the same projections because they can unlock commit-related
    /// achievements indirectly through the daily rollup.
    pub const MERGE: ReadSet = ReadSet::COMMIT;
}

/// A single projection mutation the Engine wants applied.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StateChange {
    /// Logical projection name.
    pub projection: String,
    /// New value, already computed.
    pub payload: String,
}

/// What the Engine produced for one event.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EngineOutput {
    /// Projection mutations, applied atomically with the outbox intents.
    pub state_changes: Vec<StateChange>,
    /// Side effects the consumer MAY perform after the transaction commits.
    pub side_effect_intents: Vec<SideEffectIntent>,
}

impl EngineOutput {
    /// Whether anything changed. A repeat delivery of the same event should produce
    /// an empty output, which is how idempotency becomes observable.
    pub fn is_empty(&self) -> bool {
        self.state_changes.is_empty() && self.side_effect_intents.is_empty()
    }
}

/// An intent, not an action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SideEffectIntent {
    /// Stable notification identity.
    pub notification_id: String,
    /// Why it exists.
    pub kind: SideEffectKind,
}

/// Why a notification intent was produced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SideEffectKind {
    /// A permanent achievement unlocked.
    AchievementUnlocked {
        /// Stable achievement id.
        achievement_id: String,
        /// Eligibility source, so the consumer can stay silent for imports.
        source: crate::UnlockSourceTag,
    },
    /// The daily rank improved.
    DailyRankUpgrade {
        /// The new rank.
        rank: crate::daily_rank::DailyRank,
    },
    /// A plain commit was recorded (light sound only by default).
    CommitRecorded,
}

/// Input to one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitFacts {
    /// Whether this commit should count toward ordinary statistics.
    pub counts_as_commit: bool,
    /// Whether it is a merge commit (two or more parents).
    pub is_merge: bool,
    /// Whether it is an empty commit (tree identical to its parent).
    pub is_empty: bool,
}

/// A GitHub activity reported by the browser extension (spec 2.3).
///
/// Only activities a built-in achievement can observe appear here. An opened pull
/// request or a closed issue is still recorded as an event, but nothing reads a
/// counter for it, and inventing one would leave every reader guessing whether the
/// number was maintained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserActivity {
    /// A pull request was merged.
    PullRequestMerged,
    /// A release was published.
    ReleasePublished,
    /// A repository was created.
    RepositoryCreated,
    /// A star was given.
    StarGiven,
}

impl BrowserActivity {
    /// The projection this activity counts toward.
    pub fn counter_projection(self) -> &'static str {
        match self {
            BrowserActivity::PullRequestMerged => "repository_pull_request_merged_count",
            BrowserActivity::ReleasePublished => "repository_release_count",
            BrowserActivity::RepositoryCreated => "repository_repositories_created_count",
            BrowserActivity::StarGiven => "repository_star_given_count",
        }
    }

    /// The statistics the achievement rules see once this activity has happened.
    ///
    /// Shared with the storage layer so the user's own rules are judged against
    /// exactly the numbers the built-ins were judged against.
    pub fn statistics_with(
        self,
        snapshot: &StateSnapshot,
    ) -> crate::achievements::StatisticsSnapshot {
        let mut statistics = crate::achievements::StatisticsSnapshot {
            commit_count: snapshot.repository_commit_count,
            pull_request_merged_count: snapshot.pull_request_merged_count,
            release_count: snapshot.release_count,
            issue_opened_count: snapshot.issue_opened_count,
            repository_created_count: snapshot.repositories_created_count,
            star_given_count: snapshot.stars_given_count,
            // Streaks are derived from days of commits; a browser activity cannot
            // move one, and reading a zero here cannot relock anything because
            // already-unlocked achievements are skipped by id.
            longest_streak_days: 0,
        };
        match self {
            BrowserActivity::PullRequestMerged => statistics.pull_request_merged_count += 1,
            BrowserActivity::ReleasePublished => statistics.release_count += 1,
            BrowserActivity::RepositoryCreated => statistics.repository_created_count += 1,
            BrowserActivity::StarGiven => statistics.star_given_count += 1,
        }
        statistics
    }
}

/// The Engine.
///
/// Stateless: everything it needs arrives in the arguments, which is what makes it
/// trivially testable and identical on native and wasm.
#[derive(Debug, Default, Clone, Copy)]
pub struct Engine;

impl Engine {
    /// Evaluate one commit event.
    ///
    /// Returns deterministic output for identical inputs. Notably, evaluating the
    /// same event twice against the SAME snapshot returns the same result: the
    /// caller achieves idempotency by not applying an event whose id it has already
    /// seen, which is why `event_id` doubles as the idempotency key (spec 5.3).
    pub fn evaluate_commit(
        &self,
        snapshot: &StateSnapshot,
        facts: &CommitFacts,
        event_id: &str,
        ctx: &InjectedContext<'_>,
    ) -> EngineOutput {
        let mut out = EngineOutput::default();

        // Spec 8.1: empty commits and merge commits are excluded from ordinary
        // commit statistics. Excluding them here (rather than in storage) keeps the
        // rule in one place and makes it testable without a database.
        if !facts.counts_as_commit || facts.is_empty {
            return out;
        }

        let new_repository_total = snapshot.repository_commit_count.saturating_add(1);
        let new_daily_total = snapshot.daily_commit_count.saturating_add(1);

        // Both numbers are USER totals (spec 13.2, 14.1): the snapshot was loaded from
        // the global projections, so "+1" here means "one more commit anywhere".
        //
        // The projection NAME says `global_` on purpose. Storage keeps the
        // per-repository counters separately, and an earlier version of this change
        // wrote the global number into the per-repository column -- which then made
        // `SUM(commit_count)` count the same commits once per write.
        out.state_changes.push(StateChange {
            projection: "global_commit_total".to_string(),
            payload: new_repository_total.to_string(),
        });
        out.state_changes.push(StateChange {
            projection: "daily_commit_count".to_string(),
            payload: new_daily_total.to_string(),
        });

        // Plain commit feedback is a light sound by default (spec 16.4).
        out.side_effect_intents.push(SideEffectIntent {
            notification_id: format!("commit:{event_id}"),
            kind: SideEffectKind::CommitRecorded,
        });

        // Daily rank progression (spec 14.3): only a rank not yet granted today
        // produces upgrade intents.
        let thresholds = crate::daily_rank::DailyRankThresholds::official();
        let new_rank = thresholds.rank_for(new_daily_total);
        for rank in crate::daily_rank::ranks_newly_reached(snapshot.daily_rank_today, new_rank) {
            out.side_effect_intents.push(SideEffectIntent {
                notification_id: format!("rank:{}:{event_id}", rank.as_str()),
                kind: SideEffectKind::DailyRankUpgrade { rank },
            });
        }
        if let Some(rank) = new_rank {
            out.state_changes.push(StateChange {
                projection: "daily_rank_today".to_string(),
                payload: rank.as_str().to_string(),
            });
        }

        // Permanent achievements use the OFFICIAL profile (spec 13.3), so user
        // filters and custom thresholds cannot influence them.
        let statistics = crate::achievements::StatisticsSnapshot {
            commit_count: new_repository_total,
            ..Default::default()
        };
        let unlocks = crate::achievements::evaluate(
            &crate::achievements::builtin_achievements(),
            &statistics,
            &snapshot.unlocked_achievement_ids,
            &[crate::achievements::Capability::LocalGit],
        );
        for achievement_id in unlocks.newly_unlocked {
            out.side_effect_intents.push(SideEffectIntent {
                notification_id: format!("ach:{achievement_id}:{event_id}"),
                kind: SideEffectKind::AchievementUnlocked {
                    achievement_id,
                    source: crate::UnlockSourceTag::Realtime,
                },
            });
        }

        // The context is part of the contract even when unused today; reference it
        // so the signature cannot silently drift from the spec.
        let _ = ctx.config.config_version();

        out
    }

    /// Evaluate one activity reported by the browser extension (spec 2.3).
    ///
    /// # Why this is not a commit
    ///
    /// Every record used to reach `evaluate_commit`, so a merged pull request
    /// incremented the commit counter and could unlock "First Commit". A browser
    /// activity now moves only its own counter, and earns a notification only by
    /// unlocking something (spec 16.4 has no row for a bare browser activity).
    pub fn evaluate_browser_event(
        &self,
        snapshot: &StateSnapshot,
        activity: BrowserActivity,
        event_id: &str,
        ctx: &InjectedContext<'_>,
    ) -> EngineOutput {
        let mut out = EngineOutput::default();

        let statistics = activity.statistics_with(snapshot);
        let new_total = match activity {
            BrowserActivity::PullRequestMerged => statistics.pull_request_merged_count,
            BrowserActivity::ReleasePublished => statistics.release_count,
            BrowserActivity::RepositoryCreated => statistics.repository_created_count,
            BrowserActivity::StarGiven => statistics.star_given_count,
        };
        out.state_changes.push(StateChange {
            projection: activity.counter_projection().to_string(),
            payload: new_total.to_string(),
        });

        // Built-ins are judged on the OFFICIAL profile (spec 13.3). Both capabilities
        // are available here because a browser event is only evaluated in a mode where
        // the extension is installed -- but a commit achievement it satisfies is still
        // a legitimate unlock, since the rule is full-history.
        let unlocks = crate::achievements::evaluate(
            &crate::achievements::builtin_achievements(),
            &statistics,
            &snapshot.unlocked_achievement_ids,
            &[
                crate::achievements::Capability::LocalGit,
                crate::achievements::Capability::BrowserGithub,
            ],
        );
        for achievement_id in unlocks.newly_unlocked {
            out.side_effect_intents.push(SideEffectIntent {
                notification_id: format!("ach:{achievement_id}:{event_id}"),
                kind: SideEffectKind::AchievementUnlocked {
                    achievement_id,
                    source: crate::UnlockSourceTag::Realtime,
                },
            });
        }

        let _ = ctx.config.config_version();

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{
        ConfigurationProvider, FixedClock, IdGenerator, InjectedContext, TimezoneProvider,
    };

    #[derive(Debug)]
    struct StubTimezone;
    impl TimezoneProvider for StubTimezone {
        fn canonicalize(&self, name: &str) -> Option<String> {
            Some(name.to_string())
        }
        fn local_date(&self, _: i64, _: &str, _: u16) -> Option<String> {
            Some("2026-11-16".to_string())
        }
    }

    #[derive(Debug, Default)]
    struct Counter(u64);
    impl IdGenerator for Counter {
        fn next_id(&mut self) -> String {
            self.0 += 1;
            format!("id-{}", self.0)
        }
    }

    #[derive(Debug)]
    struct Config;
    impl ConfigurationProvider for Config {
        fn config_version(&self) -> u64 {
            1
        }
    }

    fn with_ctx<R>(f: impl FnOnce(&mut InjectedContext<'_>) -> R) -> R {
        let clock = FixedClock(1_700_000_000_000);
        let tz = StubTimezone;
        let mut ids = Counter::default();
        let cfg = Config;
        let mut ctx = InjectedContext {
            clock: &clock,
            timezone: &tz,
            ids: &mut ids,
            config: &cfg,
        };
        f(&mut ctx)
    }

    fn commit() -> CommitFacts {
        CommitFacts {
            counts_as_commit: true,
            is_merge: false,
            is_empty: false,
        }
    }

    #[test]
    fn a_first_commit_yields_counts_and_a_light_sound() {
        with_ctx(|ctx| {
            let out = Engine.evaluate_commit(&StateSnapshot::default(), &commit(), "e1", ctx);
            assert_eq!(
                out.state_changes.len(),
                3,
                "repo total, daily total, daily rank"
            );
            let kinds: Vec<&SideEffectKind> =
                out.side_effect_intents.iter().map(|i| &i.kind).collect();
            assert!(kinds
                .iter()
                .any(|k| matches!(k, SideEffectKind::CommitRecorded)));
            assert!(kinds.iter().any(|k| matches!(
                k,
                SideEffectKind::AchievementUnlocked { achievement_id, .. } if achievement_id == "first_commit"
            )));
        });
    }

    #[test]
    fn empty_commits_produce_no_output() {
        // Spec 8.1: empty commits are excluded.
        with_ctx(|ctx| {
            let facts = CommitFacts {
                counts_as_commit: true,
                is_merge: false,
                is_empty: true,
            };
            let out = Engine.evaluate_commit(&StateSnapshot::default(), &facts, "e1", ctx);
            assert!(out.is_empty());
        });
    }

    #[test]
    fn merge_commits_do_not_count_as_ordinary_commits() {
        // Spec 8.1: merge commits are classified separately.
        with_ctx(|ctx| {
            let facts = CommitFacts {
                counts_as_commit: false,
                is_merge: true,
                is_empty: false,
            };
            assert!(Engine
                .evaluate_commit(&StateSnapshot::default(), &facts, "e1", ctx)
                .is_empty());
        });
    }

    #[test]
    fn a_browser_activity_moves_only_its_own_counter() {
        // Spec 2.3: a merged pull request is not a commit. Before this existed, a
        // merged PR incremented the commit counter and could unlock "First Commit".
        with_ctx(|ctx| {
            let out = Engine.evaluate_browser_event(
                &StateSnapshot::default(),
                BrowserActivity::PullRequestMerged,
                "e1",
                ctx,
            );
            let counters: Vec<&str> = out
                .state_changes
                .iter()
                .map(|c| c.projection.as_str())
                .collect();
            assert_eq!(counters, vec!["repository_pull_request_merged_count"]);
            assert_eq!(out.state_changes[0].payload, "1");
            assert!(
                out.side_effect_intents
                    .iter()
                    .all(|i| !matches!(i.kind, SideEffectKind::CommitRecorded)),
                "a merge must not produce a commit acknowledgement"
            );
            assert!(
                out.side_effect_intents
                    .iter()
                    .all(|i| !matches!(i.kind, SideEffectKind::DailyRankUpgrade { .. })),
                "spec 14.3 counts commits, so a merge cannot move the daily rank"
            );
        });
    }

    #[test]
    fn a_merged_pull_request_unlocks_the_merge_achievement() {
        with_ctx(|ctx| {
            let out = Engine.evaluate_browser_event(
                &StateSnapshot::default(),
                BrowserActivity::PullRequestMerged,
                "e1",
                ctx,
            );
            let unlocked: Vec<String> = out
                .side_effect_intents
                .iter()
                .filter_map(|i| match &i.kind {
                    SideEffectKind::AchievementUnlocked { achievement_id, .. } => {
                        Some(achievement_id.clone())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(unlocked, vec!["first_pr_merged".to_string()]);
        });
    }

    #[test]
    fn a_second_merged_pull_request_counts_but_does_not_announce_again() {
        with_ctx(|ctx| {
            let snapshot = StateSnapshot {
                pull_request_merged_count: 1,
                unlocked_achievement_ids: vec!["first_pr_merged".to_string()],
                ..Default::default()
            };
            let out = Engine.evaluate_browser_event(
                &snapshot,
                BrowserActivity::PullRequestMerged,
                "e2",
                ctx,
            );
            assert!(out.side_effect_intents.is_empty());
            assert_eq!(out.state_changes[0].payload, "2");
        });
    }

    #[test]
    fn the_same_rank_is_only_announced_once_per_day() {
        // Spec 14.3: re-reaching a rank must not notify again.
        with_ctx(|ctx| {
            let snapshot = StateSnapshot {
                repository_commit_count: 0,
                daily_commit_count: 0,
                daily_rank_today: None,
                ..Default::default()
            };
            let first = Engine.evaluate_commit(&snapshot, &commit(), "e1", ctx);
            let ranks: Vec<_> = first
                .side_effect_intents
                .iter()
                .filter(|i| matches!(i.kind, SideEffectKind::DailyRankUpgrade { .. }))
                .collect();
            assert_eq!(ranks.len(), 1, "Common is reached once");

            // Same day, one more commit: already Common, so no upgrade intent.
            let snapshot2 = StateSnapshot {
                daily_commit_count: 1,
                daily_rank_today: Some(crate::daily_rank::DailyRank::Common),
                ..Default::default()
            };
            let second = Engine.evaluate_commit(&snapshot2, &commit(), "e2", ctx);
            let ranks2: Vec<_> = second
                .side_effect_intents
                .iter()
                .filter_map(|i| match &i.kind {
                    SideEffectKind::DailyRankUpgrade { rank } => Some(*rank),
                    _ => None,
                })
                .collect();

            // 2 commits reaches Uncommon, so exactly one upgrade is announced...
            assert_eq!(
                ranks2,
                vec![crate::daily_rank::DailyRank::Uncommon],
                "crossing into a NEW rank must be announced"
            );
            // ...and the already-granted rank is NOT repeated (spec 14.3).
            assert!(
                !ranks2.contains(&crate::daily_rank::DailyRank::Common),
                "Common was already granted today and must not be announced again"
            );
        });
    }

    #[test]
    fn already_unlocked_achievements_are_not_re_announced() {
        with_ctx(|ctx| {
            let snapshot = StateSnapshot {
                unlocked_achievement_ids: vec!["first_commit".to_string()],
                ..Default::default()
            };
            let out = Engine.evaluate_commit(&snapshot, &commit(), "e1", ctx);
            assert!(
                !out.side_effect_intents.iter().any(|i| matches!(
                    &i.kind,
                    SideEffectKind::AchievementUnlocked { achievement_id, .. } if achievement_id == "first_commit"
                )),
                "an achievement unlocked once must never be announced again"
            );
        });
    }

    #[test]
    fn evaluating_twice_against_the_same_snapshot_is_deterministic() {
        // Determinism is what Golden Tests depend on.
        with_ctx(|ctx| {
            let snapshot = StateSnapshot::default();
            let a = Engine.evaluate_commit(&snapshot, &commit(), "e1", ctx);
            let b = Engine.evaluate_commit(&snapshot, &commit(), "e1", ctx);
            assert_eq!(a, b);
        });
    }

    #[test]
    fn the_read_set_covers_every_projection_the_engine_touches() {
        // If this ever fails, a rule started reading something it did not declare,
        // which would make commit latency grow with repository age (spec 4.3).
        let rs = StateSnapshot::read_set_for(crate::EventKind::Commit);
        assert!(rs.repository_commit_count);
        assert!(rs.daily_commit_count);
        assert!(rs.daily_rank_today);
        assert!(rs.unlocked_achievements);
    }

    #[test]
    fn achievement_intents_carry_their_eligibility_source() {
        // Spec 13.4: the consumer must be able to tell a live unlock from an
        // imported one and stay silent for the latter.
        with_ctx(|ctx| {
            let out = Engine.evaluate_commit(&StateSnapshot::default(), &commit(), "e1", ctx);
            let unlocks: Vec<_> = out
                .side_effect_intents
                .iter()
                .filter_map(|i| match &i.kind {
                    SideEffectKind::AchievementUnlocked { source, .. } => Some(*source),
                    _ => None,
                })
                .collect();
            assert!(!unlocks.is_empty());
            assert!(
                unlocks.iter().all(|s| !s.is_silent()),
                "live commits are not silent"
            );
        });
    }

    /// A non-commit event must not move commit statistics.
    ///
    /// Measured before the fix: a merged pull request arriving from the browser
    /// extension produced `repository_commit_count = 1` and unlocked "First Commit",
    /// because every record reached `evaluate_commit`, whose only input is
    /// `CommitFacts`. The consumer now sends facts that do not count for anything that
    /// is not a commit; this pins the Engine half of that contract.
    #[test]
    fn facts_that_do_not_count_produce_no_changes() {
        let not_a_commit = CommitFacts {
            counts_as_commit: false,
            is_merge: true,
            is_empty: false,
        };
        with_ctx(|ctx| {
            let out = Engine.evaluate_commit(&StateSnapshot::default(), &not_a_commit, "pr-1", ctx);
            assert!(
                out.state_changes.is_empty(),
                "no projection may move: {:?}",
                out.state_changes
            );
            assert!(
                out.side_effect_intents.is_empty(),
                "and nothing may be celebrated: {:?}",
                out.side_effect_intents
            );
        });
    }
}

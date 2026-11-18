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
}

impl ReadSet {
    /// The read-set for a plain commit.
    pub const COMMIT: ReadSet = ReadSet {
        repository_commit_count: true,
        daily_commit_count: true,
        daily_rank_today: true,
        unlocked_achievements: true,
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

        out.state_changes.push(StateChange {
            projection: "repository_commit_count".to_string(),
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

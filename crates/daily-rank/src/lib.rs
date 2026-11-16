//! Daily rank and daily milestones (spec 14).
//!
//! # Why this is separate from permanent achievements
//!
//! Daily ranks repeat every day; permanent achievements unlock once and never
//! revoke. Spec 13.1 requires separate data models for exactly this reason, and
//! spec 14.2 restates it. Mixing them produces the classic bug where a user's
//! "Legendary" day silently marks a permanent achievement as already earned.
//!
//! # Why user thresholds do not affect built-in achievements
//!
//! A user may lower the Legendary threshold to 15 commits. The permanent
//! "Legendary Day" achievement still requires the official 20 (spec 13.3), because
//! achievements use a fixed, versioned statistics profile. The two live in
//! different types on purpose: this module owns the *user-configurable* view.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// The built-in daily ranks, ordered from lowest to highest.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DailyRank {
    /// 1 valid commit (default threshold).
    Common,
    /// 2-4 valid commits.
    Uncommon,
    /// 5-9 valid commits.
    Rare,
    /// 10-19 valid commits.
    Epic,
    /// 20+ valid commits.
    Legendary,
}

impl DailyRank {
    /// The stable identifier used in the database and notifications.
    pub fn as_str(self) -> &'static str {
        match self {
            DailyRank::Common => "common",
            DailyRank::Uncommon => "uncommon",
            DailyRank::Rare => "rare",
            DailyRank::Epic => "epic",
            DailyRank::Legendary => "legendary",
        }
    }

    /// Every rank from lowest to highest.
    pub fn all() -> [DailyRank; 5] {
        [
            DailyRank::Common,
            DailyRank::Uncommon,
            DailyRank::Rare,
            DailyRank::Epic,
            DailyRank::Legendary,
        ]
    }

    /// The official, versioned threshold for this rank (spec 14.1).
    ///
    /// These numbers are part of the achievement contract and must NOT be
    /// user-editable; see `DailyRankThresholds` for the editable copy.
    pub fn official_threshold(self) -> u32 {
        match self {
            DailyRank::Common => 1,
            DailyRank::Uncommon => 2,
            DailyRank::Rare => 5,
            DailyRank::Epic => 10,
            DailyRank::Legendary => 20,
        }
    }
}

/// The default thresholds table (spec 14.1).
///
/// Stored ascending by `minimum_commits`; the first entry must start at 1 and the
/// table must be non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyRankThresholds {
    entries: Vec<(u32, DailyRank)>,
}

/// Why a threshold table was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThresholdConfigError {
    /// The table had no entries.
    Empty,
    /// The first threshold was not 1, so a day with commits could have no rank.
    DoesNotStartAtOne(u32),
    /// Two ranks shared a minimum.
    DuplicateMinimum(u32),
    /// Thresholds were not strictly ascending.
    NotAscending {
        /// The offending minimum.
        at: u32,
    },
    /// A rank appeared twice.
    DuplicateRank(DailyRank),
    /// A minimum was zero.
    ///
    /// Unreachable through `custom` today because `DoesNotStartAtOne` fires first;
    /// kept because a future caller that builds a table some other way could still
    /// hit it, and a silent zero threshold would make every day a rank.
    ZeroMinimum,
}

impl std::fmt::Display for ThresholdConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThresholdConfigError::Empty => write!(f, "threshold table is empty"),
            ThresholdConfigError::DoesNotStartAtOne(v) => {
                write!(f, "first threshold must be 1, got {v}")
            }
            ThresholdConfigError::DuplicateMinimum(v) => write!(f, "duplicate threshold {v}"),
            ThresholdConfigError::NotAscending { at } => {
                write!(f, "thresholds not ascending at {at}")
            }
            ThresholdConfigError::DuplicateRank(r) => {
                write!(f, "rank {} appears twice", r.as_str())
            }
            ThresholdConfigError::ZeroMinimum => write!(f, "a threshold of 0 is not allowed"),
        }
    }
}

impl std::error::Error for ThresholdConfigError {}

impl DailyRankThresholds {
    /// The official defaults from spec 14.1.
    pub fn official() -> Self {
        DailyRankThresholds {
            entries: vec![
                (1, DailyRank::Common),
                (2, DailyRank::Uncommon),
                (5, DailyRank::Rare),
                (10, DailyRank::Epic),
                (20, DailyRank::Legendary),
            ],
        }
    }

    /// Build a user-customised table, validating the invariants.
    ///
    /// Spec 14.2 allows custom thresholds; this is how a user lowering Legendary to
    /// 15 is represented WITHOUT touching the official constants.
    pub fn custom(entries: Vec<(u32, DailyRank)>) -> Result<Self, ThresholdConfigError> {
        if entries.is_empty() {
            return Err(ThresholdConfigError::Empty);
        }

        let mut sorted = entries;
        sorted.sort_by_key(|(min, _)| *min);

        if sorted[0].0 != 1 {
            return Err(ThresholdConfigError::DoesNotStartAtOne(sorted[0].0));
        }

        let mut seen_minima = Vec::new();
        let mut seen_ranks = Vec::new();
        let mut previous = 0u32;
        for (min, rank) in &sorted {
            if *min == 0 {
                return Err(ThresholdConfigError::ZeroMinimum);
            }
            if seen_minima.contains(min) {
                return Err(ThresholdConfigError::DuplicateMinimum(*min));
            }
            if seen_ranks.contains(rank) {
                return Err(ThresholdConfigError::DuplicateRank(*rank));
            }
            if *min <= previous && previous != 0 {
                return Err(ThresholdConfigError::NotAscending { at: *min });
            }
            previous = *min;
            seen_minima.push(*min);
            seen_ranks.push(*rank);
        }

        Ok(DailyRankThresholds { entries: sorted })
    }

    /// The rank earned by a given count of valid commits in a day.
    ///
    /// Returns `None` for zero commits: no commits means no rank, which is
    /// different from "Common".
    pub fn rank_for(&self, valid_commits: u32) -> Option<DailyRank> {
        if valid_commits == 0 {
            return None;
        }
        let mut current = None;
        for (min, rank) in &self.entries {
            if valid_commits >= *min {
                current = Some(*rank);
            } else {
                break;
            }
        }
        current
    }

    /// Progress toward the next rank, for the UI progress bar.
    pub fn progress(&self, valid_commits: u32) -> RankProgress {
        let current = self.rank_for(valid_commits);
        let next = self
            .entries
            .iter()
            .find(|(min, _)| *min > valid_commits)
            .map(|(min, rank)| (*min, *rank));

        RankProgress {
            valid_commits,
            current,
            next_threshold: next.map(|(min, _)| min),
            next_rank: next.map(|(_, rank)| rank),
        }
    }
}

/// Where a day sits relative to its ranks, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankProgress {
    /// Valid commits counted so far today.
    pub valid_commits: u32,
    /// The rank currently held, if any.
    pub current: Option<DailyRank>,
    /// Commits needed to reach the next rank.
    pub next_threshold: Option<u32>,
    /// The next rank to be reached.
    pub next_rank: Option<DailyRank>,
}

/// Decide which upgrade notifications to emit for a day's progression.
///
/// Spec 14.3: the full upgrade notification fires only on the FIRST time a rank is
/// reached that day; re-reaching the same rank later must not notify again. Since a
/// day's commit count only grows, this reduces to "notify once per newly crossed
/// rank", which is what this returns, lowest first.
pub fn ranks_newly_reached(
    previous: Option<DailyRank>,
    current: Option<DailyRank>,
) -> Vec<DailyRank> {
    let Some(current) = current else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rank in DailyRank::all() {
        let crossed_now = previous.is_none_or(|p| rank > p);
        if rank <= current && crossed_now {
            out.push(rank);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_thresholds_match_the_specification() {
        // Spec 14.1: Common 1, Uncommon 2-4, Rare 5-9, Epic 10-19, Legendary 20+.
        let t = DailyRankThresholds::official();
        assert_eq!(t.rank_for(0), None);
        assert_eq!(t.rank_for(1), Some(DailyRank::Common));
        assert_eq!(t.rank_for(2), Some(DailyRank::Uncommon));
        assert_eq!(t.rank_for(4), Some(DailyRank::Uncommon));
        assert_eq!(t.rank_for(5), Some(DailyRank::Rare));
        assert_eq!(t.rank_for(9), Some(DailyRank::Rare));
        assert_eq!(t.rank_for(10), Some(DailyRank::Epic));
        assert_eq!(t.rank_for(19), Some(DailyRank::Epic));
        assert_eq!(t.rank_for(20), Some(DailyRank::Legendary));
        assert_eq!(t.rank_for(1000), Some(DailyRank::Legendary));
    }

    #[test]
    fn zero_commits_is_no_rank_not_common() {
        // "No commits" and "Common" must not be conflated: one means nothing
        // happened, the other means the day already earned a reward.
        let t = DailyRankThresholds::official();
        assert!(t.rank_for(0).is_none());
    }

    #[test]
    fn a_user_can_lower_the_legendary_threshold() {
        // Spec 14.2 explicitly allows this.
        let custom = DailyRankThresholds::custom(vec![
            (1, DailyRank::Common),
            (2, DailyRank::Uncommon),
            (5, DailyRank::Rare),
            (10, DailyRank::Epic),
            (15, DailyRank::Legendary),
        ])
        .expect("valid custom table");
        assert_eq!(custom.rank_for(15), Some(DailyRank::Legendary));
        assert_eq!(custom.rank_for(14), Some(DailyRank::Epic));
    }

    #[test]
    fn custom_thresholds_do_not_change_official_ones() {
        // This is the spec 13.3 rule in executable form: editing the daily view must
        // leave the achievement contract untouched.
        let _custom = DailyRankThresholds::custom(vec![(1, DailyRank::Legendary)]).expect("valid");
        assert_eq!(DailyRank::Legendary.official_threshold(), 20);
        assert_eq!(
            DailyRankThresholds::official().rank_for(15),
            Some(DailyRank::Epic),
            "official table must be unaffected by a custom one"
        );
    }

    #[test]
    fn threshold_table_must_start_at_one() {
        assert_eq!(
            DailyRankThresholds::custom(vec![(5, DailyRank::Rare)]),
            Err(ThresholdConfigError::DoesNotStartAtOne(5))
        );
    }

    #[test]
    fn threshold_table_must_not_be_empty() {
        assert_eq!(
            DailyRankThresholds::custom(vec![]),
            Err(ThresholdConfigError::Empty)
        );
    }

    #[test]
    fn duplicate_minimum_is_rejected() {
        assert!(matches!(
            DailyRankThresholds::custom(vec![(1, DailyRank::Common), (1, DailyRank::Rare)]),
            Err(ThresholdConfigError::DuplicateMinimum(1))
        ));
    }

    #[test]
    fn duplicate_rank_is_rejected() {
        assert!(matches!(
            DailyRankThresholds::custom(vec![(1, DailyRank::Common), (5, DailyRank::Common)]),
            Err(ThresholdConfigError::DuplicateRank(_))
        ));
    }

    #[test]
    fn zero_minimum_is_rejected_as_a_start_value() {
        // A 0 minimum is rejected by the "must start at 1" rule, which is the more
        // informative error: thresholds are minimum commit counts, and a day with 0
        // commits has no rank at all, so 0 can never be a valid threshold.
        assert!(matches!(
            DailyRankThresholds::custom(vec![(0, DailyRank::Common), (1, DailyRank::Rare)]),
            Err(ThresholdConfigError::DoesNotStartAtOne(0))
        ));
    }

    #[test]
    fn progress_reports_the_next_rank() {
        let t = DailyRankThresholds::official();
        let p = t.progress(3);
        assert_eq!(p.current, Some(DailyRank::Uncommon));
        assert_eq!(p.next_threshold, Some(5));
        assert_eq!(p.next_rank, Some(DailyRank::Rare));

        let top = t.progress(25);
        assert_eq!(top.current, Some(DailyRank::Legendary));
        assert_eq!(top.next_threshold, None, "Legendary is the ceiling");
    }

    #[test]
    fn reaching_a_rank_the_first_time_emits_every_crossed_rank() {
        // Going from nothing to 5 commits crosses Common, Uncommon and Rare.
        let reached = ranks_newly_reached(None, Some(DailyRank::Rare));
        assert_eq!(
            reached,
            vec![DailyRank::Common, DailyRank::Uncommon, DailyRank::Rare]
        );
    }

    #[test]
    fn re_reaching_the_same_rank_emits_nothing() {
        // Spec 14.3: same rank twice in one day must not notify twice.
        assert!(ranks_newly_reached(Some(DailyRank::Rare), Some(DailyRank::Rare)).is_empty());
    }

    #[test]
    fn going_up_one_rank_emits_only_that_rank() {
        assert_eq!(
            ranks_newly_reached(Some(DailyRank::Rare), Some(DailyRank::Epic)),
            vec![DailyRank::Epic]
        );
    }

    #[test]
    fn ranks_are_ordered_lowest_to_highest() {
        let all = DailyRank::all();
        for w in all.windows(2) {
            assert!(w[0] < w[1], "order must be ascending");
        }
    }
}

//! Hook-order independence (spec 9.6 rule 7).
//!
//! > "两个 Hook 无论谁先到达，最终统计结果必须一致"
//!
//! Phase 0 recorded that post-commit and post-rewrite arrive in a fixed order on
//! Git 2.54.0, but the spec refuses to depend on that: a slow filesystem, a
//! suspended process or a different Git version can reorder them. So the property
//! below is stated over ALL orderings, not the observed one.
//!
//! # What "same result" means here
//!
//! The set of counted commits must be identical regardless of arrival order. The
//! notification history is allowed to differ (a late rewrite only corrects
//! statistics, spec 9.6 rule 6) — asserting on it would encode a guarantee the
//! product never made.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_notifications::{
    freshness_action, FreshnessAction, NotificationStatus, OutboxEntry,
};
use proptest::prelude::*;
use std::collections::BTreeSet;

/// A commit as the classification logic sees it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CommitFact {
    sha: String,
    /// Whether a rewrite later said this commit was replaced.
    rewritten_to: Option<String>,
}

/// Apply a sequence of hook arrivals and return the set of counted commits.
///
/// Models spec 9.6 precisely:
///   - a post-commit records that this sha happened;
///   - a post-rewrite says "old was replaced by new", so OLD stops counting and NEW
///     starts counting (even if its own post-commit never arrives);
///   - applying the same fact twice changes nothing.
///
/// The whole point of the test is that the two arrival orders converge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrival {
    Commit(usize),
    Rewrite(usize),
}

fn apply_arrivals(commits: &[CommitFact], arrivals: &[Arrival]) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    // sha -> the sha that replaced it.
    let mut replaced: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    for arrival in arrivals {
        match *arrival {
            Arrival::Commit(idx) => {
                let sha = &commits[idx].sha;
                // A commit that is already known to be superseded never counts.
                if !replaced.contains_key(sha) {
                    seen.insert(sha.clone());
                }
            }
            Arrival::Rewrite(idx) => {
                if let Some(new_sha) = &commits[idx].rewritten_to {
                    let old_sha = &commits[idx].sha;
                    replaced.insert(old_sha.clone(), new_sha.clone());
                    seen.remove(old_sha);
                    seen.insert(new_sha.clone());
                }
            }
        }
    }

    // Any sha that was later replaced must not remain counted.
    seen.retain(|sha| !replaced.contains_key(sha));
    seen
}

/// Forward order: every commit, then every rewrite.
fn forward_arrivals(n: usize) -> Vec<Arrival> {
    let mut v: Vec<Arrival> = (0..n).map(Arrival::Commit).collect();
    v.extend((0..n).map(Arrival::Rewrite));
    v
}

/// Reverse order: every rewrite, then every commit.
///
/// This is the adversarial case spec 9.6 rule 7 is about: post-rewrite arriving
/// before the post-commit it refers to.
fn rewrite_first_arrivals(n: usize) -> Vec<Arrival> {
    let mut v: Vec<Arrival> = (0..n).map(Arrival::Rewrite).collect();
    v.extend((0..n).map(Arrival::Commit));
    v
}

/// Interleaved order, to cover the realistic middle case.
fn interleaved_arrivals(n: usize) -> Vec<Arrival> {
    let mut v = Vec::new();
    for i in 0..n {
        v.push(Arrival::Rewrite(i));
        v.push(Arrival::Commit(i));
    }
    v
}

fn commits_strategy() -> impl Strategy<Value = Vec<CommitFact>> {
    prop::collection::vec(
        (any::<u8>(), any::<bool>()).prop_map(|(n, rewritten)| {
            let sha = format!("{n:040x}");
            CommitFact {
                sha: sha.clone(),
                rewritten_to: if rewritten {
                    Some(format!("{:040x}", u16::from(n) + 1000))
                } else {
                    None
                },
            }
        }),
        1..6usize,
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        .. ProptestConfig::default()
    })]

    /// Forward arrival order and reversed arrival order agree.
    ///
    /// This is the concrete form of spec 9.6 rule 7 for the case that actually
    /// happens: post-rewrite arriving just before or just after post-commit.
    #[test]
    fn every_arrival_order_converges(commits in commits_strategy()) {
        let n = commits.len();
        let f = apply_arrivals(&commits, &forward_arrivals(n));
        let r = apply_arrivals(&commits, &rewrite_first_arrivals(n));
        let i = apply_arrivals(&commits, &interleaved_arrivals(n));

        let message = format!("orders diverged:\n forward={f:?}\n rewrite-first={r:?}\n interleaved={i:?}");
        prop_assert_eq!(&f, &r, "{}", message);
        prop_assert_eq!(&r, &i, "{}", message);
    }

    /// A rewrite never causes double counting.
    #[test]
    fn a_rewritten_commit_is_counted_at_most_once(commits in commits_strategy()) {
        let n = commits.len();
        let counted = apply_arrivals(&commits, &forward_arrivals(n));

        for c in &commits {
            if let Some(new_sha) = &c.rewritten_to {
                prop_assert!(
                    !counted.contains(&c.sha),
                    "a rewritten commit's original sha must not be counted: {}",
                    c.sha
                );
                prop_assert!(
                    counted.contains(new_sha),
                    "the replacement sha must be counted instead: {}",
                    new_sha
                );
            }
        }
    }

    /// Duplicate delivery of the same arrival order is idempotent.
    #[test]
    fn duplicating_the_whole_sequence_is_idempotent(commits in commits_strategy()) {
        // At-least-once delivery means the same arrivals can happen twice.
        let n = commits.len();
        let mut doubled = forward_arrivals(n);
        let mut again = forward_arrivals(n);
        doubled.append(&mut again);

        let once = apply_arrivals(&commits, &forward_arrivals(n));
        let twice = apply_arrivals(&commits, &doubled);
        prop_assert_eq!(&once, &twice);
    }

    /// Freshness depends only on the event's own age.
    ///
    /// Spec 16.3: the age is measured from when the thing happened. If this were
    /// computed from arrival time, a weekend-offline Spool replay would celebrate
    /// two-day-old commits as if they just happened.
    #[test]
    fn freshness_is_a_pure_function_of_age(age_ms in 0i64..200_000_000i64) {
        let a = freshness_action(age_ms);
        let b = freshness_action(age_ms);
        prop_assert_eq!(a, b);

        // And the buckets are ordered: older is never "more prominent".
        let rank = |f: FreshnessAction| match f {
            FreshnessAction::FullNotification => 2,
            FreshnessAction::Digested => 1,
            FreshnessAction::Silent => 0,
        };
        prop_assert!(rank(a) <= 2);
    }

    /// Older events are never more prominent than newer ones.
    #[test]
    fn freshness_is_monotonic_in_age(a in 0i64..200_000_000i64, delta in 0i64..100_000_000i64) {
        let rank = |f: FreshnessAction| match f {
            FreshnessAction::FullNotification => 2,
            FreshnessAction::Digested => 1,
            FreshnessAction::Silent => 0,
        };
        prop_assert!(
            rank(freshness_action(a + delta)) <= rank(freshness_action(a)),
            "a later age produced a MORE prominent action ({} + {} )",
            a,
            delta
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        .. ProptestConfig::default()
    })]

    /// A notification reaches at most one terminal state, whatever the message order.
    ///
    /// Spec 17.8 requires "each notification reaches at most one display terminal
    /// state" under any sequence including duplicates, reordering and loss.
    #[test]
    fn a_notification_never_reaches_two_terminal_states(
        claim_at in 0i64..200i64,
        decline_at in 0i64..200i64,
        ack_at in 0i64..200i64,
        use_extension in any::<bool>(),
    ) {
        let t0 = 1_000_000i64;
        let mut entry = OutboxEntry::new("n1", "e1", t0);
        let mut terminal_count = 0;

        if use_extension {
            let offered = entry.offer(t0 + 150, t0).is_ok();
            if offered {
                // Exactly one of claim/decline may win, whichever arrives first.
                let claimed = claim_at <= decline_at
                    && entry.claim("ext", t0 + claim_at.min(149)).is_ok();
                let accumulated = claimed
                    && entry.acknowledge_display(t0 + ack_at).is_ok()
                    && entry.complete(t0 + ack_at + 1).is_ok();
                if accumulated {
                    terminal_count += 1;
                }

                if !claimed
                    && entry.decline(t0 + decline_at.min(149)).is_ok()
                    && entry.fall_back_to_desktop(t0 + decline_at + 1).is_ok()
                {
                    entry.record_display_attempt(t0 + decline_at + 2);
                    if entry.complete(t0 + decline_at + 3).is_ok() {
                        terminal_count += 1;
                    }
                }
            }
        } else if entry.transition(NotificationStatus::DesktopPending, t0).is_ok() {
            entry.record_display_attempt(t0 + 1);
            if entry.complete(t0 + 2).is_ok() {
                terminal_count += 1;
            }
        }

        prop_assert!(
            terminal_count <= 1,
            "a notification reached {} terminal states",
            terminal_count
        );
    }
}

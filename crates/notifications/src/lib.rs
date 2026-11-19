//! Outbox state machine and notification ownership protocol (spec 16, 17).
//!
//! # The problem this solves
//!
//! In Full Experience two things can show a notification: the browser extension
//! and the desktop companion. Showing it twice is a visible bug. The spec's answer
//! is an atomic claim protocol with a strict precedence for failure:
//!
//! > **Rather notify too little than notify twice** (spec 17.5, 17.9)
//!
//! That single sentence drives every decision here. When in doubt the state machine
//! refuses to re-display, and instead relies on the achievement gallery's "new
//! unlock" marker so the user can still find what happened (spec 24.2).
//!
//! # Why the transitions are validated rather than just set
//!
//! Spec 17.8 requires that under ANY message sequence (duplicates, reordering,
//! loss) each notification reaches at most one display terminal state. Enforcing the
//! legal-transition table in code is what makes that testable, instead of relying on
//! every call site remembering the rules.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// The outbox row status (spec 17.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    /// The outbox record exists.
    Created,
    /// An offer was sent to connected extensions.
    Offered,
    /// One extension atomically claimed it.
    Claimed,
    /// An extension confirmed it displayed.
    Displayed,
    /// Every connected extension explicitly declined.
    Declined,
    /// The offer expired with no claim.
    Expired,
    /// Waiting for the desktop to display it.
    DesktopPending,
    /// The desktop displayed it.
    DesktopDisplayed,
    /// Display and sound both finished.
    Completed,
    /// Claimed, but the ACK was lost and the extension is unreachable.
    DeliveryUnknown,
    /// Retries exhausted.
    Failed,
    /// Suppressed by policy (freshness, quiet mode).
    Suppressed,
}

impl NotificationStatus {
    /// Terminal states never change again (spec 17.2).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            NotificationStatus::Completed
                | NotificationStatus::DeliveryUnknown
                | NotificationStatus::Failed
                | NotificationStatus::Suppressed
        )
    }

    /// Whether this state means "a user has been shown something".
    ///
    /// Used by the "at most one display terminal state" invariant: `completed` after
    /// a desktop display and `completed` after an extension display are both a
    /// display, but a notification must never go through both routes.
    pub fn implies_display(self) -> bool {
        matches!(
            self,
            NotificationStatus::Displayed
                | NotificationStatus::DesktopDisplayed
                | NotificationStatus::Completed
                | NotificationStatus::DeliveryUnknown
        )
    }

    /// Whether the state means "the extension route was used".
    pub fn uses_extension_route(self) -> bool {
        matches!(
            self,
            NotificationStatus::Offered
                | NotificationStatus::Claimed
                | NotificationStatus::Displayed
                | NotificationStatus::Declined
                | NotificationStatus::Expired
        )
    }

    /// Whether the state means "the desktop route was used".
    pub fn uses_desktop_route(self) -> bool {
        matches!(
            self,
            NotificationStatus::DesktopPending | NotificationStatus::DesktopDisplayed
        )
    }

    /// The stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationStatus::Created => "created",
            NotificationStatus::Offered => "offered",
            NotificationStatus::Claimed => "claimed",
            NotificationStatus::Displayed => "displayed",
            NotificationStatus::Declined => "declined",
            NotificationStatus::Expired => "expired",
            NotificationStatus::DesktopPending => "desktop_pending",
            NotificationStatus::DesktopDisplayed => "desktop_displayed",
            NotificationStatus::Completed => "completed",
            NotificationStatus::DeliveryUnknown => "delivery_unknown",
            NotificationStatus::Failed => "failed",
            NotificationStatus::Suppressed => "suppressed",
        }
    }
}

/// Why a transition was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The notification is already in a terminal state.
    Terminal {
        /// The state it is stuck in.
        from: NotificationStatus,
    },
    /// The transition is not in the legal table.
    Illegal {
        /// Current state.
        from: NotificationStatus,
        /// Requested state.
        to: NotificationStatus,
    },
    /// A claim arrived after the offer had already been resolved.
    AlreadyHandled,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::Terminal { from } => {
                write!(f, "notification is already terminal ({})", from.as_str())
            }
            TransitionError::Illegal { from, to } => {
                write!(f, "illegal transition {} -> {}", from.as_str(), to.as_str())
            }
            TransitionError::AlreadyHandled => write!(f, "notification was already handled"),
        }
    }
}

impl std::error::Error for TransitionError {}

/// The legal transition table (spec 17.3).
///
/// Kept as one function so the table can be reviewed against the spec in one place.
fn is_legal(from: NotificationStatus, to: NotificationStatus) -> bool {
    use NotificationStatus::*;
    matches!(
        (from, to),
        // Creation routes.
        (Created, Offered)
            | (Created, DesktopPending)
            | (Created, Suppressed)
            // Extension route.
            | (Offered, Claimed)
            | (Offered, Declined)
            | (Offered, Expired)
            // Everything that went unanswered falls back to the desktop.
            | (Declined, DesktopPending)
            | (Expired, DesktopPending)
            // An extension that claimed but then reported it could not show it.
            | (Claimed, DesktopPending)
            | (Claimed, Displayed)
            | (Claimed, DeliveryUnknown)
            | (Displayed, Completed)
            // Desktop route.
            | (DesktopPending, DesktopDisplayed)
            | (DesktopDisplayed, Completed)
            // Any non-terminal state may fail when retries run out.
            | (Created, Failed)
            | (Offered, Failed)
            | (Claimed, Failed)
            | (Declined, Failed)
            | (Expired, Failed)
            | (DesktopPending, Failed)
            | (DesktopDisplayed, Failed)
    )
}

/// An outbox record (spec 16.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// Stable notification id; also the idempotency key.
    pub notification_id: String,
    /// The event that caused it.
    pub event_id: String,
    /// Current status.
    pub status: NotificationStatus,
    /// Which channel owns delivery right now.
    pub owner_channel: Option<Channel>,
    /// Which extension instance holds the claim.
    pub claimer_id: Option<String>,
    /// When the offer stops being claimable, Unix epoch milliseconds.
    pub offer_deadline: Option<i64>,
    /// When the row was created.
    pub created_at: i64,
    /// Delivery attempts so far.
    pub attempts: u32,
    /// Last error, for diagnostics.
    pub last_error: Option<String>,
    /// When it reached a terminal state.
    pub processed_at: Option<i64>,
    /// When a desktop display was attempted, used for crash semantics (spec 17.9).
    pub display_attempted_at: Option<i64>,
}

/// Which channel is responsible for delivering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// The browser extension.
    Extension,
    /// The desktop companion.
    Desktop,
}

impl OutboxEntry {
    /// Create a new outbox row in the `created` state.
    pub fn new(
        notification_id: impl Into<String>,
        event_id: impl Into<String>,
        created_at: i64,
    ) -> Self {
        OutboxEntry {
            notification_id: notification_id.into(),
            event_id: event_id.into(),
            status: NotificationStatus::Created,
            owner_channel: None,
            claimer_id: None,
            offer_deadline: None,
            created_at,
            attempts: 0,
            last_error: None,
            processed_at: None,
            display_attempted_at: None,
        }
    }

    /// Apply a transition, rejecting anything not in the legal table.
    ///
    /// This is the ONLY way status changes. Spec 17.4 forbids relying on a frontend
    /// boolean; by the same logic it must not rely on a bare field assignment.
    pub fn transition(
        &mut self,
        to: NotificationStatus,
        now_ms: i64,
    ) -> Result<(), TransitionError> {
        if self.status.is_terminal() {
            return Err(TransitionError::Terminal { from: self.status });
        }
        if !is_legal(self.status, to) {
            return Err(TransitionError::Illegal {
                from: self.status,
                to,
            });
        }

        self.status = to;
        if to.is_terminal() {
            self.processed_at = Some(now_ms);
        }
        Ok(())
    }

    /// Send an offer to connected extensions (spec 17.3: only when one is connected).
    pub fn offer(&mut self, deadline_ms: i64, now_ms: i64) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::Offered, now_ms)?;
        self.offer_deadline = Some(deadline_ms);
        self.owner_channel = Some(Channel::Extension);
        Ok(())
    }

    /// Attempt to claim. First claim wins; later ones are rejected (spec 17.4).
    ///
    /// Returns `AlreadyHandled` when the offer has already been resolved, which is
    /// the exact response the spec requires for a late claim.
    pub fn claim(&mut self, claimer: &str, now_ms: i64) -> Result<(), TransitionError> {
        if self.status != NotificationStatus::Offered {
            return Err(TransitionError::AlreadyHandled);
        }
        // Deadline enforcement belongs to the state machine, not the caller: a
        // claim that arrives after the deadline must not be granted.
        if let Some(deadline) = self.offer_deadline {
            if now_ms > deadline {
                return Err(TransitionError::AlreadyHandled);
            }
        }
        self.transition(NotificationStatus::Claimed, now_ms)?;
        self.claimer_id = Some(claimer.to_string());
        Ok(())
    }

    /// Record that the extension displayed it.
    pub fn acknowledge_display(&mut self, now_ms: i64) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::Displayed, now_ms)
    }

    /// Record that the extension declined.
    pub fn decline(&mut self, now_ms: i64) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::Declined, now_ms)
    }

    /// Move to the desktop fallback.
    pub fn fall_back_to_desktop(&mut self, now_ms: i64) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::DesktopPending, now_ms)?;
        self.owner_channel = Some(Channel::Desktop);
        Ok(())
    }

    /// Persist the display attempt BEFORE actually showing anything (spec 17.9).
    ///
    /// This is the crash-safety hook: if the process dies between this call and the
    /// completion write, recovery must not re-display. It is a separate method
    /// precisely so it cannot be forgotten at a call site.
    pub fn record_display_attempt(&mut self, now_ms: i64) {
        self.display_attempted_at = Some(now_ms);
        self.transition(NotificationStatus::DesktopDisplayed, now_ms)
            .ok();
    }

    /// Whether a crash before completion means we must NOT re-display (spec 17.9).
    pub fn must_not_redisplay_after_crash(&self) -> bool {
        self.display_attempted_at.is_some()
            && !matches!(self.status, NotificationStatus::Completed)
            && self.status != NotificationStatus::Suppressed
    }

    /// Complete delivery.
    pub fn complete(&mut self, now_ms: i64) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::Completed, now_ms)
    }

    /// Mark delivery as unknown rather than re-notifying (spec 17.5).
    pub fn mark_delivery_unknown(
        &mut self,
        reason: &str,
        now_ms: i64,
    ) -> Result<(), TransitionError> {
        self.transition(NotificationStatus::DeliveryUnknown, now_ms)?;
        self.last_error = Some(reason.to_string());
        Ok(())
    }
}

/// Freshness policy (spec 16.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessAction {
    /// Show the full notification plus sound.
    FullNotification,
    /// Show a digest instead of one alert per event.
    Digested,
    /// Update statistics silently.
    Silent,
}

/// Decide what to do with an event of a given age.
///
/// Age is measured from `occurred_at`, not from when the system noticed it. Getting
/// this wrong is how a Spool replay after a weekend triggers a pile of "just now"
/// celebrations.
pub fn freshness_action(age_ms: i64) -> FreshnessAction {
    const TEN_MINUTES: i64 = 10 * 60 * 1000;
    const DAY: i64 = 24 * 60 * 60 * 1000;
    if age_ms < 0 {
        // Clock skew. Treat as fresh rather than pretending it is ancient, but never
        // claim it is older than it is.
        FreshnessAction::FullNotification
    } else if age_ms <= TEN_MINUTES {
        FreshnessAction::FullNotification
    } else if age_ms <= DAY {
        FreshnessAction::Digested
    } else {
        FreshnessAction::Silent
    }
}

/// A single notification ready to be merged for display (spec 16.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationItem {
    /// The achievement or rank id.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Rarity, used to pick which item leads the merged popup.
    pub rarity: crate::Rarity,
    /// Whether this is a daily rank rather than a permanent achievement.
    pub is_daily_rank: bool,
}

/// The merged result of several items triggered by one event (spec 16.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedNotification {
    /// The item whose rarity and sound lead the popup.
    pub primary: NotificationItem,
    /// Up to three further items shown as sub-lines.
    pub others: Vec<NotificationItem>,
    /// How many items did not fit and are only summarised as "+N".
    pub omitted: usize,
}

/// Merge several items into at most one popup (spec 16.6).
///
/// Ordering rules from the spec:
///   - the highest rarity leads;
///   - on equal rarity, a permanent achievement leads over a daily rank;
///   - at most 3 items are listed, the rest become "+N".
pub fn merge_notification(mut items: Vec<NotificationItem>) -> Option<MergedNotification> {
    if items.is_empty() {
        return None;
    }

    // Highest rarity first; on a tie, achievements (is_daily_rank == false) first.
    items.sort_by(|a, b| {
        b.rarity
            .cmp(&a.rarity)
            .then(a.is_daily_rank.cmp(&b.is_daily_rank))
    });

    let mut iter = items.into_iter();
    let primary = iter.next()?;
    let rest: Vec<NotificationItem> = iter.collect();
    let listed = rest.len().min(3);
    let others = rest.iter().take(listed).cloned().collect();
    let omitted = rest.len() - listed;

    Some(MergedNotification {
        primary,
        others,
        omitted,
    })
}

// Re-export Rarity so this module's API reads cleanly.
pub use devchievements_achievements::Rarity;

/// What the consumer should actually do for one event (spec 16.4, 16.6).
///
/// The distinction exists because spec 16.4 does NOT treat every event the same:
/// a plain commit gets a light sound and no popup, while an unlock gets a full
/// popup. Treating them uniformly is the easy mistake, and it is a visible one --
/// every commit would interrupt the user.
///
/// Spec 16.3 adds a third band: an event more than ten minutes old is digested, so it
/// gets [`Presentation::Digest`] instead of a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presentation {
    /// Show a merged popup (spec 16.6).
    Popup(Popup),
    /// Play a light acknowledgement sound and show nothing.
    SoundOnly,
    /// Show one summary for several quiet events, never a celebration window.
    Digest(DigestSummary),
    /// Nothing at all.
    Nothing,
}

/// Several events folded into a single, non-interrupting summary (spec 16.3).
///
/// The digest is deliberately not a [`Popup`]: events between ten minutes and a day
/// old happened while the user was away, so opening a window per record would be the
/// exact pile of "just unlocked" alerts a Spool replay used to produce after a
/// weekend. One line, no window, and the gallery still shows the real unlocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestSummary {
    /// How many events were folded into this summary.
    pub events: usize,
    /// Titles of the achievements those events unlocked.
    pub unlocks: Vec<String>,
    /// Unlocks beyond the listed titles, already counted in the headline.
    pub omitted_unlocks: usize,
}

impl DigestSummary {
    /// How many unlocks the summary stands for, listed or not.
    pub fn unlock_count(&self) -> usize {
        self.unlocks.len() + self.omitted_unlocks
    }

    /// One readable line. English only, like the rest of this crate's copy: the UI
    /// localises its own strings, and a digest is delivered outside the UI.
    pub fn headline(&self) -> String {
        match self.unlock_count() {
            0 => format!("{} events recorded quietly", self.events),
            1 => "1 achievement unlocked while you were away".to_string(),
            n => format!("{n} achievements unlocked while you were away"),
        }
    }

    /// Fold another digest into this one.
    ///
    /// A Spool replay produces one digest row per event, but the user must see one
    /// summary for the whole replay (spec 16.3), so delivery merges them as it goes.
    /// The three-title cap is applied again after merging, and whatever no longer
    /// fits is added to the count rather than dropped.
    pub fn merged_with(mut self, other: DigestSummary) -> DigestSummary {
        self.events += other.events;
        let omitted = self.omitted_unlocks + other.omitted_unlocks;
        self.unlocks.extend(other.unlocks);

        let listed = self.unlocks.len().min(3);
        let overflow = self.unlocks.len() - listed;
        self.unlocks.truncate(listed);
        self.omitted_unlocks = omitted + overflow;
        self
    }
}

/// The content of a merged popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    /// The leading item's title.
    pub title: String,
    /// The id of the leading item, used to pick the sound and icon.
    pub primary_id: String,
    /// Rarity of the leading item.
    pub rarity: Rarity,
    /// Sub-lines, at most three (spec 16.6).
    pub others: Vec<String>,
    /// How many items did not fit and are summarised as "+N".
    pub omitted: usize,
}

/// Decide what to present for a single event.
///
/// # Rules applied, in order
///
/// 1. An event that only produced a plain commit is sound-only (spec 16.4).
/// 2. One or more unlocks produce exactly one popup, merged per spec 16.6.
/// 3. No items and no commit means nothing.
///
/// The plain-commit signal is passed separately rather than folded into `items`
/// because a commit is not a reward: it must never become the popup's title, and it
/// must not consume one of the three sub-line slots.
pub fn present_event(items: Vec<NotificationItem>, had_plain_commit: bool) -> Presentation {
    match merge_notification(items) {
        Some(merged) => Presentation::Popup(Popup {
            title: merged.primary.title.clone(),
            primary_id: merged.primary.id.clone(),
            rarity: merged.primary.rarity,
            others: merged.others.iter().map(|i| i.title.clone()).collect(),
            omitted: merged.omitted,
        }),
        None if had_plain_commit => Presentation::SoundOnly,
        None => Presentation::Nothing,
    }
}

/// Fold several digested events into one summary (spec 16.3).
///
/// `unlocks` is every title the folded events unlocked, in delivery order. At most
/// three are listed on the line; the rest are counted so the headline stays honest
/// about how much happened rather than silently truncating it.
pub fn digest_presentation(events: usize, unlocks: Vec<String>) -> Presentation {
    let listed = unlocks.len().min(3);
    let listed_unlocks = unlocks[..listed].to_vec();
    Presentation::Digest(DigestSummary {
        events,
        omitted_unlocks: unlocks.len() - listed,
        unlocks: listed_unlocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;

    fn offered() -> OutboxEntry {
        let mut e = OutboxEntry::new("n1", "e1", T0);
        e.offer(T0 + 150, T0).expect("offer from created");
        e
    }

    #[test]
    fn created_can_go_to_offered_desktop_or_suppressed() {
        for target in [
            NotificationStatus::Offered,
            NotificationStatus::DesktopPending,
            NotificationStatus::Suppressed,
        ] {
            let mut e = OutboxEntry::new("n", "e", T0);
            assert!(
                e.transition(target, T0).is_ok(),
                "{target:?} should be legal"
            );
        }
    }

    #[test]
    fn created_cannot_jump_straight_to_completed() {
        // Skipping the display would let a notification be "done" without ever
        // being shown, which breaks the discovery guarantees.
        let mut e = OutboxEntry::new("n", "e", T0);
        assert!(matches!(
            e.transition(NotificationStatus::Completed, T0),
            Err(TransitionError::Illegal { .. })
        ));
    }

    #[test]
    fn first_claim_wins_and_a_second_claim_is_rejected() {
        // Spec 17.4: first-wins; everyone else gets already_claimed.
        let mut e = offered();
        assert!(e.claim("ext-a", T0 + 10).is_ok());
        assert_eq!(
            e.claim("ext-b", T0 + 20),
            Err(TransitionError::AlreadyHandled)
        );
        assert_eq!(e.claimer_id.as_deref(), Some("ext-a"));
    }

    #[test]
    fn a_claim_after_the_deadline_is_rejected() {
        // Late claims must not be granted even if the row still says 'offered'.
        let mut e = offered();
        assert_eq!(
            e.claim("ext-a", T0 + 999),
            Err(TransitionError::AlreadyHandled)
        );
    }

    #[test]
    fn declined_and_expired_both_fall_back_to_desktop() {
        for terminal in [NotificationStatus::Declined, NotificationStatus::Expired] {
            let mut e = offered();
            e.transition(terminal, T0 + 1).expect("to decline/expire");
            assert!(e.fall_back_to_desktop(T0 + 2).is_ok());
            assert_eq!(e.owner_channel, Some(Channel::Desktop));
        }
    }

    #[test]
    fn terminal_states_never_change_again() {
        let mut e = offered();
        e.claim("ext", T0 + 1).expect("claim");
        e.mark_delivery_unknown("ack_timeout", T0 + 2)
            .expect("to unknown");
        assert!(e.status.is_terminal());
        assert_eq!(
            e.transition(NotificationStatus::DesktopPending, T0 + 2),
            Err(TransitionError::Terminal {
                from: NotificationStatus::DeliveryUnknown
            })
        );
    }

    #[test]
    fn delivery_unknown_does_not_fall_back_to_desktop() {
        // Spec 17.5: rather notify too little than notify twice.
        let mut e = offered();
        e.claim("ext", T0 + 10).expect("claim");
        e.mark_delivery_unknown("ack_timeout", T0 + 20)
            .expect("unknown");
        assert!(
            e.transition(NotificationStatus::DesktopPending, T0 + 30)
                .is_err(),
            "a possibly-displayed notification must never be shown again on desktop"
        );
    }

    #[test]
    fn crash_between_attempt_and_completion_must_not_redisplay() {
        // Spec 17.9: persist the attempt before showing anything.
        let mut e = OutboxEntry::new("n", "e", T0);
        e.transition(NotificationStatus::DesktopPending, T0)
            .expect("desktop");
        assert!(!e.must_not_redisplay_after_crash(), "nothing attempted yet");

        e.record_display_attempt(T0 + 5);
        assert!(
            e.must_not_redisplay_after_crash(),
            "attempt recorded, no completion"
        );

        e.complete(T0 + 10).expect("complete");
        assert!(!e.must_not_redisplay_after_crash(), "completed cleanly");
    }

    #[test]
    fn delivery_unknown_requires_a_claim_first() {
        // Spec 17.3 lists "claimed -> delivery_unknown" only. Going there from
        // 'offered' would mean claiming uncertainty about a display nobody was ever
        // granted permission to perform.
        let mut e = offered();
        assert!(matches!(
            e.mark_delivery_unknown("ack_timeout", T0 + 1),
            Err(TransitionError::Illegal { .. })
        ));
        assert_eq!(e.status, NotificationStatus::Offered);
    }

    #[test]
    fn extension_and_desktop_routes_never_both_display() {
        // The core anti-duplicate invariant, expressed as a reachable-states check.
        let mut e = OutboxEntry::new("n", "e", T0);
        e.transition(NotificationStatus::Offered, T0)
            .expect("offered");
        e.claim("ext", T0 + 1).expect("claim");
        e.acknowledge_display(T0 + 2).expect("displayed");
        e.complete(T0 + 3).expect("completed");

        assert_eq!(e.status, NotificationStatus::Completed);
        assert!(
            e.transition(NotificationStatus::DesktopPending, T0 + 4)
                .is_err(),
            "completed notifications must not be re-routed to desktop"
        );
    }

    #[test]
    fn freshness_uses_event_age_not_arrival_time() {
        // Spec 16.3.
        assert_eq!(freshness_action(0), FreshnessAction::FullNotification);
        assert_eq!(
            freshness_action(9 * 60 * 1000),
            FreshnessAction::FullNotification
        );
        assert_eq!(
            freshness_action(10 * 60 * 1000),
            FreshnessAction::FullNotification
        );
        assert_eq!(freshness_action(11 * 60 * 1000), FreshnessAction::Digested);
        assert_eq!(
            freshness_action(23 * 60 * 60 * 1000),
            FreshnessAction::Digested
        );
        assert_eq!(
            freshness_action(24 * 60 * 60 * 1000),
            FreshnessAction::Digested
        );
        assert_eq!(
            freshness_action(25 * 60 * 60 * 1000),
            FreshnessAction::Silent
        );
    }

    #[test]
    fn negative_age_from_clock_skew_counts_as_fresh() {
        assert_eq!(freshness_action(-5000), FreshnessAction::FullNotification);
    }

    #[test]
    fn merging_puts_the_highest_rarity_first() {
        let merged = merge_notification(vec![
            NotificationItem {
                id: "daily".into(),
                title: "Daily Rare".into(),
                rarity: Rarity::Rare,
                is_daily_rank: true,
            },
            NotificationItem {
                id: "ach".into(),
                title: "Epic Achievement".into(),
                rarity: Rarity::Epic,
                is_daily_rank: false,
            },
        ])
        .expect("non-empty");
        assert_eq!(merged.primary.id, "ach");
    }

    #[test]
    fn on_equal_rarity_an_achievement_leads_over_a_daily_rank() {
        // Spec 16.6 explicit rule.
        let merged = merge_notification(vec![
            NotificationItem {
                id: "daily".into(),
                title: "Daily".into(),
                rarity: Rarity::Rare,
                is_daily_rank: true,
            },
            NotificationItem {
                id: "ach".into(),
                title: "Achievement".into(),
                rarity: Rarity::Rare,
                is_daily_rank: false,
            },
        ])
        .expect("non-empty");
        assert_eq!(merged.primary.id, "ach");
    }

    #[test]
    fn merging_lists_at_most_three_extras_and_counts_the_rest() {
        let items: Vec<NotificationItem> = (0..7)
            .map(|i| NotificationItem {
                id: format!("i{i}"),
                title: format!("Item {i}"),
                rarity: Rarity::Common,
                is_daily_rank: false,
            })
            .collect();
        let merged = merge_notification(items).expect("non-empty");
        assert_eq!(merged.others.len(), 3);
        assert_eq!(
            merged.omitted, 3,
            "7 items = 1 primary + 3 listed + 3 omitted"
        );
    }

    #[test]
    fn merging_nothing_yields_nothing() {
        assert!(merge_notification(vec![]).is_none());
    }

    #[test]
    fn status_names_round_trip_to_the_wire_format() {
        // The persisted strings must stay stable; changing them needs a migration.
        for (status, name) in [
            (NotificationStatus::Created, "created"),
            (NotificationStatus::Offered, "offered"),
            (NotificationStatus::Claimed, "claimed"),
            (NotificationStatus::Displayed, "displayed"),
            (NotificationStatus::Declined, "declined"),
            (NotificationStatus::Expired, "expired"),
            (NotificationStatus::DesktopPending, "desktop_pending"),
            (NotificationStatus::DesktopDisplayed, "desktop_displayed"),
            (NotificationStatus::Completed, "completed"),
            (NotificationStatus::DeliveryUnknown, "delivery_unknown"),
            (NotificationStatus::Failed, "failed"),
            (NotificationStatus::Suppressed, "suppressed"),
        ] {
            assert_eq!(status.as_str(), name);
        }
    }

    #[test]
    fn exactly_four_states_are_terminal() {
        let terminal: Vec<&str> = [
            NotificationStatus::Created,
            NotificationStatus::Offered,
            NotificationStatus::Claimed,
            NotificationStatus::Displayed,
            NotificationStatus::Declined,
            NotificationStatus::Expired,
            NotificationStatus::DesktopPending,
            NotificationStatus::DesktopDisplayed,
            NotificationStatus::Completed,
            NotificationStatus::DeliveryUnknown,
            NotificationStatus::Failed,
            NotificationStatus::Suppressed,
        ]
        .into_iter()
        .filter(|s| s.is_terminal())
        .map(|s| s.as_str())
        .collect();
        assert_eq!(
            terminal,
            vec!["completed", "delivery_unknown", "failed", "suppressed"]
        );
    }

    fn item(id: &str, title: &str, rarity: Rarity, daily: bool) -> NotificationItem {
        NotificationItem {
            id: id.to_string(),
            title: title.to_string(),
            rarity,
            is_daily_rank: daily,
        }
    }

    #[test]
    fn a_plain_commit_produces_a_sound_and_no_popup() {
        // Spec 16.4: a normal commit gets a light acknowledgement, not a window.
        assert_eq!(
            present_event(vec![], true),
            Presentation::SoundOnly,
            "every commit must not interrupt the user"
        );
    }

    #[test]
    fn an_event_with_nothing_to_say_produces_nothing() {
        assert_eq!(present_event(vec![], false), Presentation::Nothing);
    }

    #[test]
    fn one_unlock_produces_one_popup() {
        let presentation = present_event(
            vec![item("first_commit", "First Commit", Rarity::Common, false)],
            true,
        );
        match presentation {
            Presentation::Popup(popup) => {
                assert_eq!(popup.title, "First Commit");
                assert!(popup.others.is_empty());
                assert_eq!(popup.omitted, 0);
            }
            other => panic!("expected a popup, got {other:?}"),
        }
    }

    #[test]
    fn an_unlock_wins_over_the_plain_commit_sound() {
        // A commit that also unlocks something must show the popup, not just chime.
        let presentation = present_event(
            vec![item("first_commit", "First Commit", Rarity::Common, false)],
            true,
        );
        assert!(matches!(presentation, Presentation::Popup(_)));
    }

    #[test]
    fn the_highest_rarity_leads_the_merged_popup() {
        // Spec 16.6: an Epic permanent achievement leads over a Rare daily rank.
        let presentation = present_event(
            vec![
                item("daily:rare", "Rare Day", Rarity::Rare, true),
                item("epic_thing", "Epic Thing", Rarity::Epic, false),
            ],
            true,
        );
        match presentation {
            Presentation::Popup(popup) => {
                assert_eq!(popup.title, "Epic Thing");
                assert_eq!(popup.rarity, Rarity::Epic);
                assert_eq!(popup.others, vec!["Rare Day".to_string()]);
            }
            other => panic!("expected a popup, got {other:?}"),
        }
    }

    #[test]
    fn more_than_three_items_are_summarised() {
        // Spec 16.6: at most three sub-lines, the rest become "+N".
        let items = (0..6)
            .map(|i| item(&format!("a{i}"), &format!("A{i}"), Rarity::Common, false))
            .collect();
        match present_event(items, false) {
            Presentation::Popup(popup) => {
                assert_eq!(popup.others.len(), 3);
                assert_eq!(
                    popup.omitted, 2,
                    "six items, one leads, three listed, two remain"
                );
            }
            other => panic!("expected a popup, got {other:?}"),
        }
    }

    #[test]
    fn a_digest_is_never_a_popup() {
        // Spec 16.3: the middle band is summarised. A window here is the reported bug,
        // so the assertion is about the variant, not just about a count.
        let presentation = digest_presentation(3, vec!["First Commit".to_string()]);
        assert!(
            !matches!(presentation, Presentation::Popup(_)),
            "a digested event must not open a window: {presentation:?}"
        );
        match presentation {
            Presentation::Digest(summary) => {
                assert_eq!(summary.events, 3);
                assert_eq!(summary.unlocks, vec!["First Commit".to_string()]);
                assert_eq!(summary.omitted_unlocks, 0);
            }
            other => panic!("expected a digest, got {other:?}"),
        }
    }

    #[test]
    fn a_digest_lists_at_most_three_titles_and_counts_the_rest() {
        let unlocks: Vec<String> = (0..5).map(|i| format!("Achievement {i}")).collect();
        match digest_presentation(5, unlocks) {
            Presentation::Digest(summary) => {
                assert_eq!(summary.unlocks.len(), 3);
                assert_eq!(summary.omitted_unlocks, 2);
                assert_eq!(
                    summary.unlock_count(),
                    5,
                    "the count must not shrink when titles are trimmed"
                );
            }
            other => panic!("expected a digest, got {other:?}"),
        }
    }

    #[test]
    fn merging_digests_keeps_one_line_for_a_whole_replay() {
        // A weekend replay queues one digest per event; the user must still see one.
        let mut merged = DigestSummary {
            events: 1,
            unlocks: vec!["First Commit".to_string()],
            omitted_unlocks: 0,
        };
        for _ in 0..49 {
            merged = merged.merged_with(DigestSummary {
                events: 1,
                unlocks: vec!["Epic Thing".to_string()],
                omitted_unlocks: 0,
            });
        }

        assert_eq!(merged.events, 50, "every event is counted");
        assert_eq!(merged.unlocks.len(), 3, "only three titles fit");
        assert_eq!(merged.unlock_count(), 50, "the rest are counted, not lost");
        assert!(
            merged.headline().contains("50"),
            "the headline must say how much happened, got {:?}",
            merged.headline()
        );
    }
}

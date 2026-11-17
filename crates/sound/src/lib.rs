//! Sound selection (spec 19).
//!
//! # What this crate decides, and what it does not
//!
//! It answers exactly one question: **given this notification, which sound plays?**
//! It does not open files, decode audio, or talk to the OS. That keeps the priority
//! rules testable without a sound card, which matters because the rules are the part
//! that is easy to get wrong.
//!
//! # Priority (spec 19)
//!
//! Achievement-specific > daily rank > rarity > global.
//!
//! A plain commit is explicitly NOT part of that chain: spec 19 says it "uses its own
//! policy", because it is an acknowledgement rather than a reward. Routing it through
//! the rarity chain would make every commit sound like an unlock.
//!
//! # Fallback (spec 19)
//!
//! A missing or unloadable asset must fall back safely. Falling back happens in two
//! steps: a custom sound that is not present drops to the slot default, and a slot
//! default that is not present drops to the compiled-in fallback. Every one of those
//! steps is reported in SoundSource, so a user hearing the wrong sound can be told
//! exactly why.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The reward sound shipped with the app.
///
/// One sound for the whole reward family (every rarity and every daily rank). Spec 19
/// allows per-category sounds, but Phase 2a ships built-in defaults only and the
/// product owner supplied a single achievement sound, so the tiers share it. Per-tier
/// files belong to the Sound Manager in Phase 4a, and the selection logic already
/// supports them: only this constant has to change.
pub const REWARD_SOUND: &str = "achievement";

/// The sound played when nothing else applies. Same asset as the reward sound, so an
/// unfilled slot still plays something the user recognises.
pub const FALLBACK_SOUND: &str = REWARD_SOUND;

/// The sound used for a plain commit (spec 16.4: a light acknowledgement).
///
/// Deliberately NOT the reward sound: a 0.8 s jingle on every commit would be exactly
/// the interruption that rule exists to prevent.
pub const COMMIT_SOUND: &str = "commit";

/// A rarity, mirrored here so this crate stays free of presentation dependencies.
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
    /// Legendary.
    Legendary,
}

/// A daily rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyRank {
    /// Common.
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Epic.
    Epic,
    /// Legendary.
    Legendary,
}

/// What is being announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundRequest {
    /// A plain commit: its own policy, never the rarity chain (spec 19).
    Commit,
    /// A reward popup.
    Reward {
        /// The leading reward's id, for an achievement-specific override.
        primary_id: String,
        /// The leading reward's rarity.
        rarity: Rarity,
        /// The leading reward's daily rank, when it is one.
        daily_rank: Option<DailyRank>,
    },
}

/// Why a particular sound was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundSource {
    /// The user set a sound for this exact achievement.
    AchievementSpecific,
    /// The user set a sound for this daily rank.
    DailyRankCustom,
    /// The user set a sound for this rarity.
    RarityCustom,
    /// The user set a global sound.
    GlobalCustom,
    /// The user set a sound for plain commits.
    CommitCustom,
    /// Built-in sound for this slot.
    BuiltIn,
    /// Both the custom and the slot sound were missing; the compiled-in fallback won.
    Fallback,
}

/// The outcome of selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSelection {
    /// The sound id to play.
    pub sound: String,
    /// Which rule produced it.
    pub source: SoundSource,
}

/// User overrides, keyed by slot.
///
/// Keys are strings because that is what a config file holds and what a settings UI
/// edits; typed keys would need conversion at both ends for no benefit. Recognised
/// keys are: global, commit, rarity:<name>, daily:<name>, achievement:<id>.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundOverrides {
    /// Slot key to sound id.
    pub slots: BTreeMap<String, String>,
}

impl SoundOverrides {
    /// An empty set of overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the global sound.
    pub fn set_global(&mut self, sound: impl Into<String>) {
        self.slots.insert("global".to_string(), sound.into());
    }

    /// Set the plain-commit sound.
    pub fn set_commit(&mut self, sound: impl Into<String>) {
        self.slots.insert("commit".to_string(), sound.into());
    }

    /// Set the sound for a rarity.
    pub fn set_rarity(&mut self, rarity: Rarity, sound: impl Into<String>) {
        self.slots
            .insert(format!("rarity:{}", rarity_key(rarity)), sound.into());
    }

    /// Set the sound for a daily rank.
    pub fn set_daily(&mut self, rank: DailyRank, sound: impl Into<String>) {
        self.slots
            .insert(format!("daily:{}", rank_key(rank)), sound.into());
    }

    /// Set the sound for one achievement.
    pub fn set_achievement(&mut self, id: impl Into<String>, sound: impl Into<String>) {
        self.slots
            .insert(format!("achievement:{}", id.into()), sound.into());
    }

    /// Look a slot up.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.slots.get(key).map(|s| s.as_str())
    }
}

fn rarity_key(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "common",
        Rarity::Uncommon => "uncommon",
        Rarity::Rare => "rare",
        Rarity::Epic => "epic",
        Rarity::Legendary => "legendary",
    }
}

fn rank_key(rank: DailyRank) -> &'static str {
    match rank {
        DailyRank::Common => "common",
        DailyRank::Uncommon => "uncommon",
        DailyRank::Rare => "rare",
        DailyRank::Epic => "epic",
        DailyRank::Legendary => "legendary",
    }
}

/// Choose the sound for a request.
///
/// The available set holds sound ids that actually resolved to a loadable file. An
/// override pointing at a deleted file therefore degrades instead of failing, which is
/// exactly what spec 19 asks for.
pub fn select(
    request: &SoundRequest,
    overrides: &SoundOverrides,
    available: &BTreeSet<String>,
) -> SoundSelection {
    match request {
        // Plain commits never enter the rarity chain (spec 19).
        SoundRequest::Commit => first_available(
            &[
                (
                    overrides.get("commit"),
                    Some(COMMIT_SOUND),
                    SoundSource::CommitCustom,
                ),
                (None, Some(FALLBACK_SOUND), SoundSource::Fallback),
            ],
            available,
        ),
        SoundRequest::Reward {
            primary_id,
            rarity,
            daily_rank,
        } => {
            let achievement_key = format!("achievement:{primary_id}");
            let rarity_slot = format!("rarity:{}", rarity_key(*rarity));

            let mut candidates: Vec<(Option<&str>, Option<&str>, SoundSource)> =
                Vec::with_capacity(4);
            // Achievement-specific sounds have no built-in: there is nothing sensible
            // to fall back to for one achievement in particular.
            candidates.push((
                overrides.get(&achievement_key),
                None,
                SoundSource::AchievementSpecific,
            ));
            if let Some(rank) = daily_rank {
                let key = format!("daily:{}", rank_key(*rank));
                candidates.push((
                    overrides.get(&key),
                    Some(REWARD_SOUND),
                    SoundSource::DailyRankCustom,
                ));
            }
            candidates.push((
                overrides.get(&rarity_slot),
                Some(REWARD_SOUND),
                SoundSource::RarityCustom,
            ));
            candidates.push((
                overrides.get("global"),
                Some(FALLBACK_SOUND),
                SoundSource::GlobalCustom,
            ));

            first_available(&candidates, available)
        }
    }
}

/// Walk the candidate list and return the first sound that resolves.
///
/// # Why a category's built-in beats a less specific category's custom sound
///
/// The candidates are ordered by spec 19's category priority, and each category
/// contributes its custom sound and then its built-in. That means a rarity's default
/// sound outranks a global custom sound.
///
/// This is a judgement call the spec does not spell out. The alternative -- letting
/// any custom sound win over any built-in -- would make a user who set one global
/// sound lose every rarity distinction, which defeats the point of having per-rarity
/// sounds at all. Category order is preserved instead.
fn first_available(
    candidates: &[(Option<&str>, Option<&str>, SoundSource)],
    available: &BTreeSet<String>,
) -> SoundSelection {
    for (custom, builtin, source) in candidates {
        if let Some(custom) = custom {
            if available.contains(*custom) {
                return SoundSelection {
                    sound: (*custom).to_string(),
                    source: *source,
                };
            }
            // The override exists but its file does not: degrade, do not fail. The
            // category's own built-in is tried before moving on.
        }
        if let Some(builtin) = builtin {
            if available.contains(*builtin) {
                return SoundSelection {
                    sound: (*builtin).to_string(),
                    source: SoundSource::BuiltIn,
                };
            }
        }
    }

    // Nothing resolved at all. Silence would hide a broken install, so we still return
    // an id and let the host decide; the source records why it is the fallback.
    SoundSelection {
        sound: FALLBACK_SOUND.to_string(),
        source: SoundSource::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything a normal install ships.
    fn all_builtins() -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        for s in [FALLBACK_SOUND, COMMIT_SOUND, REWARD_SOUND] {
            set.insert(s.to_string());
        }
        set
    }

    fn reward(id: &str, rarity: Rarity, daily: Option<DailyRank>) -> SoundRequest {
        SoundRequest::Reward {
            primary_id: id.to_string(),
            rarity,
            daily_rank: daily,
        }
    }

    #[test]
    fn a_reward_with_no_overrides_uses_the_reward_sound() {
        let selection = select(
            &reward("first_commit", Rarity::Rare, None),
            &SoundOverrides::new(),
            &all_builtins(),
        );
        assert_eq!(selection.sound, REWARD_SOUND);
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }

    #[test]
    fn an_achievement_override_beats_everything() {
        // Spec 19: achievement-specific > daily rank > rarity > global.
        let mut overrides = SoundOverrides::new();
        overrides.set_achievement("first_commit", "custom-ach");
        overrides.set_daily(DailyRank::Rare, "custom-daily");
        overrides.set_rarity(Rarity::Rare, "custom-rarity");
        overrides.set_global("custom-global");

        let mut available = all_builtins();
        for s in [
            "custom-ach",
            "custom-daily",
            "custom-rarity",
            "custom-global",
        ] {
            available.insert(s.to_string());
        }

        let selection = select(
            &reward("first_commit", Rarity::Rare, Some(DailyRank::Rare)),
            &overrides,
            &available,
        );
        assert_eq!(selection.sound, "custom-ach");
        assert_eq!(selection.source, SoundSource::AchievementSpecific);
    }

    #[test]
    fn a_daily_rank_override_beats_rarity_and_global() {
        let mut overrides = SoundOverrides::new();
        overrides.set_daily(DailyRank::Epic, "custom-daily");
        overrides.set_rarity(Rarity::Epic, "custom-rarity");
        overrides.set_global("custom-global");

        let mut available = all_builtins();
        for s in ["custom-daily", "custom-rarity", "custom-global"] {
            available.insert(s.to_string());
        }

        let selection = select(
            &reward("daily:epic", Rarity::Epic, Some(DailyRank::Epic)),
            &overrides,
            &available,
        );
        assert_eq!(selection.sound, "custom-daily");
        assert_eq!(selection.source, SoundSource::DailyRankCustom);
    }

    #[test]
    fn a_rarity_override_beats_global() {
        let mut overrides = SoundOverrides::new();
        overrides.set_rarity(Rarity::Epic, "custom-rarity");
        overrides.set_global("custom-global");

        let mut available = all_builtins();
        available.insert("custom-rarity".to_string());
        available.insert("custom-global".to_string());

        let selection = select(&reward("x", Rarity::Epic, None), &overrides, &available);
        assert_eq!(selection.sound, "custom-rarity");
        assert_eq!(selection.source, SoundSource::RarityCustom);
    }

    #[test]
    fn the_reward_sound_outranks_a_global_custom_sound() {
        // Documented judgement call: category order is preserved, so a user who set
        // only a global sound still hears the rarity distinction. The alternative
        // would make every rarity sound identical.
        let mut overrides = SoundOverrides::new();
        overrides.set_global("custom-global");
        let mut available = all_builtins();
        available.insert("custom-global".to_string());

        let selection = select(&reward("x", Rarity::Common, None), &overrides, &available);
        assert_eq!(selection.sound, REWARD_SOUND);
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }

    #[test]
    fn a_global_custom_is_used_when_nothing_more_specific_exists() {
        let mut overrides = SoundOverrides::new();
        overrides.set_global("custom-global");
        let mut available = BTreeSet::new();
        available.insert("custom-global".to_string());

        let selection = select(&reward("x", Rarity::Common, None), &overrides, &available);
        assert_eq!(selection.sound, "custom-global");
        assert_eq!(selection.source, SoundSource::GlobalCustom);
    }

    #[test]
    fn a_deleted_custom_file_falls_back_to_the_built_in() {
        // Spec 19: missing assets fall back safely instead of failing silently.
        let mut overrides = SoundOverrides::new();
        overrides.set_rarity(Rarity::Rare, "deleted-file");

        let selection = select(
            &reward("x", Rarity::Rare, None),
            &overrides,
            // note: deleted-file is NOT in the available set
            &all_builtins(),
        );
        assert_eq!(selection.sound, REWARD_SOUND);
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }

    #[test]
    fn when_even_the_built_in_is_missing_the_fallback_wins() {
        let empty = BTreeSet::new();
        let selection = select(
            &reward("x", Rarity::Epic, None),
            &SoundOverrides::new(),
            &empty,
        );
        assert_eq!(selection.sound, FALLBACK_SOUND);
        assert_eq!(selection.source, SoundSource::Fallback);
    }

    #[test]
    fn a_plain_commit_never_uses_the_rarity_chain() {
        // Spec 19: plain commits have their own policy. If this regressed, every
        // commit would sound like an unlock.
        let mut overrides = SoundOverrides::new();
        overrides.set_rarity(Rarity::Legendary, "custom-legendary");
        let mut available = all_builtins();
        available.insert("custom-legendary".to_string());

        let selection = select(&SoundRequest::Commit, &overrides, &available);
        assert_eq!(selection.sound, COMMIT_SOUND);
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }

    #[test]
    fn a_commit_override_is_used_when_present() {
        let mut overrides = SoundOverrides::new();
        overrides.set_commit("custom-commit");
        let mut available = all_builtins();
        available.insert("custom-commit".to_string());

        let selection = select(&SoundRequest::Commit, &overrides, &available);
        assert_eq!(selection.sound, "custom-commit");
        assert_eq!(selection.source, SoundSource::CommitCustom);
    }

    #[test]
    fn a_missing_commit_override_falls_back_to_the_commit_sound() {
        let mut overrides = SoundOverrides::new();
        overrides.set_commit("deleted-commit");
        let selection = select(&SoundRequest::Commit, &overrides, &all_builtins());
        assert_eq!(selection.sound, COMMIT_SOUND);
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }

    #[test]
    fn selection_is_deterministic() {
        let request = reward("x", Rarity::Epic, Some(DailyRank::Epic));
        let overrides = SoundOverrides::new();
        let available = all_builtins();
        let first = select(&request, &overrides, &available);
        for _ in 0..10 {
            assert_eq!(select(&request, &overrides, &available), first);
        }
    }

    #[test]
    fn exactly_one_sound_is_returned() {
        // Spec 16.6: the sound of the leading item is used, they are never stacked.
        // A single return value makes stacking structurally impossible.
        let mut overrides = SoundOverrides::new();
        overrides.set_achievement("a", "sa");
        overrides.set_achievement("b", "sb");
        let mut available = all_builtins();
        available.insert("sa".to_string());
        available.insert("sb".to_string());

        let selection = select(&reward("a", Rarity::Common, None), &overrides, &available);
        assert_eq!(selection.sound, "sa");
    }

    #[test]
    fn the_reward_family_shares_one_built_in() {
        // Documents the shipped policy: every rarity and daily rank resolves to the
        // same asset, because Phase 2a ships one user-supplied achievement sound.
        // The selection still routes category-specific CUSTOM sounds first, so adding
        // per-tier files later needs no change here.
        let builtins = all_builtins();
        for r in [Rarity::Common, Rarity::Epic, Rarity::Legendary] {
            let selection = select(&reward("x", r, None), &SoundOverrides::new(), &builtins);
            assert_eq!(selection.sound, REWARD_SOUND);
        }
        for rank in [DailyRank::Common, DailyRank::Legendary] {
            let selection = select(
                &reward("daily:x", Rarity::Common, Some(rank)),
                &SoundOverrides::new(),
                &builtins,
            );
            assert_eq!(selection.sound, REWARD_SOUND);
        }
    }

    #[test]
    fn overrides_survive_a_json_round_trip() {
        let mut overrides = SoundOverrides::new();
        overrides.set_global("g");
        overrides.set_commit("c");
        overrides.set_rarity(Rarity::Epic, "r");
        overrides.set_daily(DailyRank::Rare, "d");
        overrides.set_achievement("first_commit", "a");

        let json = serde_json::to_string(&overrides).expect("serialise");
        let back: SoundOverrides = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(overrides, back);
    }

    #[test]
    fn a_rarity_override_beats_its_builtin() {
        let mut overrides = SoundOverrides::new();
        overrides.set_rarity(Rarity::Uncommon, "commit");
        let mut available = BTreeSet::new();
        available.insert("achievement".to_string());
        available.insert("commit".to_string());

        let request = SoundRequest::Reward {
            primary_id: "uncommon_day".to_string(),
            rarity: Rarity::Uncommon,
            daily_rank: None,
        };
        let selection = select(&request, &overrides, &available);
        assert_eq!(selection.sound, "commit", "the override must win");
        assert_eq!(selection.source, SoundSource::RarityCustom);
    }

    #[test]
    fn an_override_pointing_at_a_missing_file_degrades_to_the_builtin() {
        // Spec 19: a deleted custom sound must not silence the achievement.
        let mut overrides = SoundOverrides::new();
        overrides.set_rarity(Rarity::Uncommon, "deleted");
        let mut available = BTreeSet::new();
        available.insert("achievement".to_string());

        let request = SoundRequest::Reward {
            primary_id: "x".to_string(),
            rarity: Rarity::Uncommon,
            daily_rank: None,
        };
        let selection = select(&request, &overrides, &available);
        assert_eq!(selection.sound, "achievement");
        assert_eq!(selection.source, SoundSource::BuiltIn);
    }
}

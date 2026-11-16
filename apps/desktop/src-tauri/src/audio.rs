//! Sound playback (spec 19: Phase 2a is built-in defaults plus basic playback).
//!
//! # Why this is thin
//!
//! Selecting a sound is real logic and lives in the devchievements-sound crate, where
//! it is tested without a sound card. This module only maps a sound id to a file and
//! asks Windows to play it.
//!
//! # Failure policy
//!
//! Audio is a side effect of a notification that has already been committed. A missing
//! file or a muted device must never disturb ingestion, so every path returns and the
//! caller logs.

use std::path::{Path, PathBuf};

use devchievements_sound::{select, SoundOverrides, SoundRequest, SoundSelection};

/// Where the built-in sounds live, next to the executable.
fn sounds_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sounds")
}

/// The ids that actually resolved to a file on disk.
fn available() -> std::collections::BTreeSet<String> {
    let dir = sounds_dir();
    let mut set = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return set;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("wav") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                set.insert(stem.to_string());
            }
        }
    }
    set
}

/// The sound ids that resolved to a file, for the diagnostics view.
pub fn available_sound_ids() -> Vec<String> {
    available().into_iter().collect()
}

/// Map the notification's leading reward onto a sound request.
///
/// The daily-rank variant is recognised from the id the delivery layer assigns
/// ("daily:<rank>"), which is the same convention used for merging (spec 16.6).
pub fn request_for(popup_id: &str, rarity: devchievements_sound::Rarity) -> SoundRequest {
    if let Some(rank) = popup_id.strip_prefix("daily:") {
        return SoundRequest::Reward {
            primary_id: popup_id.to_string(),
            rarity,
            daily_rank: rank_from(rank),
        };
    }
    SoundRequest::Reward {
        primary_id: popup_id.to_string(),
        rarity,
        daily_rank: None,
    }
}

fn rank_from(name: &str) -> Option<devchievements_sound::DailyRank> {
    use devchievements_sound::DailyRank;
    match name {
        "common" => Some(DailyRank::Common),
        "uncommon" => Some(DailyRank::Uncommon),
        "rare" => Some(DailyRank::Rare),
        "epic" => Some(DailyRank::Epic),
        "legendary" => Some(DailyRank::Legendary),
        _ => None,
    }
}

/// Choose a sound without playing it. Exposed for diagnostics and tests.
pub fn choose(request: &SoundRequest) -> SoundSelection {
    select(request, &SoundOverrides::new(), &available())
}

/// Play the sound chosen for a request. Returns the selection so the caller can log it.
pub fn play_for(request: &SoundRequest) -> SoundSelection {
    let selection = choose(request);
    let file = sounds_dir().join(format!("{}.wav", selection.sound));
    play_file(&file);
    selection
}

#[cfg(windows)]
fn play_file(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

    if !path.exists() {
        return;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // Asynchronous so a notification never delays the drain loop.
    let ok = unsafe {
        PlaySoundW(
            windows::core::PCWSTR(wide.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        )
    };
    if !ok.as_bool() {
        // Not fatal: the device may be muted or absent.
        eprintln!("sound: playback failed for {}", path.display());
    }
}

#[cfg(not(windows))]
fn play_file(_path: &Path) {
    // No playback layer outside Windows for now.
}

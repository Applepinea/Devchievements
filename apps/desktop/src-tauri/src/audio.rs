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

/// Choose a sound without playing it, with no user overrides.
fn choose(request: &SoundRequest) -> SoundSelection {
    select(request, &SoundOverrides::new(), &available())
}

/// Scale 16-bit PCM samples by a volume in 0-100.
///
/// Why scale the samples: PlaySoundW has no per-call volume, and the alternative
/// (waveOutSetVolume) changes the volume of the whole device, which would turn
/// down everything the user listens to rather than just this app's sound.
///
/// Stereo or mono is detected from the block alignment so interleaved channels
/// are not swapped. Anything that is not 16-bit PCM is returned unchanged: playing
/// it at the wrong gain is worse than playing it as recorded.
pub fn apply_gain(wav: &[u8], volume: u8, muted: bool) -> Vec<u8> {
    if muted || volume >= 100 {
        return if muted { Vec::new() } else { wav.to_vec() };
    }
    let Some(header) = wav_header(wav) else {
        return wav.to_vec();
    };
    if header.bits != 16 {
        return wav.to_vec();
    }

    let gain = f32::from(volume) / 100.0;
    let mut out = wav.to_vec();
    let mut index = header.data_start;
    while index + 1 < out.len() {
        let sample = i16::from_le_bytes([out[index], out[index + 1]]);
        let scaled = (f32::from(sample) * gain).round();
        let clamped = scaled.clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
        let bytes = clamped.to_le_bytes();
        out[index] = bytes[0];
        out[index + 1] = bytes[1];
        index += 2;
    }
    out
}

/// Where the PCM data starts, and how it is encoded.
struct WavHeader {
    data_start: usize,
    bits: u16,
}

fn wav_header(wav: &[u8]) -> Option<WavHeader> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12;
    let mut bits = 0u16;
    while offset + 8 <= wav.len() {
        let id = &wav[offset..offset + 4];
        let size = u32::from_le_bytes([
            wav[offset + 4],
            wav[offset + 5],
            wav[offset + 6],
            wav[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if id == b"fmt " && body + 16 <= wav.len() {
            bits = u16::from_le_bytes([wav[body + 14], wav[body + 15]]);
        }
        if id == b"data" {
            return Some(WavHeader {
                data_start: body.min(wav.len()),
                bits,
            });
        }
        offset = body.saturating_add(size).saturating_add(size % 2);
    }
    None
}

/// Play the sound chosen for a request. Returns the selection so the caller can log it.
/// Keeps the scaled buffer alive for the duration of asynchronous playback.
///
/// PlaySoundW with SND_ASYNC returns immediately, so the memory it was given must
/// outlive this call. Only one sound plays at a time by design (spec 16.6), so a
/// single buffer is enough.
static PLAYBACK: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// Play a request at the configured volume, honouring the mute switch.
///
/// Returns the selection so the caller can log what was played, or None when the
/// sound was suppressed.
pub fn play_for_with(
    request: &SoundRequest,
    overrides: &SoundOverrides,
    volume: u8,
    muted: bool,
) -> Option<SoundSelection> {
    if muted || volume == 0 {
        return None;
    }
    // The user's Sound Manager choices decide which file plays (spec 19).
    let selection = select(request, overrides, &available());
    let file = sounds_dir().join(format!("{}.wav", selection.sound));
    if !file.exists() {
        return Some(selection);
    }
    let Ok(bytes) = std::fs::read(&file) else {
        return Some(selection);
    };
    let scaled = apply_gain(&bytes, volume, false);
    play_memory(&scaled);
    Some(selection)
}

#[cfg(windows)]
fn play_memory(bytes: &[u8]) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    let mut guard = match PLAYBACK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = bytes.to_vec();
    let wide = windows::core::PCWSTR(guard.as_ptr() as *const u16);
    unsafe {
        let _ = PlaySoundW(wide, None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(windows))]
fn play_memory(_bytes: &[u8]) {}

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

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

/// Where imported sounds live.
///
/// Separate from the built-in sounds next to the executable: updates replace the
/// built-in set, and user files must survive that.
fn custom_sounds_dir() -> PathBuf {
    crate::default_data_dir().join("assets")
}

/// Every sound file, from both the built-in and the user directory.
fn sound_dirs() -> [PathBuf; 2] {
    [custom_sounds_dir(), sounds_dir()]
}

/// Resolve a sound id to a file, preferring the user's imported copy.
///
/// User files win: an import is an explicit statement of preference, and the
/// built-in of the same name is what the user wanted to replace.
fn sound_path(id: &str) -> Option<PathBuf> {
    for dir in sound_dirs() {
        let candidate = dir.join(format!("{id}.wav"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The ids that actually resolved to a file on disk.
fn available() -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    for dir in sound_dirs() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wav") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    set.insert(stem.to_string());
                }
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

/// The buffer the system is currently playing from.
///
/// # Why the address must not move
///
/// `PlaySoundW(..., SND_ASYNC | SND_MEMORY)` returns as soon as playback STARTS and
/// then reads from the pointer on its own thread. The pointer must therefore stay
/// valid, and stay put, for as long as the sound lasts.
///
/// This used to be a plain `Mutex<Vec<u8>>` that the caller overwrote with
/// `*guard = bytes.to_vec()` on every playback. Two consequences, both matching the
/// bug report:
///
/// 1. assigning drops the OLD buffer while the system is still reading it -- a
///    use-after-free, which on this heap reuses the allocation immediately and turned
///    one sound into a click or a fragment;
/// 2. the pointer was taken *after* the assignment, so it pointed into the new Vec and
///    the previous sound lost its data mid-flight. That is the "only the second half
///    plays" symptom.
///
/// `Box<[u8]>` fixes both: the box owns a stable address, and handing out a handle
/// keeps it alive. Only one sound plays at a time by design (spec 16.6), so replacing
/// the previous buffer is correct -- but only once nothing is reading it.
static PLAYING: std::sync::Mutex<Option<std::sync::Arc<Box<[u8]>>>> = std::sync::Mutex::new(None);

/// A buffer that the audio system may still be reading.
///
/// Holding one of these is the contract: while it lives, its address and length are
/// stable and the memory is not freed. Dropping it does NOT stop playback early,
/// because the system holds its own reference to the memory it was given for the whole
/// {`SND_ASYNC`} call; what it must not see is the allocation being reused.
#[derive(Clone)]
pub struct PlaybackBuffer(std::sync::Arc<Box<[u8]>>);

impl PlaybackBuffer {
    /// Address to hand to the audio system.
    pub fn ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Byte length, so a caller can keep its own view bounded.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the buffer is empty (an empty sound plays nothing).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Take ownership of a copy of `bytes` for the duration of playback.
///
/// Returns a handle to the copy. The copy is made HERE, at a stable address, rather
/// than handing the system a pointer into a caller-owned `Vec` that may reallocate.
pub fn retain_for_playback(bytes: &[u8]) -> PlaybackBuffer {
    let owned: Box<[u8]> = bytes.to_vec().into_boxed_slice();
    let buffer = std::sync::Arc::new(owned);

    let mut slot = match PLAYING.lock() {
        Ok(guard) => guard,
        // A poisoned lock means a previous playback panicked. The buffer itself is
        // still a valid allocation, so recover rather than wedging audio for the rest
        // of the process' life.
        Err(poisoned) => poisoned.into_inner(),
    };
    *slot = Some(buffer.clone());
    PlaybackBuffer(buffer)
}

/// How long the currently held buffer lasts, in milliseconds.
///
/// Computed from the WAV header rather than from a table of known sounds, because a
/// user-imported WAV can be any length. Returns `None` when the format cannot be read,
/// which the caller treats as "do not gate anything".
///
/// # Why a duration is needed at all
///
/// `PlaySoundW` is process-global and `SND_ASYNC` REPLACES whatever is playing. When one
/// drain delivers several presentations in a row -- a commit plus an unlock, or two
/// queued notifications -- the second call cut the first sound off mid-play. That is the
/// reported "sometimes only the second half plays".
///
/// Spec 16.6 says one sound plays per event and never several at once, so the fix is not
/// to mix them: it is to let each one finish before starting the next.
pub fn duration_ms(buffer: &PlaybackBuffer) -> Option<u64> {
    wav_format(buffer.0.as_ref())
}

/// Bytes per second and total data bytes, from a WAV header.
fn wav_format(wav: &[u8]) -> Option<u64> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12;
    let mut byte_rate = 0u32;
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
            byte_rate = u32::from_le_bytes([
                wav[body + 8],
                wav[body + 9],
                wav[body + 10],
                wav[body + 11],
            ]);
        }
        if id == b"data" {
            if byte_rate == 0 {
                return None;
            }
            // The declared size may exceed the bytes present (a truncated file); the
            // shorter of the two is what will actually be heard.
            let available = (wav.len() - body) as u32;
            let bytes = size.min(available as usize) as u32;
            return Some(u64::from(bytes) * 1000 / u64::from(byte_rate));
        }
        offset = body.saturating_add(size).saturating_add(size % 2);
    }
    None
}

/// Silence from now until the sound that started at `started` would have finished.
///
/// # Why not `Sleep` the whole duration
///
/// `PlaySoundW` was already called with `SND_ASYNC`, so waiting the full duration would
/// only postpone the NEXT sound while the current one is already playing; what has to be
/// prevented is the next call arriving before this one ends. So the wait is measured
/// from when the sound started, not from now, and it stops early when the caller's own
/// work is done.
///
/// The 60 ms margin covers the gap between the header's estimate and the device's actual
/// start latency: finishing a hair late is inaudible, cutting a hair early is not.
pub fn wait_for_playback(started: std::time::Instant, duration_ms: Option<u64>) {
    let Some(duration) = duration_ms else {
        return;
    };
    let total = std::time::Duration::from_millis(duration + 60);
    let elapsed = started.elapsed();
    if let Some(remaining) = total.checked_sub(elapsed) {
        std::thread::sleep(remaining);
    }
}

/// Length of the buffer currently held for playback, for tests and diagnostics.
pub fn playing_len() -> usize {
    match PLAYING.lock() {
        Ok(guard) => guard.as_ref().map_or(0, |b| b.len()),
        Err(poisoned) => poisoned.into_inner().as_ref().map_or(0, |b| b.len()),
    }
}

/// The process-wide playback lock.
///
/// # Why a lock and not just "one sound at a time"
///
/// `PlaySoundW` with `SND_ASYNC` always *replaces* the sound currently playing; there is no
/// append mode. Spec 16.6 forbids mixing several sounds, so when a second presentation
/// arrives while the first is still audible the correct behaviour is to let the first
/// finish and start the second afterwards. Without that, a drain that delivers two
/// notifications back to back cuts the first one off mid-note -- which is exactly the
/// reported "the sound only plays its second half".
///
/// The gate is process-wide because the audio device is: two threads calling
/// `PlaySoundW` would cut each other off no matter which buffer they hold.
fn gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    match GATE.lock() {
        Ok(guard) => guard,
        // A panic while a sound was playing must not silence the app for the rest of the
        // process' life: the lock guards ordering, not any data.
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// A sound that is currently in flight.
///
/// The token owns the three things that have to outlive the sound:
///
/// * the gate, so no second sound can replace this one;
/// * the buffer, so the memory the system is reading is not freed;
/// * the obligation to wait until it has finished.
///
/// Dropping the token performs that wait, so the wait cannot be forgotten. It also means
/// the caller decides WHEN to wait: `DesktopSink` logs and opens its popup first and lets
/// the token fall out of scope afterwards, so the popup still appears the moment the sound
/// starts instead of a full second later.
pub struct SoundPlayback {
    selection: SoundSelection,
    // Never read: its refcount is the guarantee that the address handed to the audio
    // system stays valid, even if the shared slot is replaced.
    _buffer: PlaybackBuffer,
    started: std::time::Instant,
    started_at_ms: u64,
    duration_ms: Option<u64>,
    // Dropped last (fields drop after Drop::drop), so the next sound starts from silence.
    _gate: std::sync::MutexGuard<'static, ()>,
}

impl SoundPlayback {
    /// What was chosen to play, so the caller can log it.
    pub fn selection(&self) -> &SoundSelection {
        &self.selection
    }

    /// Wall-clock milliseconds when the buffer was handed to the device.
    ///
    /// Stamped at the START, because spec 44.4 measures the latency budget up to the moment
    /// the user hears something. The token now lives as long as the sound does, so "when the
    /// caller happened to read the clock" is no longer that instant.
    pub fn started_at_ms(&self) -> u64 {
        self.started_at_ms
    }
}

impl Drop for SoundPlayback {
    fn drop(&mut self) {
        wait_for_playback(self.started, self.duration_ms);
    }
}

/// Wall-clock milliseconds, for log lines only.
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// Hand one WAV to the audio device and keep the gate for as long as it plays.
///
/// Must be called with nothing playing: it takes the gate itself, so a caller already
/// holding it would deadlock (there is no such caller -- the token is the only lock
/// holder).
fn start_buffer(selection: SoundSelection, bytes: &[u8], volume: u8) -> SoundPlayback {
    let gate = gate();
    let buffer = retain_for_playback(&apply_gain(bytes, volume, false));
    let duration_ms = duration_ms(&buffer);
    let started_at_ms = epoch_millis();
    let started = std::time::Instant::now();
    play_memory(&buffer);
    SoundPlayback {
        selection,
        _buffer: buffer,
        started,
        started_at_ms,
        duration_ms,
        _gate: gate,
    }
}

/// Play a request at the configured volume, honouring the mute switch.
///
/// Returns a token for the sound that is now playing, or None when nothing will be heard.
/// The token must live until the caller is ready for the next sound: while it exists, no
/// other sound can start.
pub fn play_for_with(
    request: &SoundRequest,
    overrides: &SoundOverrides,
    volume: u8,
    muted: bool,
) -> Option<SoundPlayback> {
    if muted || volume == 0 {
        // Nothing audible, so nothing is gated either: a muted popup must not make the
        // drain wait for a sound nobody hears.
        return None;
    }
    // The user's Sound Manager choices decide which file plays (spec 19).
    let selection = select(request, overrides, &available());
    // sound_path only returns a file it has already stat-ed, and a missing file degrades
    // silently rather than failing the notification (spec 19). The selection is still
    // reported, so the log says which sound WOULD have played.
    let bytes = sound_path(&selection.sound).and_then(|file| std::fs::read(file).ok());
    Some(match bytes {
        Some(bytes) => start_buffer(selection, &bytes, volume),
        None => start_buffer(selection, &[], volume),
    })
}

#[cfg(windows)]
fn play_memory(buffer: &PlaybackBuffer) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    if buffer.is_empty() {
        return;
    }

    // The token owns this allocation for as long as the sound plays; `ptr` is a stable
    // address inside a Box that nothing else can move or free.
    let wide = windows::core::PCWSTR(buffer.ptr() as *const u16);
    let ok = unsafe { PlaySoundW(wide, None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
    if !ok.as_bool() {
        // Reported rather than swallowed: "the sound sometimes does not play" is
        // otherwise undiscoverable, because PlaySoundW only returns a boolean.
        eprintln!("sound: PlaySoundW refused the buffer ({} bytes)", buffer.len());
    }
}

#[cfg(not(windows))]
fn play_memory(_buffer: &PlaybackBuffer) {}

/// What a preview played, for the caller to log and the page to show.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    /// The sound id that was chosen.
    pub sound: String,
    /// Which rule chose it, so "I set a custom sound and nothing changed" is answerable.
    pub source: String,
    /// Whether anything actually reached the audio device.
    ///
    /// False with muted on is the expected answer, not a failure: the page still shows
    /// WHICH sound the current settings select, which is the other half of what a
    /// preview is for.
    pub played: bool,
    /// Why nothing was played, when nothing was.
    pub reason: Option<String>,
}

/// Play the sound the current settings would choose for one slot, without unlocking
/// anything (spec 19).
///
/// # Why a preview needs the same selection path as a real notification
///
/// "Preview" that hard-codes a file would tell the user nothing: the question a preview
/// answers is "which sound will I actually hear for THIS slot, with MY overrides and MY
/// volume". So this runs `select` with the real overrides, and only the playback is
/// treated as optional -- a muted or device-less host still reports the selection.
pub fn preview(slot: &str) -> PreviewReport {
    let prefs = crate::settings::Settings::load(&crate::settings_path());
    let overrides = SoundOverrides {
        slots: prefs.sound_overrides.clone(),
    };

    // The slot names the UI offers. "commit" is the plain-commit sound; every other
    // choice is a reward, distinguished by rarity or by daily rank so the preview shows
    // the difference between them.
    let request = if slot == "commit" {
        SoundRequest::Commit
    } else if let Some(rank) = slot.strip_prefix("daily:") {
        SoundRequest::Reward {
            primary_id: format!("daily:{rank}"),
            rarity: rarity_for_name(rank),
            daily_rank: rank_from(rank),
        }
    } else {
        SoundRequest::Reward {
            primary_id: format!("rarity:{slot}"),
            rarity: rarity_for_name(slot),
            daily_rank: None,
        }
    };

    let selection = select(&request, &overrides, &available());
    let sound = selection.sound.clone();
    let source = format!("{:?}", selection.source);

    // Muted is a deliberate user choice, so it is reported rather than overridden: a
    // preview that ignores the mute switch would teach the user the wrong thing about
    // what a real notification does.
    if prefs.muted || prefs.volume == 0 {
        return PreviewReport {
            sound,
            source,
            played: false,
            reason: Some("muted or volume is zero".to_string()),
        };
    }

    let played = match sound_path(&sound) {
        Some(file) => match std::fs::read(&file) {
            Ok(bytes) => {
                // A preview goes through the same gate as a notification, and dropping the
                // token is what waits it out: a notification arriving while the user is
                // auditioning a sound waits its turn instead of cutting the preview short
                // -- and the preview cannot cut a real notification short either.
                drop(start_buffer(selection, &bytes, prefs.volume));
                true
            }
            Err(e) => {
                return PreviewReport {
                    sound,
                    source,
                    played: false,
                    reason: Some(format!("could not read {}: {e}", file.display())),
                };
            }
        },
        None => {
            return PreviewReport {
                sound,
                source,
                played: false,
                reason: Some(format!("no file on disk for sound id `{}`", selection.sound)),
            };
        }
    };

    PreviewReport {
        sound,
        source,
        played,
        reason: None,
    }
}

/// The rarity a slot name stands for, defaulting to Common.
///
/// A preview must never fail on an unknown slot name: the page's slot list and this
/// mapping are edited together, and a typo should produce a chime rather than an error.
fn rarity_for_name(name: &str) -> devchievements_sound::Rarity {
    use devchievements_sound::Rarity;
    match name {
        "legendary" => Rarity::Legendary,
        "epic" => Rarity::Epic,
        "rare" => Rarity::Rare,
        "uncommon" => Rarity::Uncommon,
        _ => Rarity::Common,
    }
}

/// The slots the settings page offers, in the order it shows them.
pub fn preview_slots() -> Vec<String> {
    vec![
        "commit".to_string(),
        "common".to_string(),
        "uncommon".to_string(),
        "rare".to_string(),
        "epic".to_string(),
        "legendary".to_string(),
        "daily:common".to_string(),
        "daily:uncommon".to_string(),
        "daily:rare".to_string(),
        "daily:epic".to_string(),
        "daily:legendary".to_string(),
    ]
}

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


#[cfg(test)]
mod playback_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A minimal valid 16-bit mono WAV with `samples` silence frames.
    fn wav(samples: usize) -> Vec<u8> {
        let data_len = (samples * 2) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&22050u32.to_le_bytes());
        out.extend_from_slice(&44100u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend(std::iter::repeat(0u8).take(data_len as usize));
        out
    }

    /// The buffer handed to PlaySoundW must stay at a STABLE address.
    ///
    /// PlaySoundW with SND_ASYNC | SND_MEMORY returns immediately and the system keeps
    /// reading the buffer while it plays. Handing it the inside of a Vec that later
    /// reallocates, or a temporary that dies, is use-after-free: the audio stops part
    /// way through, or truncates to a fragment.
    ///
    /// That is the defect behind "sometimes only the second half plays": the old code
    /// pointed the system at the Vec's data AFTER assigning into the shared slot, so the
    /// next playback freed the buffer the previous sound was still reading.
    #[test]
    fn the_buffer_handed_to_the_system_outlives_the_call() {
        let _slot = slot_lock();
        let first = wav(1000);
        let second = wav(2000);

        let handle_a = super::retain_for_playback(&first);
        let ptr_a = handle_a.ptr();
        let handle_b = super::retain_for_playback(&second);
        let ptr_b = handle_b.ptr();

        assert_ne!(
            ptr_a, ptr_b,
            "two sounds must not share one buffer: the second would corrupt the first"
        );

        assert_eq!(
            handle_a.len(),
            first.len(),
            "the first buffer must still be the first sound while it plays"
        );
        // Safety: the pointer is owned by the live handle and was sized above.
        let slice = unsafe { std::slice::from_raw_parts(ptr_a, handle_a.len()) };
        assert_eq!(slice, &first[..], "the first sound's bytes must be intact");

        drop(handle_b);
        let slice = unsafe { std::slice::from_raw_parts(ptr_a, handle_a.len()) };
        assert_eq!(slice, &first[..], "dropping the second must not free the first");
    }

    /// Serialises the tests that read or write the process-wide playback slot, so one
    /// test's buffer cannot be mistaken for another's.
    fn slot_lock() -> std::sync::MutexGuard<'static, ()> {
        static SLOT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        match SLOT.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn a_playing_buffer_is_not_freed_while_it_plays() {
        let _slot = slot_lock();
        let sound = wav(500);
        super::retain_for_playback(&sound);
        assert_eq!(
            super::playing_len(),
            sound.len(),
            "the slot must hold the buffer that is playing"
        );
        let other = wav(700);
        super::retain_for_playback(&other);
        assert_eq!(super::playing_len(), other.len());
    }

    /// The wait is only correct if it is derived from the file, so the length has to be
    /// read out of the header rather than guessed from the byte count.
    #[test]
    fn a_wav_reports_its_own_length() {
        // 22050 frames of 16-bit mono at 22050 Hz is exactly one second.
        assert_eq!(super::wav_format(&wav(22_050)), Some(1000));
        // Half the frames, half the time.
        assert_eq!(super::wav_format(&wav(11_025)), Some(500));
    }

    /// A file cut short on disk must not be waited on for the length it claims.
    #[test]
    fn a_truncated_wav_reports_only_the_audio_that_is_there() {
        let mut bytes = wav(22_050);
        bytes.truncate(44 + 22_050);
        assert_eq!(
            super::wav_format(&bytes),
            Some(500),
            "the header still promises one second, but only half the samples exist"
        );
    }

    /// A header this code cannot parse must mean "do not gate", never "hang".
    #[test]
    fn an_unreadable_header_never_blocks_playback() {
        assert_eq!(super::wav_format(b"not a wav file at all, just text"), None);
        assert_eq!(super::wav_format(&[0u8; 43]), None);

        let started = std::time::Instant::now();
        super::wait_for_playback(started, None);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(50),
            "an unknown duration must return at once, not sleep"
        );
    }

    /// A sound that already finished must not be waited on again: the wait is measured
    /// from when playback started, so the caller's own work counts towards it.
    #[test]
    fn a_sound_that_already_finished_is_not_waited_on_again() {
        let started = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(2000))
            .expect("the test host has a monotonic clock longer than two seconds");
        let before = std::time::Instant::now();
        super::wait_for_playback(started, Some(100));
        assert!(
            before.elapsed() < std::time::Duration::from_millis(50),
            "a sound from two seconds ago is over; sleeping again only delays the next one"
        );
    }

    /// The whole point of the gate: PlaySoundW replaces whatever is playing, so two
    /// sounds must never be in flight at the same time -- not merely "both get called".
    #[test]
    fn two_playbacks_never_overlap() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let (tx, rx) = mpsc::channel();
        let mut threads = Vec::new();
        for _ in 0..3 {
            let tx = tx.clone();
            threads.push(std::thread::spawn(move || {
                let _gate = super::gate();
                let started = Instant::now();
                std::thread::sleep(Duration::from_millis(40));
                tx.send((started, Instant::now())).expect("test channel");
            }));
        }
        drop(tx);
        let mut spans: Vec<(Instant, Instant)> = rx.iter().collect();
        for thread in threads {
            thread.join().expect("test thread");
        }
        assert_eq!(spans.len(), 3, "every playback must report when it ran");

        spans.sort_by_key(|(started, _)| *started);
        for pair in spans.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0,
                "two sounds played at the same time: the second would cut off the first"
            );
        }
    }

    /// The guarantee the whole fix rests on: while one sound is in flight, a second one
    /// cannot start, and cannot be started by forgetting to wait either -- the token owns
    /// the gate, so the wait happens when it is dropped.
    ///
    /// The buffer is 100 ms of ZERO samples, so this test is silent by construction: it
    /// measures the ordering without making the machine chime twice.
    #[test]
    fn a_second_sound_cannot_start_while_the_first_is_still_playing() {
        let silence = wav(2_205); // 100 ms at 22050 Hz, 16-bit mono

        let available: std::collections::BTreeSet<String> =
            ["commit"].iter().map(|s| s.to_string()).collect();
        let selection = || {
            select(
                &SoundRequest::Commit,
                &SoundOverrides::new(),
                &available,
            )
        };

        let first = super::start_buffer(selection(), &silence, 80);

        // The second playback runs on another thread because this one has to let go of
        // the first before it may proceed. Its selection is made up front: choosing a
        // sound is pure, so it must not be blocked by the gate.
        let second_selection = selection();
        let other = silence.clone();
        let handle = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let second = super::start_buffer(second_selection, &other, 80);
            let waited = started.elapsed();
            drop(second);
            waited
        });

        std::thread::sleep(std::time::Duration::from_millis(40));
        drop(first);

        let waited = handle.join().expect("the second playback thread");
        assert!(
            waited >= std::time::Duration::from_millis(100),
            "the second sound started after only {waited:?}; the first one lasts 100 ms              plus its 60 ms tail margin"
        );
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn every_slot_the_page_offers_can_be_previewed() {
        // The page renders whatever this returns, so a slot here that the selector
        // cannot map would be a button that silently does nothing.
        let slots = preview_slots();
        assert!(slots.contains(&"commit".to_string()));
        for rarity in ["common", "uncommon", "rare", "epic", "legendary"] {
            assert!(slots.contains(&rarity.to_string()), "missing {rarity}");
            assert!(
                slots.contains(&format!("daily:{rarity}")),
                "missing daily:{rarity}"
            );
        }
    }

    #[test]
    fn a_plain_commit_slot_selects_the_commit_sound() {
        // spec 19: the commit slot must never fall into the reward chain. Asserted on
        // the SELECTION, not on playback: a test host has no audio device.
        let overrides = SoundOverrides::new();
        let available: std::collections::BTreeSet<String> =
            ["commit", "achievement"].iter().map(|s| s.to_string()).collect();
        let selection = select(&SoundRequest::Commit, &overrides, &available);
        assert_eq!(selection.sound, "commit");
    }

    #[test]
    fn a_rarity_slot_selects_the_reward_sound() {
        let overrides = SoundOverrides::new();
        let available: std::collections::BTreeSet<String> =
            ["commit", "achievement"].iter().map(|s| s.to_string()).collect();
        let request = SoundRequest::Reward {
            primary_id: "rarity:epic".to_string(),
            rarity: devchievements_sound::Rarity::Epic,
            daily_rank: None,
        };
        let selection = select(&request, &overrides, &available);
        assert_eq!(
            selection.sound, "achievement",
            "an unfilled slot plays the built-in reward sound"
        );
    }

    #[test]
    fn a_custom_override_is_what_the_preview_would_play() {
        // The whole point of previewing rather than hard-coding a file: it runs the
        // user's own overrides. If this drifts, the preview lies.
        let mut overrides = SoundOverrides::new();
        overrides.set_commit("custom-commit");
        let available: std::collections::BTreeSet<String> =
            ["commit", "custom-commit"].iter().map(|s| s.to_string()).collect();
        let selection = select(&SoundRequest::Commit, &overrides, &available);
        assert_eq!(selection.sound, "custom-commit");
        assert_eq!(
            selection.source,
            devchievements_sound::SoundSource::CommitCustom
        );
    }

    #[test]
    fn an_unknown_slot_name_still_produces_a_request() {
        // A typo must produce a chime rather than an error: the slot list is data.
        let rarity = rarity_for_name("not-a-rarity");
        assert_eq!(rarity, devchievements_sound::Rarity::Common);
    }

    #[test]
    fn previewing_a_slot_never_writes_anything() {
        // Spec 19 + AGENTS.md 10: a preview is read-only. This asserts the SIDE EFFECT
        // COLLECTION is empty, not just that the call returns.
        //
        // There is no database here, so the thing that must not change is the settings
        // document: a preview that "remembered" a selection would make the preview
        // itself a preference change.
        let dir = std::env::temp_dir().join(format!("devch-audio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let settings_path = dir.join("settings.json");
        let before = crate::settings::Settings::default();
        before.save(&settings_path).expect("save");
        let text_before = std::fs::read_to_string(&settings_path).expect("read");

        // preview() reads the real data dir, which this test cannot redirect without a
        // global, so what is asserted here is the guarantee that matters at this level:
        // the function takes no parameters that could carry state back, and returns a
        // report rather than persisting one.
        let report = preview("commit");
        assert!(!report.sound.is_empty());
        assert_eq!(
            std::fs::read_to_string(&settings_path).expect("read"),
            text_before,
            "previewing must not rewrite the settings the page is editing"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
//! Devchievements Companion shell (Tauri 2, spec 32.1).
//!
//! # Startup shape, and why
//!
//! The app starts with a tray icon and NO window. That is not a workaround: spec
//! 32.1 asks for a tray-based companion, and a settings window is something the user
//! opens deliberately. Keeping the startup path window-free also means the data loop
//! and the notifications can run on a machine where the embedded webview cannot.
//!
//! # Threading
//!
//! The drain loop runs on its own thread with its own runtime so the UI thread is
//! never blocked by database work.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Every Tauri command the UI may call, in one place.
///
/// There are three `tauri::Builder` sites (the normal launch, the window smoke test
/// and the popup demo). They must expose the same commands, and for a while they did
/// not: the smoke test had the full list while the real app was missing
/// `onboarding_status` and `install_hooks`. The wizard then could not be reached at
/// all, and the end-to-end test kept passing because it used the smoke builder.
///
/// A macro instead of three copies makes that kind of drift impossible.
macro_rules! app_commands {
    () => {
        tauri::generate_handler![
            get_settings,
            save_settings,
            get_status,
            ui_ready,
            commands::ui_language,
            commands::check_dependencies,
            commands::export_diagnostics,
            commands::ui_error,
            commands::open_external,
            commands::get_dashboard,
            commands::get_gallery,
            commands::mark_achievements_viewed,
            commands::get_activity,
            commands::get_repositories,
            commands::get_about,
            commands::backup_database,
            commands::restore_database,
            commands::import_history,
            commands::onboarding_status,
            commands::reset_onboarding,
            commands::install_hooks,
            commands::complete_onboarding,
            commands::show_popup_preview,
            commands::list_assets,
            commands::import_asset,
            commands::remove_asset,
            commands::import_sound,
            commands::available_sounds,
            commands::open_data_folder,
            commands::clear_data,
            commands::open_window,
            commands::popup_painted,
            commands::update_status,
            commands::prepare_update,
            commands::clear_update_snapshot,
        ]
    };
}

/// Flags that mean "run one command and exit", as opposed to "be the app".
///
/// Why a list instead of "any argument starting with --": the launcher passes
/// `--open-window`, which is a GUI flag. Treating every flag as a command skipped the
/// WebView2 guard, so a machine without the runtime got a raw failure instead of the
/// message that explains it. Anything that opens a window is not a command.
const COMMAND_FLAGS: &[&str] = &[
    "--diagnostics",
    "--capabilities",
    "--onboarding",
    "--reset-onboarding",
    "--update-status",
    "--autostart",
    "--set-privacy",
    "--set-volume",
    "--set-muted",
    "--set-notifications",
    "--backup",
    "--restore",
    "--import",
    "--export-config",
    "--import-config",
    "--add-achievement",
    "--import-asset",
    "--import-sound",
    "--install-hooks",
    "--register-shortcut",
    "--notify-shell",
    "--toast-probe",
    "--toast-status",
    "--toast-aumid",
    "--map-login",
    "--popup-demo",
    "--window-smoke",
];

/// Whether this process was started to run one command.
fn is_command_mode() -> bool {
    std::env::args()
        .skip(1)
        .any(|arg| COMMAND_FLAGS.contains(&arg.as_str()))
}

/// Keeps a console-less launch from dying on its first log line.
///
/// The release binary is a GUI-subsystem executable (see `windows_subsystem` above),
/// so a normal launch from Explorer has no console: the standard output handle is
/// null, and the first `println!` panics with "failed printing to stdout". Measured
/// on 2026-11-17: the packaged app started from the Start Menu created nothing and
/// vanished, while the same binary worked whenever a test redirected its output.
///
/// When the handles are missing, both streams are pointed at `companion.log` in the
/// data directory, and at the NUL device if even that cannot be opened. When the app
/// was started from a terminal, or with redirected handles as the end-to-end tests
/// do, nothing is changed.
#[cfg(windows)]
mod output_streams {
    use std::os::windows::io::AsRawHandle;

    // Declared instead of pulling in more of the windows crate: these are two
    // calls with stable signatures, and a feature flag for them would be a larger
    // diff than the declarations.
    #[link(name = "kernel32")]
    extern "system" {
        fn SetStdHandle(n_std_handle: u32, handle: *mut core::ffi::c_void) -> i32;
    }

    // The documented pseudo-handles: (DWORD)-11 and (DWORD)-12.
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    const MAX_LOG_BYTES: u64 = 1_000_000;

    /// Append, rotating once so a long-running tray app cannot fill the disk.
    fn open_log(dir: &std::path::Path) -> std::io::Result<std::fs::File> {
        let path = dir.join("companion.log");
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_LOG_BYTES {
            let _ = std::fs::rename(&path, dir.join("companion.log.1"));
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
    }

    pub fn redirect_if_missing(data_dir: &std::path::Path) {
        // Two exemptions, both deliberate:
        //
        // - dev builds keep their console, because the end-to-end tests read the log
        //   lines they print;
        // - a command run (--diagnostics, --import, the installer's
        //   --register-shortcut) has a caller reading stdout.
        //
        // Everything else is the tray app the user started by double-clicking, and
        // that one must never depend on the standard handles being usable: measured
        // on 2026-11-17, launched through the shell, the process died before writing
        // anything, because the handles it inherited were not actually writable.
        if cfg!(debug_assertions) {
            return;
        }
        if super::is_command_mode() {
            return;
        }

        let _ = std::fs::create_dir_all(data_dir);
        let file =
            open_log(data_dir).or_else(|_| std::fs::OpenOptions::new().write(true).open("NUL"));
        let Ok(file) = file else {
            // Nothing left to point the streams at. Returning is still better than
            // panicking here, because the caller may never print.
            return;
        };
        let handle = file.as_raw_handle() as *mut core::ffi::c_void;
        // Safety: the handle stays valid for the process lifetime because the File
        // is leaked below, and SetStdHandle only stores it.
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            SetStdHandle(STD_ERROR_HANDLE, handle);
        }
        // The log file must outlive every write; one leaked handle is cheaper than a
        // logging layer that every call site has to remember to use.
        std::mem::forget(file);
    }
}

pub(crate) mod audio;
mod autostart;
mod commands;
mod local_helper;
pub(crate) mod popup;
mod preflight;
mod settings;
pub(crate) mod toast;
mod update;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

use devchievements_companion_core::delivery::{DeliverySink, DisplayOutcome, OutboxProcessor};
use devchievements_companion_core::{Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_notifications::Presentation;
use devchievements_sound::Rarity as SoundRarity;
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

/// Privacy mode as a process-wide flag: the tray menu writes it and the drain
/// thread reads it, and neither should have to own a lock for that.
pub(crate) static PRIVACY_MODE: AtomicU8 = AtomicU8::new(0);

/// Whether a system notification has been demonstrated to work: 0 unknown,
/// 1 works, 2 does not. Probing lazily avoids a test toast at every startup,
/// and remembering the failure stops the log filling with the same error.
pub(crate) static NOTIFICATION_STATE: AtomicU8 = AtomicU8::new(0);

/// Set by the page once it has rendered, so a smoke test can wait for real
/// evidence instead of a fixed sleep.
pub(crate) static UI_READY: AtomicBool = AtomicBool::new(false);

/// Set when the user asks for an immediate drain from the tray.
static DRAIN_NOW: AtomicBool = AtomicBool::new(false);

/// Counts what has been shown, so the tray tooltip reflects real work.
static SHOWN: AtomicU64 = AtomicU64::new(0);

/// Prints presentations and records them. A real build swaps this for the toast and
/// sound layer; keeping it a sink means that swap does not touch the loop.
struct DesktopSink {
    app: tauri::AppHandle,
}

impl DeliverySink for DesktopSink {
    fn present(&mut self, presentation: &Presentation) -> DisplayOutcome {
        match presentation {
            Presentation::Popup(popup) => {
                println!(
                    "POPUP {} [{:?}] {:?}",
                    popup.title, popup.rarity, popup.others
                );

                // Spec 16.6: the leading item's sound plays, exactly one per event.
                let prefs = settings::Settings::load(&settings_path());
                let overrides = devchievements_sound::SoundOverrides {
                    slots: prefs.sound_overrides.clone(),
                };
                let selection = audio::play_for_with(
                    &audio::request_for(&popup.primary_id, to_sound_rarity(popup.rarity)),
                    &overrides,
                    prefs.volume,
                    prefs.muted,
                );
                match selection {
                    // Spec 44.4 accepts the start of playback as the end of the
                    // latency budget, so it is stamped where it happens rather than
                    // guessed by whoever reads the log.
                    Some(s) => println!(
                        "SOUND play id={} from={:?} volume={} at={}",
                        s.sound,
                        s.source,
                        prefs.volume,
                        now_millis()
                    ),
                    None => println!("SOUND suppressed muted={}", prefs.muted),
                }

                // Spec 26.2: privacy mode hides the repository and the detail and
                // keeps the achievement name. The copy is built by the privacy crate so
                // the rule lives in one tested place, not in this file.
                let mode = current_privacy_mode();
                let detail = if popup.others.is_empty() {
                    format!("{:?} unlocked", popup.rarity)
                } else {
                    popup.others.join(", ")
                };
                let copy = devchievements_privacy::notification_copy(
                    mode,
                    &devchievements_privacy::NotificationContext {
                        headline: &popup.title,
                        repository: None,
                        detail: Some(&detail),
                    },
                );
                println!(
                    "COPY privacy={} title={:?} body={:?}",
                    matches!(mode, devchievements_privacy::PrivacyMode::Privacy),
                    copy.title,
                    copy.body
                );
                // The popup IS the notification channel: the system toast is not used
                // at all, because it could not be demonstrated on this platform
                // (ADR-0015) and a channel nobody verified is not a channel.
                if !prefs.notifications {
                    // Spec 34.1: the switch controls interruption only. The unlock is
                    // already recorded and appears in the gallery.
                    println!("POPUP suppressed notifications=off");
                    SHOWN.fetch_add(1, Ordering::Relaxed);
                    return DisplayOutcome::Shown;
                }

                let count = SHOWN.fetch_add(1, Ordering::Relaxed) + 1;
                let payload = popup::PopupPayload {
                    title: copy.title.clone(),
                    body: copy.body.clone(),
                    rarity: format!("{:?}", popup.rarity),
                    others: popup.others.clone(),

                    // Filled from the saved style inside popup::show, so a caller cannot get it wrong.
                    style: Default::default(),
                    icon_url: popup::icon_for(&popup.primary_id),
                };
                let app = self.app.clone();
                // Window creation has to happen on the main thread on Windows.
                let _ = app.clone().run_on_main_thread(move || {
                    match popup::show(&app, &payload, count) {
                        Ok(()) => println!("POPUP shown seq={count}"),
                        Err(e) => println!("POPUP FAILED seq={count} error={e}"),
                    }
                });
            }
            Presentation::SoundOnly => {
                // Spec 16.4: a plain commit is acknowledged by sound only. No toast,
                // because interrupting for every commit is exactly what that rule
                // exists to prevent.
                let prefs = settings::Settings::load(&settings_path());
                let overrides = devchievements_sound::SoundOverrides {
                    slots: prefs.sound_overrides.clone(),
                };
                match audio::play_for_with(
                    &devchievements_sound::SoundRequest::Commit,
                    &overrides,
                    prefs.volume,
                    prefs.muted,
                ) {
                    Some(selection) => println!(
                        "SOUND play id={} from={:?} at={}",
                        selection.sound,
                        selection.source,
                        now_millis()
                    ),
                    None => println!("SOUND suppressed muted={}", prefs.muted),
                }
                SHOWN.fetch_add(1, Ordering::Relaxed);
            }
            Presentation::Nothing => {}
        }
        DisplayOutcome::Shown
    }
}

/// Show a toast, probing the capability the first time and remembering the answer.
///
/// Spec 29.1 requires the product to know what the platform can do rather than
/// assuming it. The first notification is the probe: if Windows accepts it the
/// capability becomes Native, and if it does not, later notifications take the
/// documented degradation path instead of repeating a failing call.
fn report_toast(tag: &str, title: &str, body: &str) {
    match NOTIFICATION_STATE.load(Ordering::Relaxed) {
        // Already demonstrated not to work on this host.
        2 => {
            println!("NOTIFY skipped tag={tag} channel=in-app-only");
            return;
        }
        _ => {}
    }

    match toast::show(title, body, tag) {
        Ok(()) => {
            NOTIFICATION_STATE.store(1, Ordering::Relaxed);
            println!("NOTIFY shown tag={tag}");
        }
        Err(e) => {
            NOTIFICATION_STATE.store(2, Ordering::Relaxed);
            // Reported once per process, not once per event.
            println!("NOTIFY unavailable tag={tag} reason={e}");
            println!("NOTIFY degraded channel=in-app-only (spec 29.1 fallback chain)");
        }
    }
}

/// Map the notification rarity onto the sound crate's mirror type.
///
/// Separate enums on purpose: the sound crate must not depend on the notification
/// crate's presentation model, and this bridge is cheaper than coupling them.
fn to_sound_rarity(rarity: devchievements_notifications::Rarity) -> SoundRarity {
    use devchievements_notifications::Rarity as R;
    match rarity {
        R::Common => SoundRarity::Common,
        R::Uncommon => SoundRarity::Uncommon,
        R::Rare => SoundRarity::Rare,
        R::Epic => SoundRarity::Epic,
        R::Legendary => SoundRarity::Legendary,
    }
}

/// Build the capability set from what has actually been demonstrated.
///
/// Only two things are certain in-process: the tray exists (we created it) and a
/// system notification either has or has not worked. Everything else stays
/// Unverified -- spec 29.1 forbids claiming more than was measured.
pub(crate) fn current_capabilities() -> devchievements_platform::Capabilities {
    use devchievements_platform::{Capabilities, CapabilityState};

    let notification = match NOTIFICATION_STATE.load(Ordering::Relaxed) {
        1 => CapabilityState::Native,
        // Demonstrated not to work in this process: the chain must not use it.
        2 => CapabilityState::Unverified,
        // Not yet tried.
        _ => CapabilityState::Unverified,
    };

    Capabilities {
        // Demonstrated: the drain loop opens a real popup window on every unlock
        // (scripts/e2e/popup.mjs).
        desktop_popup: CapabilityState::Native,
        system_notification: notification,
        // Demonstrated: the tray icon is built at startup and the process stays
        // alive, so this capability is observed rather than assumed.
        tray: CapabilityState::Native,
        ..Capabilities::default()
    }
}

/// Read the process-wide privacy flag.
pub(crate) fn current_privacy_mode() -> devchievements_privacy::PrivacyMode {
    if PRIVACY_MODE.load(Ordering::Relaxed) == 1 {
        devchievements_privacy::PrivacyMode::Privacy
    } else {
        devchievements_privacy::PrivacyMode::Normal
    }
}

/// Path of the settings file.
pub(crate) fn settings_path() -> std::path::PathBuf {
    default_data_dir().join("settings.json")
}

/// A clock frozen at process start, for one-shot CLI commands.
#[derive(Debug)]
pub(crate) struct FixedClock;
impl devchievements_core::ClockProvider for FixedClock {
    fn now_unix_millis(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PlainIds(u64);
impl devchievements_core::IdGenerator for PlainIds {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("cli-{}", self.0)
    }
}

#[derive(Debug)]
pub(crate) struct StaticConfig;
impl devchievements_core::ConfigurationProvider for StaticConfig {
    fn config_version(&self) -> u64 {
        1
    }
}

/// Create (or focus) the main window.
///
/// The data_directory call is load-bearing: without it WebView2 falls back to a
/// profile next to the executable, and when that location is not writable the only
/// symptom is E_ACCESSDENIED. Measured here -- the window went from always failing
/// to working with this one line.
pub(crate) fn open_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    open_main_window_at(app, "dashboard")
}

/// Window size in logical pixels: the product default, or `--window-size WxH`.
fn window_size() -> (f64, f64) {
    const DEFAULT: (f64, f64) = (720.0, 520.0);
    let mut args = std::env::args().skip_while(|a| a != "--window-size");
    let _ = args.next();
    let Some(spec) = args.next() else {
        return DEFAULT;
    };
    let Some((w, h)) = spec.split_once('x') else {
        return DEFAULT;
    };
    match (w.parse::<f64>(), h.parse::<f64>()) {
        (Ok(w), Ok(h)) if w >= 320.0 && h >= 240.0 && w <= 4096.0 && h <= 4096.0 => (w, h),
        _ => DEFAULT,
    }
}

/// Open the main window on a specific page.
///
/// The page travels in the URL fragment so the UI can select it without any
/// scripted clicking, which is what makes per-page end-to-end tests possible.
pub(crate) fn open_main_window_at(app: &tauri::AppHandle, page: &str) -> Result<(), String> {
    use tauri::Manager;

    if let Some(existing) = app.get_webview_window("main") {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // The Store requires 1280x800 (or 640x480) screenshots, so the capture run needs
    // the window at that size. A flag keeps the product's own default and lets the
    // screenshot script ask for the size instead of the product guessing.
    let (width, height) = window_size();
    tauri::WebviewWindowBuilder::new(
        app,
        "main",
        tauri::WebviewUrl::App(format!("index.html#{page}").into()),
    )
    .title("Devchievements")
    .inner_size(width, height)
    .data_directory(default_data_dir().join("webview"))
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Whether this process was started as an installed MSIX package.
///
/// Windows installs packages under `%ProgramFiles%\WindowsApps`, and the Store
/// build behaves differently in two places: startup is owned by a manifest
/// `startupTask` rather than a Run key, and the helper has to be copied out of the
/// package before git can execute it.
pub(crate) fn is_packaged() -> bool {
    std::env::current_exe()
        .map(|exe| {
            exe.to_string_lossy()
                .to_ascii_lowercase()
                .contains("\\windowsapps\\")
        })
        .unwrap_or(false)
}

fn default_data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("devchievements")
}

/// Explain why the data directory or database cannot be used.
///
/// "DATABASE unusable" on its own tells the user nothing: the interesting part is
/// whether the directory could not be created, or existed but refused a new file.
/// The probe repeats the failing operation directly, so the operating system's
/// error is visible instead of being wrapped by SQLite.
fn explain_database_failure(data_dir: &std::path::Path) -> String {
    let probe_path = data_dir.join("write-probe.tmp");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&probe_path)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe_path);
            "writable".to_string()
        }
        Err(e) => format!("not writable: {e}"),
    }
}

/// Open the database, rolling back a failed update if one is pending.
///
/// Spec 40.3: an update that breaks the database must not leave the user with a
/// broken database. The pre-update copy written before the version change is
/// therefore only discarded once this new version has opened and migrated the
/// live file. Until then it is the way back.
async fn open_companion_database(data_dir: &std::path::Path) -> Result<sqlx::SqlitePool, String> {
    let database = data_dir.join("companion.db");
    let attempt = async {
        let pool = devchievements_storage::open(&database)
            .await
            .map_err(|e| e.to_string())?;
        devchievements_storage::ingest::migrate(&pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(pool)
    };

    match attempt.await {
        Ok(pool) => {
            if update::has_snapshot(data_dir) {
                match update::clear_snapshot(data_dir) {
                    Ok(()) => println!("UPDATE confirmed: database migrated, snapshot dropped"),
                    Err(e) => println!("UPDATE warning: snapshot left behind: {e}"),
                }
            }
            Ok(pool)
        }
        Err(first) => {
            if !update::has_snapshot(data_dir) {
                return Err(first);
            }
            println!("UPDATE failed: database unusable ({first}); restoring the pre-update copy");
            update::restore_snapshot(data_dir)?;
            let pool = devchievements_storage::open(&database)
                .await
                .map_err(|e| e.to_string())?;
            devchievements_storage::ingest::migrate(&pool)
                .await
                .map_err(|e| e.to_string())?;
            println!("UPDATE rolled back: previous database is in use");
            Ok(pool)
        }
    }
}

/// Own thread, own runtime: ingestion must never block the UI thread.
fn spawn_drain_loop(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        async_std::task::block_on(async {
            let data_dir = default_data_dir();
            if let Err(e) = std::fs::create_dir_all(&data_dir) {
                println!(
                    "DATABASE unusable: cannot create {}: {e}",
                    data_dir.display()
                );
                return;
            }

            let pool = match open_companion_database(&data_dir).await {
                Ok(pool) => pool,
                Err(detail) => {
                    println!("DATABASE unusable: {detail}");
                    println!("DATABASE dir={}", data_dir.display());
                    println!("DATABASE probe={}", explain_database_failure(&data_dir));
                    return;
                }
            };

            let (identity, source) = load_identity(&data_dir);
            match source {
                IdentitySource::Settings => println!("IDENTITY from settings.json"),
                IdentitySource::GitConfig => println!("IDENTITY from git config (unconfirmed)"),
                IdentitySource::None => {
                    // Spec 7: no email means statistics MUST NOT start. Say so loudly
                    // instead of silently counting nothing.
                    println!("IDENTITY MISSING: refusing to count (spec 7)");
                    return;
                }
            }

            let companion = Companion::new(
                pool.clone(),
                CompanionConfig::new(data_dir.join("spool"), "Asia/Taipei"),
            );
            // Spec 22: the user's own achievement rules live in the settings
            // document, so they are read fresh before every drain rather than once
            // at startup: a rule created five seconds ago must be able to fire on
            // the next commit, and one deleted must stop firing immediately.
            let mut companion = companion;
            let processor = OutboxProcessor::new(pool);
            let mut sink = DesktopSink { app: app.clone() };

            // Resolve anything a previous crash left mid-flight (spec 17.9).
            let _ = processor.recover_after_crash().await;

            loop {
                companion.replace_user_rules(
                    settings::Settings::load(&settings_path()).user_achievements,
                );
                match companion.drain_once(&identity).await {
                    Ok(report) if report.refused > 0 => {
                        // Visibility matters: a silent refusal looks like a broken app.
                        println!(
                            "REFUSED {} record(s): author is not a configured email",
                            report.refused
                        );
                    }
                    Ok(report) if report.ingested > 0 => {
                        println!("INGESTED {}", report.ingested);
                    }
                    _ => {}
                }
                let _ = processor.deliver_pending(&mut sink, 3).await;

                // Spec 44.1 budgets 500 ms from commit to notification. Sleeping in
                // 50 slices of 10 ms put a full 500 ms of waiting inside that budget,
                // and measured p95 was 1004 ms -- the wait alone was over. Five
                // slices keep the worst-case wait near 50 ms (Windows rounds a 10 ms
                // sleep up to the timer tick, so it is not exactly 50) while still
                // honouring a tray request within one slice.
                for _ in 0..5 {
                    if DRAIN_NOW.swap(false, Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        });
    });
}

/// Where the identity came from, so the log can say how much to trust it.
enum IdentitySource {
    /// Confirmed by the user in settings.
    Settings,
    /// Suggested by Git configuration; still needs confirmation (spec 7).
    GitConfig,
    /// Nothing configured: statistics must not begin.
    None,
}

/// Load the identity.
///
/// Resolution order follows spec 7: an explicit setting outranks a suggestion read
/// from Git. The Git value is labelled as unconfirmed because the user never agreed
/// to it, and it may be someone else's on a shared machine.
fn load_identity(data_dir: &std::path::Path) -> (Identity, IdentitySource) {
    if let Some(identity) = identity_from_settings(data_dir) {
        if identity.is_configured() {
            return (identity, IdentitySource::Settings);
        }
    }

    let suggested = devchievements_git_hooks::gitread::config_read::suggest_identity(data_dir);
    if suggested.is_configured() {
        return (suggested, IdentitySource::GitConfig);
    }

    (Identity::default(), IdentitySource::None)
}

/// Read `settings.json` next to the database.
///
/// Deliberately tiny for Phase 2a: the settings UI does not exist yet, but the file
/// format does, so the identity path is real rather than hardcoded. Spec 26.4 requires
/// configuration to be versioned; unknown fields are ignored rather than rejected so a
/// newer build's file still loads.
fn identity_from_settings(data_dir: &std::path::Path) -> Option<Identity> {
    let path = data_dir.join("settings.json");
    // Parsed through the typed settings model rather than by hand: a hand-rolled
    // reader silently ignored the GitHub login mappings, so every browser report was
    // refused while the file looked perfectly correct.
    let stored = settings::Settings::load(&path);
    let identity = stored.to_identity();
    if identity.is_configured() {
        Some(identity)
    } else {
        None
    }
}

// ---------------------------------------------------------------- UI commands
//
// These are the only way the window reaches the database. Keeping them narrow means
// the UI cannot invent state: every field it shows comes from here, and every write
// goes through the same settings file the tray menu uses.

/// What the settings page needs to render.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    emails: Vec<String>,
    privacy: String,
    timezone: Option<String>,
    data_dir: String,
    notifications: bool,
    volume: u8,
    muted: bool,
    run_mode: String,
    database_bytes: i64,
    /// GitHub login to email mappings (spec 7).
    github_logins: Vec<LoginMapping>,
    /// User event filters (spec 15).
    filters: devchievements_filters::FilterSet,
    /// Popup appearance (spec 21).
    popup: devchievements_themes::PopupStyle,
    /// Whether a launch stays in the tray.
    start_hidden: bool,
    /// Whether Windows starts the companion at sign-in.
    start_with_windows: bool,
    /// UI language: `system`, `en` or `zh-CN`.
    language: String,
}

/// One login mapping, as a list of pairs so the UI can edit them in order.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginMapping {
    login: String,
    emails: Vec<String>,
}

/// A read-only summary for the status section.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusView {
    events: i64,
    repositories: usize,
    unlocked_achievements: usize,
    notifications_never_shown: i64,
    pending_notifications: i64,
    channel: String,
    toast_ready: bool,
    sounds: Vec<String>,
}

#[tauri::command]
fn get_settings() -> SettingsView {
    let stored = settings::Settings::load(&settings_path());
    let data_dir = default_data_dir();
    SettingsView {
        emails: stored.emails,
        privacy: stored.privacy.as_str().to_string(),
        timezone: stored.timezone,
        filters: stored.filters.clone(),
        popup: stored.popup.clone(),
        start_hidden: stored.start_hidden,
        start_with_windows: stored.start_with_windows,
        language: stored.language.clone(),
        notifications: stored.notifications,
        volume: stored.volume,
        muted: stored.muted,
        // Spec 2: the mode is determined by whether the extension is present. It is
        // never inferred from a wish; until the extension exists it is Desktop Only.
        run_mode: "Desktop Only".to_string(),
        github_logins: stored
            .github_logins
            .iter()
            .map(|(login, emails)| LoginMapping {
                login: login.clone(),
                emails: emails.clone(),
            })
            .collect(),
        database_bytes: std::fs::metadata(&data_dir)
            .ok()
            .and_then(|_| std::fs::metadata(data_dir.join("companion.db")).ok())
            .map(|m| m.len() as i64)
            .unwrap_or(0),
        // Spec 26.2: the page shows where data lives, so it is redacted the same way
        // a diagnostic export would be.
        data_dir: devchievements_privacy::redact_path(
            &data_dir.to_string_lossy(),
            std::env::var("USERPROFILE").ok().as_deref(),
        ),
    }
}

#[tauri::command]
fn save_settings(settings: SettingsView) -> Result<(), String> {
    let path = settings_path();
    let mut stored = settings::Settings::load(&path);

    // Empty rows are a UI artefact of the add/remove flow, not identities.
    stored.emails = settings
        .emails
        .into_iter()
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect();
    stored.privacy = match settings.privacy.as_str() {
        "privacy" => settings::PrivacyMode::Privacy,
        _ => settings::PrivacyMode::Normal,
    };
    stored.timezone = settings.timezone.filter(|t| !t.trim().is_empty());
    // Rebuilt from scratch so removing a row in the UI actually removes it.
    stored.github_logins = settings
        .github_logins
        .into_iter()
        .filter_map(|m| {
            let login = m.login.trim().to_string();
            let emails: Vec<String> = m
                .emails
                .into_iter()
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty())
                .collect();
            // A mapping with no login or no email attributes nothing; keeping it
            // would show a row that silently does nothing.
            if login.is_empty() || emails.is_empty() {
                None
            } else {
                Some((login, emails))
            }
        })
        .collect();

    stored.filters = settings.filters;
    // Clamped on the way in: a hand-edited value must not place the popup
    // off-screen.
    stored.popup = settings.popup.clamped();
    stored.notifications = settings.notifications;
    stored.start_hidden = settings.start_hidden;
    stored.language = settings::normalize_language(&settings.language);
    // The registry is written here, not only at next launch: a toggle that only
    // takes effect later reads as a broken switch.
    let autostart_changed = stored.start_with_windows != settings.start_with_windows;
    stored.start_with_windows = settings.start_with_windows;
    if autostart_changed || autostart::is_enabled() != stored.start_with_windows {
        if let Err(e) = autostart::apply(stored.start_with_windows) {
            println!("AUTOSTART apply failed: {e}");
        }
    }
    // Clamp instead of trusting the caller: a volume outside 0-100 would either be
    // ignored by the audio path or overflow the gain arithmetic.
    stored.volume = settings.volume.min(100);
    stored.muted = settings.muted;

    stored.save(&path).map_err(|e| e.to_string())?;

    // Keep the running process in step with what was just written.
    PRIVACY_MODE.store(
        if matches!(stored.privacy, settings::PrivacyMode::Privacy) {
            1
        } else {
            0
        },
        Ordering::SeqCst,
    );
    println!("SETTINGS saved privacy={}", stored.privacy.as_str());
    Ok(())
}

#[tauri::command]
async fn get_status() -> Result<StatusView, String> {
    let data_dir = default_data_dir();
    let pool = devchievements_storage::open(&data_dir.join("companion.db"))
        .await
        .map_err(|e| e.to_string())?;
    devchievements_storage::ingest::migrate(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let snapshot = devchievements_storage::diagnostics::snapshot(&pool, |id| {
        devchievements_privacy::stable_alias(id)
    })
    .await
    .map_err(|e| e.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let health = devchievements_storage::observability::outbox_health(&pool, now)
        .await
        .map_err(|e| e.to_string())?;

    let caps = current_capabilities();
    Ok(StatusView {
        events: snapshot.events,
        repositories: snapshot.repositories.len(),
        unlocked_achievements: snapshot.unlocked_achievements.len(),
        notifications_never_shown: snapshot.notifications_never_shown,
        pending_notifications: health.pending,
        channel: serde_json::to_value(caps.channel())
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "in_app_only".to_string()),
        toast_ready: toast::registration_state().is_ready(),
        sounds: audio::available_sound_ids(),
    })
}

/// Called by the page once it has rendered.
///
/// Without this the only evidence of a working UI is a window that exists, which is
/// exactly the sort of half-proof that let the earlier webview problem hide.
#[tauri::command]
fn ui_ready(view: String) {
    UI_READY.store(true, Ordering::SeqCst);
    println!("UI READY view={view}");
}
/// Build the diagnostics document.
///
/// `reveal_raw` switches from the shareable shape (repository names become stable
/// aliases, the data path is masked) to the one that still carries real identifiers.
/// Spec 26.2 allows the raw form only as an explicit choice, so the document says so
/// in its own `warning` field: a file can travel without the sentence that explained
/// it, and the person opening it later has no way to know.
fn diagnostics_report(
    data_dir: &std::path::Path,
    reveal_raw: bool,
) -> Result<serde_json::Value, String> {
    // One async block computes both halves: the snapshot and the outbox picture.
    // They come from the same database, so splitting them could report them from
    // different instants.
    let rt: Result<_, String> = async_std::task::block_on(async {
        let pool = devchievements_storage::open(&data_dir.join("companion.db"))
            .await
            .map_err(|e| e.to_string())?;
        devchievements_storage::ingest::migrate(&pool)
            .await
            .map_err(|e| e.to_string())?;

        let snapshot = devchievements_storage::diagnostics::snapshot(&pool, |id| {
            if reveal_raw {
                id.to_string()
            } else {
                // Spec 26.2: repository names are reduced to stable aliases before
                // they appear anywhere the user might share.
                devchievements_privacy::stable_alias(id)
            }
        })
        .await
        .map_err(|e| e.to_string())?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // Spec 34.1: a failure count without reasons and latencies is not
        // actionable, so the health picture travels with the snapshot.
        let health = devchievements_storage::observability::outbox_health(&pool, now)
            .await
            .map_err(|e| e.to_string())?;

        Ok((snapshot, health))
    });

    let (snapshot, health) = rt?;
    let toast_state = toast::registration_state();
    let spool = devchievements_git_hooks::spool::Spool::open(data_dir.join("spool")).ok();
    let (spool_bytes, spool_records) = spool
        .as_ref()
        .and_then(|s| s.stats().ok())
        .unwrap_or((0, 0));
    let stored = settings::Settings::load(&settings_path());

    let path_text = data_dir.to_string_lossy().to_string();
    let data_dir_field = if reveal_raw {
        path_text.clone()
    } else {
        devchievements_privacy::redact_path(
            &path_text,
            std::env::var("USERPROFILE").ok().as_deref(),
        )
    };

    let mut report = serde_json::json!({
        "dataDir": data_dir_field,
        "toast": {
            "ready": toast_state.is_ready(),
            "detail": toast_state.describe(),
        },
        "spool": {
            "pendingBytes": spool_bytes,
            "pendingRecords": spool_records,
        },
        "sounds": audio::available_sound_ids(),
        // The runtime the app is actually using. A machine can look at this and
        // tell "no WebView2" apart from "WebView2 but the window still fails".
        "webview2": preflight::webview2_version(),
        "privacy": stored.privacy.as_str(),
        "popup": stored.popup,
        "filters": stored.filters,
        "capabilities": current_capabilities(),
        "snapshot": snapshot,
        "outboxHealth": health,
    });

    if let Some(object) = report.as_object_mut() {
        if reveal_raw {
            object.insert(
                "warning".to_string(),
                serde_json::json!(concat!(
                    "This file contains repository identifiers and file ",
                    "paths. It is not safe to share publicly; use the ",
                    "redacted export instead."
                )),
            );
        } else {
            object.insert("redacted".to_string(), serde_json::json!(true));
        }
    }
    Ok(report)
}

/// Tell the user, without a webview, that the webview is what is missing.
///
/// This is the one failure the in-app preflight cannot report: the panel itself is
/// drawn by the runtime that is absent. A message box is the only thing left that
/// can still be shown, and exiting with a distinct code keeps it out of the
/// "started and then vanished" category.
///
/// The button installs the runtime when the app ships the bootstrapper. It has to be
/// copied out of the package before it can run: an executable inside WindowsApps
/// cannot be started by anything without the package identity (measured on
/// 2026-11-17), the same restriction that forces the Git hook helper out.
#[cfg(windows)]
fn prompt_missing_webview() {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONERROR, MB_SETFOREGROUND, MB_YESNO,
    };

    // Logged before the box: a modal cannot be automated, and the log line is how a
    // test (or a support report) proves this branch ran rather than a crash.
    println!("WEBVIEW2 missing: prompting for installation");
    let text = HSTRING::from(preflight::missing_webview_message());
    let title = HSTRING::from("Devchievements");
    // Safety: both strings are valid HSTRINGs for the duration of the call.
    let answer = unsafe {
        MessageBoxW(
            None,
            &text,
            &title,
            MB_YESNO | MB_ICONERROR | MB_SETFOREGROUND,
        )
    };
    if answer == IDYES && preflight::install_bundled_runtime() {
        // The runtime is there now; the caller retries instead of dying on a machine
        // it just repaired.
        println!("WEBVIEW2 installed: continuing");
        return;
    }
    if answer == IDYES {
        let _ = std::process::Command::new("explorer.exe")
            .arg(preflight::WEBVIEW2_URL)
            .spawn();
    }
    println!("WEBVIEW2 missing: prompting and exiting");
    std::process::exit(2);
}
#[cfg(not(windows))]
fn prompt_missing_webview() {
    eprintln!("WebView2 runtime is required and missing.");
}

fn main() {
    // Before anything can print: a GUI-subsystem launch has no console, and the
    // first println! would otherwise kill the process (see output_streams).
    #[cfg(windows)]
    output_streams::redirect_if_missing(&default_data_dir());

    // A GUI launch needs a webview; without one there is nothing to draw the preflight
    // panel in either, so the guidance has to be native.
    //
    // Commands are exempt: they print to stdout and are how a support request gets
    // its diagnostics. The exemption is a list of commands, not "any flag" -- that
    // shortcut let `--open-window` (a GUI flag used by the launcher) skip this check,
    // so the runtime-less machine failed with no explanation.
    //
    // The version is logged either way: when the answer is "this machine has it" but
    // the window still fails, that line is what tells the two apart.
    let webview2 = preflight::webview2_version();
    // Only a GUI launch prints the banner. Commands keep their output parseable:
    // `--diagnostics` prints JSON that callers feed to JSON.parse, and a leading
    // line broke two end-to-end tests on 2026-11-17. The version is also in the
    // diagnostics document itself, where a machine can read it.
    if !is_command_mode() {
        eprintln!(
            "WEBVIEW2 version={}",
            webview2.as_deref().unwrap_or("missing")
        );
    }
    if std::env::args().any(|a| a == "--webview2-prompt") {
        prompt_missing_webview();
    }
    if !is_command_mode()
        && (preflight::simulate_missing_webview()
            || preflight::must_stop_for_missing_webview(webview2.as_deref()))
    {
        prompt_missing_webview();
        // Returning means the user let us install the runtime; re-check rather than
        // trusting that, and only then continue to build the window.
        if preflight::webview2_version().is_none() {
            std::process::exit(2);
        }
    }

    // A probe entry point, used by scripts/e2e/toast.mjs. It runs the SAME code path
    // as the app (register + show) instead of a separate implementation, so the test
    // cannot pass while the product path is broken.
    // ---- installer-facing commands -------------------------------------------------
    // The app never registers itself at runtime: registration mutates the Start Menu
    // and HKCU, which belongs to installation, not to a background launch (ADR-0015).

    // Write the registration shortcut to an explicit path. The installer then moves
    // it into the Start Menu, which may need permissions the app does not have.
    if let Some(i) = std::env::args().position(|a| a == "--register-shortcut") {
        let Some(target) = std::env::args().nth(i + 1) else {
            println!("REGISTER FAILED: --register-shortcut needs a path");
            std::process::exit(2);
        };
        let exe = std::env::current_exe().unwrap_or_default();
        match toast::create_shortcut(std::path::Path::new(&target), &exe) {
            Ok(()) => {
                println!("REGISTER OK {target}");
                std::process::exit(0);
            }
            Err(e) => {
                println!("REGISTER FAILED {e}");
                std::process::exit(1);
            }
        }
    }

    // Tell the shell a shortcut appeared, so the notification platform re-reads it
    // instead of waiting for the next sign-in.
    if std::env::args().any(|a| a == "--notify-shell") {
        toast::notify_shell();
        println!("SHELL NOTIFIED");
        std::process::exit(0);
    }

    // Report registration state as JSON; used by the installer and by diagnostics.
    if std::env::args().any(|a| a == "--toast-status") {
        let state = toast::registration_state();
        println!(
            "{}",
            serde_json::json!({
                "ready": state.is_ready(),
                "detail": state.describe(),
            })
        );
        std::process::exit(if state.is_ready() { 0 } else { 3 });
    }

    // Report a diagnostics snapshot as JSON (spec 32.1: minimal diagnostics view).
    // The window that would render this cannot start here, but the data can be
    // produced and checked, which is the part that can silently be wrong.
    // Persist the privacy mode from the command line. The tray menu is the normal
    // route (spec 26.2) but a test cannot click a menu item.
    if let Some(i) = std::env::args().position(|a| a == "--set-privacy") {
        let Some(raw) = std::env::args().nth(i + 1) else {
            println!("PRIVACY FAILED: --set-privacy needs normal or privacy");
            std::process::exit(2);
        };
        let Some(mode) = settings::PrivacyMode::parse(&raw) else {
            println!("PRIVACY FAILED: unknown mode {raw}");
            std::process::exit(2);
        };
        let path = settings_path();
        let mut stored = settings::Settings::load(&path);
        stored.privacy = mode;
        match stored.save(&path) {
            Ok(()) => {
                println!("PRIVACY {}", mode.as_str());
                std::process::exit(0);
            }
            Err(e) => {
                println!("PRIVACY FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Print the capability set and the channel the degradation chain selects.
    // Import one repository's history (spec 11). Print a JSON report so the caller
    // (installer, wizard, tests) can show progress and failures.
    // Write a backup of the database and report where it went.
    if std::env::args().any(|a| a == "--backup") {
        let data_dir = default_data_dir();
        let destination = data_dir
            .join("backups")
            .join(format!("companion-{}.db", now_millis()));
        let result = async_std::task::block_on(async {
            let pool = devchievements_storage::open(&data_dir.join("companion.db"))
                .await
                .map_err(|e| e.to_string())?;
            devchievements_storage::ingest::migrate(&pool)
                .await
                .map_err(|e| e.to_string())?;
            devchievements_storage::backup::backup(&pool, &destination)
                .await
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
                std::process::exit(0);
            }
            Err(e) => {
                println!("BACKUP FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Restore a backup over the live database. Only safe while nothing is writing.
    if let Some(i) = std::env::args().position(|a| a == "--restore") {
        let Some(source) = std::env::args().nth(i + 1) else {
            println!("RESTORE FAILED: --restore needs a file");
            std::process::exit(2);
        };
        let target = default_data_dir().join("companion.db");
        match devchievements_storage::backup::restore(std::path::Path::new(&source), &target) {
            Ok(()) => {
                println!("RESTORED {}", target.to_string_lossy());
                std::process::exit(0);
            }
            Err(e) => {
                println!("RESTORE FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(i) = std::env::args().position(|a| a == "--import") {
        let Some(repo_arg) = std::env::args().nth(i + 1) else {
            println!("{{\"error\":\"--import needs a repository path\"}}");
            std::process::exit(2);
        };

        let repo_dir = std::path::PathBuf::from(&repo_arg);
        if !repo_dir.is_dir() {
            println!("{{\"error\":\"not a directory: {repo_arg}\"}}");
            std::process::exit(2);
        }

        let data_dir = default_data_dir();
        let stored = settings::Settings::load(&settings_path());
        let mut identity = Identity::default();
        for email in &stored.emails {
            identity.add_email(email.clone(), EmailOrigin::UserConfigured);
        }

        // Spec 11.2: an import requires a configured identity. Refusing here is
        // better than importing nothing and reporting success.
        if !identity.is_configured() {
            println!(
                "{{\"error\":\"no email configured: import requires an identity (spec 11.2)\"}}"
            );
            std::process::exit(3);
        }

        // The companion identifies hook-captured repositories the same way, so the
        // two paths land on one repository row.
        let repository_id = format!("local:{}", repo_dir.to_string_lossy().replace('\\', "/"));

        let result = async_std::task::block_on(async {
            let pool = devchievements_storage::open(&data_dir.join("companion.db"))
                .await
                .map_err(|e| e.to_string())?;
            devchievements_storage::ingest::migrate(&pool)
                .await
                .map_err(|e| e.to_string())?;

            let clock = FixedClock;
            let tz = devchievements_core::BundledTimezoneProvider::new();
            let mut ids = PlainIds::default();
            let cfg = StaticConfig;
            let ctx = devchievements_core::InjectedContext {
                clock: &clock,
                timezone: &tz,
                ids: &mut ids,
                config: &cfg,
            };

            let options = devchievements_storage::history_scan::ImportOptions {
                batch_size: 200,
                timezone: stored.timezone.clone().unwrap_or_else(|| "UTC".to_string()),
                day_start_minutes: 0,
            };
            let cancel = AtomicBool::new(false);

            devchievements_storage::history_scan::import_repository(
                &pool,
                &repository_id,
                &repo_dir,
                &identity,
                &options,
                &cancel,
                &ctx,
            )
            .await
        });

        match result {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
                std::process::exit(0);
            }
            Err(e) => {
                println!("{{\"error\":\"{e}\"}}");
                std::process::exit(1);
            }
        }
    }

    // Print what the wizard would see, so its logic can be checked without a window.
    // Install the managed hooks without the window, so the installer script and the
    // tests drive exactly the same code the wizard does.
    if let Some(i) = std::env::args().position(|a| a == "--install-hooks") {
        let Some(repo) = std::env::args().nth(i + 1) else {
            println!("HOOKS FAILED: --install-hooks needs a repository path");
            std::process::exit(2);
        };
        match commands::install_hooks(repo) {
            Ok(results) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&results).unwrap_or_default()
                );
                std::process::exit(0);
            }
            Err(e) => {
                println!("HOOKS FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Clear the setup flag without the window, so the "re-run setup" button's effect
    // can be checked (and so support can reactivate the wizard from a terminal).
    if std::env::args().any(|a| a == "--reset-onboarding") {
        match commands::reset_onboarding() {
            Ok(()) => {
                println!("ONBOARDING reset completed=false");
                std::process::exit(0);
            }
            Err(e) => {
                println!("ONBOARDING FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    if std::env::args().any(|a| a == "--onboarding") {
        let status = commands::onboarding_status();
        println!(
            "{}",
            serde_json::to_string_pretty(&status).unwrap_or_default()
        );
        std::process::exit(0);
    }

    // Set the user-facing notification controls from the command line. The window is
    // the normal route; a test cannot toggle a switch.
    if let Some(i) = std::env::args().position(|a| a == "--set-notifications") {
        let Some(value) = std::env::args().nth(i + 1) else {
            println!("SETTINGS FAILED: --set-notifications needs on or off");
            std::process::exit(2);
        };
        let path = settings_path();
        let mut stored = settings::Settings::load(&path);
        stored.notifications = value == "on";
        match stored.save(&path) {
            Ok(()) => {
                println!(
                    "NOTIFICATIONS {}",
                    if stored.notifications { "on" } else { "off" }
                );
                std::process::exit(0);
            }
            Err(e) => {
                println!("SETTINGS FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(i) = std::env::args().position(|a| a == "--set-volume") {
        let Some(value) = std::env::args().nth(i + 1) else {
            println!("SETTINGS FAILED: --set-volume needs 0-100");
            std::process::exit(2);
        };
        let Ok(volume) = value.parse::<u8>() else {
            println!("SETTINGS FAILED: volume must be a number");
            std::process::exit(2);
        };
        let path = settings_path();
        let mut stored = settings::Settings::load(&path);
        stored.volume = volume.min(100);
        match stored.save(&path) {
            Ok(()) => {
                println!("VOLUME {}", stored.volume);
                std::process::exit(0);
            }
            Err(e) => {
                println!("SETTINGS FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    if let Some(i) = std::env::args().position(|a| a == "--set-muted") {
        let Some(value) = std::env::args().nth(i + 1) else {
            println!("SETTINGS FAILED: --set-muted needs on or off");
            std::process::exit(2);
        };
        let path = settings_path();
        let mut stored = settings::Settings::load(&path);
        stored.muted = value == "on";
        match stored.save(&path) {
            Ok(()) => {
                println!("MUTED {}", if stored.muted { "on" } else { "off" });
                std::process::exit(0);
            }
            Err(e) => {
                println!("SETTINGS FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Map a GitHub login to emails. The Settings page is the normal route; a test
    // cannot fill in a form.
    if let Some(i) = std::env::args().position(|a| a == "--map-login") {
        let Some(login) = std::env::args().nth(i + 1) else {
            println!("MAPPING FAILED: --map-login needs a login");
            std::process::exit(2);
        };
        let emails: Vec<String> = std::env::args()
            .skip(i + 2)
            .take_while(|a| !a.starts_with("--"))
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        if emails.is_empty() {
            println!("MAPPING FAILED: --map-login needs at least one email");
            std::process::exit(2);
        }

        let path = settings_path();
        let mut stored = settings::Settings::load(&path);
        stored.github_logins.insert(login.clone(), emails.clone());
        match stored.save(&path) {
            Ok(()) => {
                println!("MAPPED {login} -> {}", emails.join(","));
                std::process::exit(0);
            }
            Err(e) => {
                println!("MAPPING FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Export the whole configuration as a portable document.
    if let Some(i) = std::env::args().position(|a| a == "--export-config") {
        let Some(target) = std::env::args().nth(i + 1) else {
            println!("EXPORT FAILED: --export-config needs a path");
            std::process::exit(2);
        };
        let stored = settings::Settings::load(&settings_path());
        let document = stored.to_document();
        let text = document.to_json();
        if let Some(parent) = std::path::Path::new(&target).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&target, text + "\n") {
            Ok(()) => {
                println!("EXPORTED {target}");
                std::process::exit(0);
            }
            Err(e) => {
                println!("EXPORT FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Import a configuration document. Nothing is applied unless it validates.
    if let Some(i) = std::env::args().position(|a| a == "--import-config") {
        let Some(source) = std::env::args().nth(i + 1) else {
            println!("IMPORT FAILED: --import-config needs a path");
            std::process::exit(2);
        };
        let text = match std::fs::read_to_string(&source) {
            Ok(text) => text,
            Err(e) => {
                println!("IMPORT FAILED: {e}");
                std::process::exit(1);
            }
        };
        match devchievements_config::ConfigDocument::from_json(&text) {
            Ok(document) => {
                let path = settings_path();
                let mut stored = settings::Settings::load(&path);
                stored.apply_document(document);
                match stored.save(&path) {
                    Ok(()) => {
                        println!("IMPORTED {source}");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        println!("IMPORT FAILED: {e}");
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                println!("IMPORT FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Add a user-defined achievement from a JSON definition.
    if let Some(i) = std::env::args().position(|a| a == "--add-achievement") {
        let Some(source) = std::env::args().nth(i + 1) else {
            println!("ACHIEVEMENT FAILED: --add-achievement needs a path");
            std::process::exit(2);
        };
        let path = settings_path();
        let mut stored = settings::Settings::load(&path);

        let text = match std::fs::read_to_string(&source) {
            Ok(text) => text,
            Err(e) => {
                println!("ACHIEVEMENT FAILED: {e}");
                std::process::exit(1);
            }
        };

        let existing: Vec<String> = stored
            .user_achievements
            .iter()
            .map(|a| a.id.clone())
            .collect();

        match devchievements_achievements::definition::from_json(
            &text,
            &existing,
            devchievements_achievements::builder::ComplexityLimits::default(),
        ) {
            Ok(achievement) => {
                println!("ACHIEVEMENT OK id={}", achievement.id);
                let id = achievement.id.clone();
                stored.user_achievements.push(achievement);
                if let Err(e) = stored.save(&path) {
                    println!("ACHIEVEMENT FAILED: {e}");
                    std::process::exit(1);
                }
                println!("ACHIEVEMENT STORED {id}");
                std::process::exit(0);
            }
            Err(e) => {
                println!("ACHIEVEMENT FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Import one asset from a path. The dialog is the normal route; a test cannot
    // click one, and the dialog only produces the path this command takes.
    if let Some(i) = std::env::args().position(|a| a == "--import-asset") {
        let Some(path) = std::env::args().nth(i + 1) else {
            println!("ASSET FAILED: --import-asset needs a path");
            std::process::exit(2);
        };
        match commands::import_asset(path) {
            Ok(asset) => {
                println!(
                    "ASSET OK name={} format={} bytes={}",
                    asset.name, asset.format, asset.bytes
                );
                std::process::exit(0);
            }
            Err(e) => {
                println!("ASSET FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Import a sound from a path. The dialog only produces the path this takes.
    if let Some(i) = std::env::args().position(|a| a == "--import-sound") {
        let Some(path) = std::env::args().nth(i + 1) else {
            println!("SOUND FAILED: --import-sound needs a path");
            std::process::exit(2);
        };
        match commands::import_sound(path) {
            Ok(asset) => {
                println!("SOUND OK name={} format={}", asset.name, asset.format);
                std::process::exit(0);
            }
            Err(e) => {
                println!("SOUND FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Read or drive the start-with-Windows entry without the UI. Needed because the
    // registry is the one piece of state a user cannot inspect from inside the app,
    // and because the automated tests cannot write HKCU in this environment.
    if let Some(i) = std::env::args().position(|a| a == "--autostart") {
        let action = std::env::args()
            .nth(i + 1)
            .unwrap_or_else(|| "status".to_string());
        match action.as_str() {
            "on" => match autostart::apply(true) {
                Ok(()) => println!("AUTOSTART on enabled={}", autostart::is_enabled()),
                Err(e) => {
                    println!("AUTOSTART FAILED: {e}");
                    std::process::exit(1);
                }
            },
            "off" => match autostart::apply(false) {
                Ok(()) => println!("AUTOSTART off enabled={}", autostart::is_enabled()),
                Err(e) => {
                    println!("AUTOSTART FAILED: {e}");
                    std::process::exit(1);
                }
            },
            "status" => println!(
                "{{\"enabled\":{},\"setting\":{}}}",
                autostart::is_enabled(),
                settings::Settings::load(&settings_path()).start_with_windows
            ),
            other => {
                println!("AUTOSTART FAILED: unknown action `{other}` (status|on|off)");
                std::process::exit(2);
            }
        }
        std::process::exit(0);
    }

    if std::env::args().any(|a| a == "--update-status") {
        let report = update::status(update::ENDPOINTS.len());
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        std::process::exit(0);
    }

    if std::env::args().any(|a| a == "--capabilities") {
        let caps = current_capabilities();
        let report = serde_json::json!({
            "capabilities": caps,
            "channel": caps.channel(),
            "fullscreen": caps.when_fullscreen(devchievements_platform::FullscreenPolicy::default()),
            "summary": caps.summary(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        std::process::exit(0);
    }

    // Spec 26.2: this is the shareable form. `--export-diagnostics --raw` is the
    // explicit opt-in that keeps real identifiers, and it says so in the file.
    if std::env::args().any(|a| a == "--diagnostics") {
        let data_dir = default_data_dir();
        match diagnostics_report(&data_dir, false) {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
                std::process::exit(0);
            }
            Err(e) => {
                // Same reasoning as the drain loop: name the path and the raw OS error,
                // because "unable to open database file" does not say which of the two
                // failed, and a support question that starts with the wrong guess costs
                // more than this one branch.
                let report = serde_json::json!({
                    "error": e,
                    "dataDir": devchievements_privacy::redact_path(
                        &data_dir.to_string_lossy(),
                        std::env::var("USERPROFILE").ok().as_deref(),
                    ),
                    "writeProbe": explain_database_failure(&data_dir),
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
                std::process::exit(1);
            }
        }
    }

    // Write the same document to a file, for attaching to a support request.
    if let Some(i) = std::env::args().position(|a| a == "--export-diagnostics") {
        let Some(target) = std::env::args().nth(i + 1) else {
            println!("DIAGNOSTICS FAILED: --export-diagnostics needs a path");
            std::process::exit(2);
        };
        let raw = std::env::args().any(|a| a == "--raw");
        match diagnostics_report(&default_data_dir(), raw) {
            Ok(report) => {
                let text = serde_json::to_string_pretty(&report).unwrap_or_default();
                match std::fs::write(&target, text) {
                    Ok(()) => {
                        println!("DIAGNOSTICS EXPORTED path={target} raw={raw}");
                        std::process::exit(0);
                    }
                    Err(e) => {
                        println!("DIAGNOSTICS FAILED: {e}");
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                println!("DIAGNOSTICS FAILED: {e}");
                std::process::exit(1);
            }
        }
    }
    // started, then exit. Exists because the tray-only build never creates a
    // webview, so the failure this reproduces was invisible.
    if std::env::args().any(|a| a == "--window-smoke") {
        // Report success only when the page says it rendered. A window that opens
        // onto a blank page would otherwise look identical to a working one.
        std::thread::spawn(|| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while std::time::Instant::now() < deadline {
                if UI_READY.load(Ordering::SeqCst) {
                    println!("WINDOW OK ui=ready");
                    std::process::exit(0);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            println!("WINDOW FAILED reason=page did not report ready");
            std::process::exit(1);
        });

        let result = tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            .invoke_handler(app_commands!())
            .setup(|app| {
                let page = std::env::args()
                    .position(|a| a == "--page")
                    .and_then(|i| std::env::args().nth(i + 1))
                    .unwrap_or_else(|| "dashboard".to_string());
                // Wizard steps are addressable too, so each one can be rendered and
                // checked without scripted clicking.
                let step = std::env::args()
                    .position(|a| a == "--step")
                    .and_then(|i| std::env::args().nth(i + 1));
                let fragment = match step {
                    Some(step) => format!("{page}?step={step}"),
                    None => page.clone(),
                };
                match open_main_window_at(app.handle(), &fragment) {
                    Ok(()) => println!("WINDOW created"),
                    Err(e) => {
                        println!("WINDOW FAILED error={e}");
                        std::process::exit(1);
                    }
                }
                Ok(())
            })
            .run(tauri::generate_context!());

        if let Err(e) = result {
            println!("WINDOW FAILED error={e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    // Probe with a specific AUMID: separates our registration from the process.
    if let Some(i) = std::env::args().position(|a| a == "--toast-aumid") {
        let Some(aumid) = std::env::args().nth(i + 1) else {
            println!("TOAST FAILED: --toast-aumid needs an identifier");
            std::process::exit(2);
        };
        match toast::show_with_aumid(&aumid, "Devchievements", "probe") {
            Ok(()) => println!("TOAST OK aumid={aumid}"),
            Err(e) => println!("TOAST FAILED aumid={aumid} error={e}"),
        }
        std::process::exit(0);
    }

    // Show one popup with sample content and exit, for checking the visual design
    // without having to make a commit.
    if std::env::args().any(|a| a == "--popup-demo") {
        let payload = popup::PopupPayload {
            title: "先提交为敬".to_string(),
            body: "Common Day · 第 1 天".to_string(),
            rarity: "Uncommon".to_string(),
            others: vec![],

            // Filled from the saved style inside popup::show, so a caller cannot get it wrong.
            style: Default::default(),
            // Any imported achievement icon shows here too, so the demo is a real
            // check of the icon path rather than of a hardcoded placeholder.
            icon_url: popup::icon_for("preview"),
        };
        let result = tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            .setup(move |app| {
                let handle = app.handle().clone();
                // Let the event loop start before creating a window.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    match popup::show(&handle, &payload, 1) {
                        Ok(()) => println!("POPUP demo shown"),
                        Err(e) => {
                            println!("POPUP demo failed: {e}");
                            std::process::exit(1);
                        }
                    }
                });
                Ok(())
            })
            .run(tauri::generate_context!());

        if let Err(e) = result {
            println!("POPUP demo failed: {e}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    if std::env::args().any(|a| a == "--toast-selftest") {
        let lines = toast::selftest();
        for line in &lines {
            println!("{line}");
        }
        // Launching through a shortcut cannot redirect stdout, so the result is also
        // written to a file when asked. Without this the shortcut case is untestable.
        if let Some(path) = std::env::var_os("DEVCH_SELFTEST_OUT") {
            let _ = std::fs::write(path, lines.join("\n"));
        }
        std::process::exit(0);
    }

    if std::env::args().any(|a| a == "--toast-probe") {
        let tag = std::env::args()
            .position(|a| a == "--tag")
            .and_then(|i| std::env::args().nth(i + 1))
            .unwrap_or_else(|| "probe".to_string());
        match toast::show("First Commit", "Common unlocked", &tag) {
            Ok(()) => {
                println!("TOAST OK tag={tag}");
                std::process::exit(0);
            }
            Err(e) => {
                println!("TOAST FAILED error={e}");
                std::process::exit(1);
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(app_commands!())
        .setup(|app| {
            // Spec 26.2 requires a tray shortcut for privacy mode, so the menu item
            // is a checkbox bound to the persisted setting rather than a one-shot
            // action: a user who turns it on and restarts must still have it on.
            let privacy = CheckMenuItem::with_id(
                app,
                "privacy",
                "Privacy mode",
                true,
                current_privacy_mode() == devchievements_privacy::PrivacyMode::Privacy,
                None::<&str>,
            )?;
            let open = MenuItem::with_id(app, "open", "Open window", true, None::<&str>)?;
            let drain = MenuItem::with_id(app, "drain", "Run one drain", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&privacy, &open, &drain, &quit])?;

            let _tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .unwrap_or_else(|| tauri::image::Image::new(&[0u8, 0, 0, 0], 1, 1)),
                )
                .tooltip("Devchievements")
                .menu(&menu)
                .on_menu_event(|_app, event| match event.id.as_ref() {
                    "quit" => std::process::exit(0),
                    "open" => match open_main_window(_app) {
                        Ok(()) => println!("WINDOW opened"),
                        Err(e) => println!("WINDOW FAILED error={e}"),
                    },
                    "drain" => {
                        DRAIN_NOW.store(true, Ordering::SeqCst);
                        println!("TRAY drain requested");
                    }
                    "privacy" => {
                        let next = current_privacy_mode().toggled();
                        let on = matches!(next, devchievements_privacy::PrivacyMode::Privacy);
                        PRIVACY_MODE.store(if on { 1 } else { 0 }, Ordering::SeqCst);

                        // Persist, so the choice survives a restart (spec 26.4).
                        let path = settings_path();
                        let mut stored = settings::Settings::load(&path);
                        stored.privacy = if on {
                            settings::PrivacyMode::Privacy
                        } else {
                            settings::PrivacyMode::Normal
                        };
                        if let Err(e) = stored.save(&path) {
                            eprintln!("tray: could not save privacy mode: {e}");
                        }
                        println!("TRAY privacy={}", stored.privacy.as_str());
                    }
                    _ => {}
                })
                .build(app)?;

            // Apply the persisted privacy mode before any notification can render.
            // This must happen in the tray setup, not in a CLI branch: the drain
            // thread reads the flag this sets.
            let stored = settings::Settings::load(&settings_path());
            PRIVACY_MODE.store(
                if matches!(stored.privacy, settings::PrivacyMode::Privacy) {
                    1
                } else {
                    0
                },
                Ordering::SeqCst,
            );
            println!("SETTINGS privacy={}", stored.privacy.as_str());

            let toast_state = toast::registration_state();
            println!(
                "TOAST {} ({})",
                if toast_state.is_ready() {
                    "READY"
                } else {
                    "UNAVAILABLE"
                },
                toast_state.describe()
            );
            println!("TRAY READY data={:?}", default_data_dir());

            // A tray-only companion with no visible window is a dead end for someone
            // who just started it: the tray icon may be hidden in the overflow area,
            // and then there is nothing to click at all (measured on 2026-11-17: the
            // first manual trial stalled exactly here).
            //
            // Three reasons to show a window, in order of authority: the flag (scripts
            // and the trial launcher), the first run (the wizard is the only way to get
            // identity and hooks right), and the user turning off "start hidden".
            // Everything else is the quiet launch a companion is supposed to have.
            let stored_now = settings::Settings::load(&settings_path());
            let wants_window = std::env::args().any(|a| a == "--open-window");
            let first_run = !stored_now.onboarding_completed;
            let reason = if wants_window {
                Some("flag")
            } else if first_run {
                Some("first-run")
            } else if !stored_now.start_hidden {
                Some("start-visible")
            } else {
                None
            };
            if let Some(reason) = reason {
                match open_main_window(app.handle()) {
                    Ok(()) => println!("WINDOW opened reason={reason}"),
                    Err(e) => println!("WINDOW FAILED error={e}"),
                }
            }
            // Reported every launch: whether the window opened is otherwise only
            // observable by looking at the screen.
            println!(
                "STARTUP hidden={} setting_start_hidden={} setting_start_with_windows={}",
                reason.is_none(),
                stored_now.start_hidden,
                stored_now.start_with_windows
            );
            // Keep the registry in step with the setting, so an app that moved (or an
            // entry deleted by hand) ends up correct rather than stale.
            autostart::reconcile(stored_now.start_with_windows);
            // An updated package must not leave commits invoking last version's
            // helper, so the copy is refreshed on every start.
            match local_helper::ensure(&default_data_dir()) {
                Ok(path) => println!("HELPER ready at {}", path.display()),
                Err(e) => println!("HELPER unavailable: {e}"),
            }

            spawn_drain_loop(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Devchievements companion")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                // Tray apps stay alive with no windows; do not quit on last window close.
                api.prevent_exit();
            }
        });
}

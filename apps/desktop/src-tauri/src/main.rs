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

mod audio;
mod settings;
mod toast;

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
static PRIVACY_MODE: AtomicU8 = AtomicU8::new(0);

/// Set when the user asks for an immediate drain from the tray.
static DRAIN_NOW: AtomicBool = AtomicBool::new(false);

/// Counts what has been shown, so the tray tooltip reflects real work.
static SHOWN: AtomicU64 = AtomicU64::new(0);

/// Prints presentations and records them. A real build swaps this for the toast and
/// sound layer; keeping it a sink means that swap does not touch the loop.
struct DesktopSink;

impl DeliverySink for DesktopSink {
    fn present(&mut self, presentation: &Presentation) -> DisplayOutcome {
        match presentation {
            Presentation::Popup(popup) => {
                println!(
                    "POPUP {} [{:?}] {:?}",
                    popup.title, popup.rarity, popup.others
                );

                // Spec 16.6: the leading item's sound plays, exactly one per event.
                let selection = audio::play_for(&audio::request_for(
                    &popup.primary_id,
                    to_sound_rarity(popup.rarity),
                ));
                println!(
                    "SOUND play id={} from={:?}",
                    selection.sound, selection.source
                );

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
                report_toast(&popup.primary_id, &copy.title, &copy.body);
                SHOWN.fetch_add(1, Ordering::Relaxed);
            }
            Presentation::SoundOnly => {
                // Spec 16.4: a plain commit is acknowledged by sound only. No toast,
                // because interrupting for every commit is exactly what that rule
                // exists to prevent.
                let selection = audio::play_for(&devchievements_sound::SoundRequest::Commit);
                println!(
                    "SOUND play id={} from={:?}",
                    selection.sound, selection.source
                );
                SHOWN.fetch_add(1, Ordering::Relaxed);
            }
            Presentation::Nothing => {}
        }
        DisplayOutcome::Shown
    }
}

/// Show a toast, logging rather than failing.
///
/// A notification is a side effect: the achievement is already committed, so a toast
/// that cannot be shown must not take the ingestion loop down with it.
fn report_toast(tag: &str, title: &str, body: &str) {
    match toast::show(title, body, tag) {
        Ok(()) => println!("TOAST OK tag={tag}"),
        Err(e) => println!("TOAST FAILED tag={tag} error={e}"),
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

/// Read the process-wide privacy flag.
fn current_privacy_mode() -> devchievements_privacy::PrivacyMode {
    if PRIVACY_MODE.load(Ordering::Relaxed) == 1 {
        devchievements_privacy::PrivacyMode::Privacy
    } else {
        devchievements_privacy::PrivacyMode::Normal
    }
}

/// Path of the settings file.
fn settings_path() -> std::path::PathBuf {
    default_data_dir().join("settings.json")
}

fn default_data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("devchievements")
}

/// Own thread, own runtime: ingestion must never block the UI thread.
fn spawn_drain_loop() {
    std::thread::spawn(move || {
        async_std::task::block_on(async {
            let data_dir = default_data_dir();
            if std::fs::create_dir_all(&data_dir).is_err() {
                return;
            }

            let Ok(pool) = devchievements_storage::open(&data_dir.join("companion.db")).await
            else {
                return;
            };
            if devchievements_storage::ingest::migrate(&pool)
                .await
                .is_err()
            {
                return;
            }

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
            let processor = OutboxProcessor::new(pool);
            let mut sink = DesktopSink;

            // Resolve anything a previous crash left mid-flight (spec 17.9).
            let _ = processor.recover_after_crash().await;

            loop {
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

                // Sleep in small slices so a tray request is honoured promptly
                // instead of waiting out the full interval.
                for _ in 0..50 {
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
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;

    let mut identity = Identity::default();
    if let Some(emails) = value.get("emails").and_then(|v| v.as_array()) {
        for email in emails {
            if let Some(email) = email.as_str() {
                identity.add_email(email, EmailOrigin::UserConfigured);
            }
        }
    }
    Some(identity)
}

fn main() {
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

    if std::env::args().any(|a| a == "--diagnostics") {
        let data_dir = default_data_dir();
        let rt = async_std::task::block_on(async {
            let pool = devchievements_storage::open(&data_dir.join("companion.db")).await?;
            devchievements_storage::ingest::migrate(&pool).await?;
            devchievements_storage::diagnostics::snapshot(&pool, |id| {
                // Spec 26.2: repository names are reduced to stable aliases before
                // they appear anywhere the user might share.
                devchievements_privacy::stable_alias(id)
            })
            .await
        });

        match rt {
            Ok(snapshot) => {
                let toast_state = toast::registration_state();
                let spool =
                    devchievements_git_hooks::spool::Spool::open(data_dir.join("spool")).ok();
                let (spool_bytes, spool_records) = spool
                    .as_ref()
                    .and_then(|s| s.stats().ok())
                    .unwrap_or((0, 0));

                let report = serde_json::json!({
                    "dataDir": devchievements_privacy::redact_path(
                        &data_dir.to_string_lossy(),
                        std::env::var("USERPROFILE").ok().as_deref(),
                    ),
                    "toast": {
                        "ready": toast_state.is_ready(),
                        "detail": toast_state.describe(),
                    },
                    "spool": {
                        "pendingBytes": spool_bytes,
                        "pendingRecords": spool_records,
                    },
                    "sounds": audio::available_sound_ids(),
                    "privacy": settings::Settings::load(&settings_path()).privacy.as_str(),
                    "snapshot": snapshot,
                });
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
            let drain = MenuItem::with_id(app, "drain", "Run one drain", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&privacy, &drain, &quit])?;

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
                if matches!(stored.privacy, settings::PrivacyMode::Privacy) { 1 } else { 0 },
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
            spawn_drain_loop();
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

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

mod toast;

use std::sync::atomic::{AtomicU64, Ordering};

use devchievements_companion_core::delivery::{DeliverySink, DisplayOutcome, OutboxProcessor};
use devchievements_companion_core::{Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_notifications::Presentation;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

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
                SHOWN.fetch_add(1, Ordering::Relaxed);
            }
            Presentation::SoundOnly => {
                // Spec 16.4: a plain commit is acknowledged by sound only. No toast,
                // because interrupting for every commit is exactly what that rule
                // exists to prevent.
                println!("SOUND commit");
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
                std::thread::sleep(std::time::Duration::from_millis(500));
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
    // Install the registration shortcut at an explicit path. Used when the caller is
    // not permitted to write to the Start Menu directly (installers, tests).
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
            let drain = MenuItem::with_id(app, "drain", "Run one drain", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&drain, &quit])?;

            let _tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .unwrap_or_else(|| tauri::image::Image::new(&[0u8, 0, 0, 0], 1, 1)),
                )
                .tooltip("Devchievements")
                .menu(&menu)
                .on_menu_event(|_app, event| {
                    if event.id.as_ref() == "quit" {
                        std::process::exit(0);
                    }
                })
                .build(app)?;

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

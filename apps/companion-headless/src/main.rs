//! Headless Companion.
//!
//! # Why a headless binary exists before the windowed one
//!
//! Spec 32.1 asks for the data loop: Spool consumption, SQLite persistence, Engine
//! evaluation, Outbox delivery. Spec 32.5 asks for a crash-recovery matrix that
//! requires killing the process at six specific moments.
//!
//! Both are blocked by the windowed app needing MSVC. Neither is blocked by this
//! binary: it runs the identical library code, and it can be killed at will.
//!
//! The Tauri shell, when it exists, adds a window and a tray and calls the same
//! [Companion] methods. It does not reimplement any of this.

use std::path::PathBuf;
use std::process::ExitCode;

use devchievements_companion_core::delivery::{DeliverySink, DisplayOutcome, OutboxProcessor};
use devchievements_companion_core::{serve_ipc_blocking, Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};
use devchievements_notifications::Presentation;

/// Prints notifications instead of rendering them.
struct StdoutSink {
    shown: u64,
}

impl DeliverySink for StdoutSink {
    fn present(&mut self, presentation: &Presentation) -> DisplayOutcome {
        match presentation {
            Presentation::Popup(popup) => {
                // One line per popup, matching what a window would draw: the leading
                // reward, then up to three sub-lines, then a "+N" summary.
                println!(
                    "NOTIFY popup title={:?} rarity={} others={:?} omitted={}",
                    popup.title,
                    popup.rarity.as_str(),
                    popup.others,
                    popup.omitted
                );
                self.shown += 1;
            }
            Presentation::SoundOnly => {
                // Spec 16.4: a plain commit is acknowledged by sound, not a window.
                println!("SOUND commit");
                self.shown += 1;
            }
            Presentation::Nothing => return DisplayOutcome::Shown,
        }
        DisplayOutcome::Shown
    }
}

#[async_std::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("companion error: {e}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Usage: devch-companion <data-dir> <email> [--once|--loop <ms>]
    let data_dir = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);
    let email = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "unknown@example.com".to_string());
    let once = args.iter().any(|a| a == "--once");
    let loop_ms: u64 = args
        .iter()
        .position(|a| a == "--loop")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    std::fs::create_dir_all(&data_dir)?;

    let mut identity = Identity::default();
    identity.add_email(email.clone(), EmailOrigin::UserConfigured);

    // Spec 7: a browser report is attributed by GitHub login. The headless companion
    // takes its identity from the command line, so map any login that resolves to
    // the configured email here too; otherwise every browser event would be refused
    // in tests while the GUI, which reads the settings file, accepted them.
    if let Ok(text) = std::fs::read_to_string(data_dir.join("settings.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(map) = value.get("githubLogins").and_then(|v| v.as_object()) {
                for (login, emails) in map {
                    if let Some(list) = emails.as_array() {
                        let owned: Vec<String> = list
                            .iter()
                            .filter_map(|e| e.as_str())
                            .filter(|e| e.eq_ignore_ascii_case(&email))
                            .map(|e| e.to_string())
                            .collect();
                        if !owned.is_empty() {
                            identity.add_login_mapping(login.clone(), owned);
                        }
                    }
                }
            }
        }
    }

    let pool = devchievements_storage::open(&data_dir.join("companion.db")).await?;
    devchievements_storage::ingest::migrate(&pool).await?;

    let mut config = CompanionConfig::new(data_dir.join("spool"), "Asia/Taipei");
    // Spec 22: the user's own rules live in the settings document. The GUI refreshes
    // them before every drain; the headless companion reads them once, because it is
    // short-lived by design.
    config.user_rules = std::fs::read_to_string(data_dir.join("settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| {
            serde_json::from_value(
                value
                    .get("userAchievements")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            )
            .ok()
        })
        .unwrap_or_default();
    let companion = Companion::new(pool.clone(), config);
    let processor = OutboxProcessor::new(pool.clone());

    // Anything left mid-flight by a previous crash must be resolved before we show
    // anything new (spec 17.9).
    let recovered = processor.recover_after_crash().await?;
    if recovered > 0 {
        println!("RECOVERED {recovered}");
    }

    // Bring up the live channel before the first drain so a Helper that fires during
    // startup is accepted rather than falling back to the Spool (spec 10.1).
    let nudge = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let nudge_writer = nudge.clone();
    match serve_ipc_blocking(user_discriminator(), data_dir.join("spool"), move || {
        nudge_writer.store(true, std::sync::atomic::Ordering::SeqCst)
    }) {
        Ok(()) => println!(
            "IPC LISTENING {}",
            devchievements_git_hooks::ipc::socket_name(&user_discriminator())
        ),
        // Not fatal: without the channel everything still works via the Spool, just
        // with the polling delay instead of an immediate nudge.
        Err(e) => println!("IPC UNAVAILABLE {e}"),
    }

    let mut sink = StdoutSink { shown: 0 };

    loop {
        let report = companion.drain_once(&identity).await?;
        println!(
            "DRAIN read={} ingested={} dup={} refused={} failed={} queued={} consumed={}",
            report.read,
            report.ingested,
            report.duplicates,
            report.refused,
            report.failed,
            report.queued_notifications,
            report.files_consumed
        );

        let (shown, unknown) = processor.deliver_pending(&mut sink, 3).await?;
        if shown > 0 || unknown > 0 {
            println!("DELIVER shown={shown} unknown={unknown}");
        }

        println!("READY");
        if once || loop_ms == 0 {
            break;
        }

        // Sleep, but wake immediately when the live channel delivered something.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(loop_ms);
        while std::time::Instant::now() < deadline {
            if nudge.swap(false, std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Same derivation as the Helper, so both sides agree on the socket name.
fn user_discriminator() -> String {
    let raw = std::env::var_os("USERNAME")
        .or_else(|| std::env::var_os("USER"))
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_default();
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "default".to_string()
    } else {
        cleaned
    }
}

fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("devchievements")
}

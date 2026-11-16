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

use devchievements_companion_core::delivery::{
    DeliverySink, DisplayOutcome, OutboxProcessor, PendingNotification,
};
use devchievements_companion_core::{Companion, CompanionConfig};
use devchievements_events::{EmailOrigin, Identity};

/// Prints notifications instead of rendering them.
struct StdoutSink {
    shown: u64,
}

impl DeliverySink for StdoutSink {
    fn show(&mut self, notification: &PendingNotification) -> DisplayOutcome {
        println!(
            "NOTIFY {} {} {}",
            notification.notification_id, notification.kind, notification.payload
        );
        self.shown += 1;
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
    identity.add_email(email, EmailOrigin::UserConfigured);

    let pool = devchievements_storage::open(&data_dir.join("companion.db")).await?;
    devchievements_storage::ingest::migrate(&pool).await?;

    let config = CompanionConfig::new(data_dir.join("spool"), "Asia/Taipei");
    let companion = Companion::new(pool.clone(), config);
    let processor = OutboxProcessor::new(pool.clone());

    // Anything left mid-flight by a previous crash must be resolved before we show
    // anything new (spec 17.9).
    let recovered = processor.recover_after_crash().await?;
    if recovered > 0 {
        println!("RECOVERED {recovered}");
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
        std::thread::sleep(std::time::Duration::from_millis(loop_ms));
    }

    Ok(ExitCode::SUCCESS)
}

fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("devchievements")
}

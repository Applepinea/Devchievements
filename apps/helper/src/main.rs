//! The Git hook helper.
//!
//! # What it is allowed to do
//!
//! Called by Git as `hook <name> <args...>`. It must:
//!
//! - never fail the Git operation (§9.2) — every error path exits 0;
//! - finish fast (§44.1: p95 < 50 ms) — it reads the repository directly, no
//!   subprocesses beyond the initial `git rev-parse` avoidance;
//! - never lose an event — if the Companion is unreachable, the event goes to the
//!   Spool.
//!
//! # Why this binary exists separately from the Companion
//!
//! It is spawned by Git on every commit. Anything expensive in its startup is paid
//! on every commit forever, so it links only what it needs and does no argument
//! parsing beyond what is required.

use std::path::PathBuf;
use std::process::ExitCode;

use devchievements_git_hooks::gitread::Repository;
use devchievements_git_hooks::ipc;
use devchievements_git_hooks::spool::Spool;

/// Whether the companion is installed, i.e. whether it owns a data directory.
///
/// Spec 34.1: hooks left behind after an uninstall must be silent. Creating the data
/// directory here would resurrect app state the user just deleted, so the helper
/// checks instead of creating -- the installer and the app are what create it.
fn companion_present() -> bool {
    devchievements_git_hooks::spool::default_data_dir()
        .map(|dir| dir.is_dir())
        .unwrap_or(false)
}

fn main() -> ExitCode {
    // Every failure mode returns success. A hook that fails a commit is worse than
    // a hook that misses an event (§9.2, §9.4).
    let _ = run();
    ExitCode::SUCCESS
}

fn run() -> Option<()> {
    // Nothing to record to and nothing to tell: exit without touching the disk.
    if !companion_present() {
        return None;
    }

    let mut args = std::env::args().skip(1);
    let first = args.next();

    match first.as_deref() {
        Some("hook") => {
            let hook_name = args.next()?;
            let hook_args: Vec<String> = args.collect();
            handle_hook(&hook_name, &hook_args)
        }
        Some("identity") => {
            // Used by onboarding to show what Git already knows.
            let dir = std::env::current_dir().ok()?;
            let identity = devchievements_git_hooks::gitread::config_read::suggest_identity(&dir);
            let emails: Vec<String> = identity.emails.iter().map(|e| e.email.clone()).collect();
            println!("{}", serde_json::json!({ "emails": emails }));
            Some(())
        }
        // WINDOWS NATIVE HOOK PATH.
        //
        // Git for Windows can execute "<hook>.exe" directly, and that is the only
        // hook mechanism that works on this host (MSYS sh.exe cannot start). When
        // installed that way Git passes **no arguments at all** for post-commit
        // (and only the rewrite kind for post-rewrite), and no subcommand. The hook
        // name therefore has to come from our own file name.
        //
        // Missing this case is why an earlier run looked like "the hook never fired":
        // the helper exited successfully having done nothing.
        _ => {
            let inferred = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().to_string()))
                .filter(|stem| stem.starts_with("post-"));

            let hook_name = inferred?;
            let mut hook_args: Vec<String> = first.into_iter().collect();
            hook_args.extend(args);
            handle_hook(&hook_name, &hook_args)
        }
    }
}

fn handle_hook(hook_name: &str, hook_args: &[String]) -> Option<()> {
    let spool_dir = spool_dir()?;
    let cwd = std::env::current_dir().ok()?;

    // A hook that fires outside a repository (rare, but possible with odd hook
    // configurations) must do nothing rather than guess.
    let repo = Repository::discover(&cwd).ok()?;
    let facts = repo.head_commit().ok()?;

    let event_id = format!("{}:{}:{}", hook_name, facts.sha, now_millis());
    let payload = serde_json::json!({
        "event_id": event_id,
        "hook": hook_name,
        "hook_args": hook_args,
        "repo_git_dir": repo.git_dir.to_string_lossy(),
        "commit_sha": facts.sha,
        "parents": facts.parents,
        "author_name": facts.author_name,
        "author_email": facts.author_email,
        "committer_email": facts.committer_email,
        "subject": facts.subject,
        "occurred_ms": facts.committer_seconds * 1000,
    })
    .to_string();

    // Try IPC first; fall back to the Spool. Both are best-effort: a failure here
    // must not surface to the user as a broken commit.
    match deliver(&event_id, &payload) {
        ipc::Delivery::Acknowledged => Some(()),
        ipc::Delivery::Unreachable => {
            let spool = Spool::open(spool_dir).ok()?;
            let _ = spool.append(&event_id, &payload);
            Some(())
        }
    }
}

/// Attempt live delivery over IPC (spec 10.1: IPC first, Spool on failure).
///
/// Returns Acknowledged only when the Companion confirmed the record is durable.
/// Anything else -- no Companion, refused, timed out -- falls back to the Spool.
fn deliver(event_id: &str, payload: &str) -> ipc::Delivery {
    ipc::deliver_live(&user_discriminator(), event_id, payload)
}

/// A stable, per-user discriminator for the socket name.
///
/// Derived from the login name so the Helper and the Companion agree without a
/// shared config file, and so two accounts on one machine never collide (spec 10.2).
///
/// SECURITY NOTE: this is only a NAME, not a permission. The operating system's
/// ACL on the pipe is what actually prevents another user connecting; see ADR-0007.
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

fn spool_dir() -> Option<PathBuf> {
    // Uses the platform data directory so the Helper and Companion agree without
    // needing a config file on the hook path.
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("devchievements").join("spool"))
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

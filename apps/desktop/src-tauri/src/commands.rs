//! Tauri commands: the only way the window reaches the database.
//!
//! # Why every page goes through here
//!
//! Spec 34.2 requires each page to read the real database. Keeping the queries behind
//! commands means the UI cannot invent a number, and the same data is available to the
//! CLI diagnostics, so the window and the diagnostics view cannot disagree.

use serde::Serialize;
use tauri::Manager;

// Needed for creation_flags on Command: every process this binary starts on the
// user's behalf must not open a console window.
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{current_capabilities, default_data_dir, settings_path, update};

/// A page's data plus the facts every page header needs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardView {
    /// Aggregate counters.
    pub statistics: devchievements_storage::read_models::Statistics,
    /// The last two weeks of activity, newest first.
    pub recent: Vec<devchievements_storage::read_models::ActivityDay>,
    /// Database health.
    pub storage: devchievements_storage::read_models::StorageHealth,
    /// Where notifications can currently be shown.
    pub channel: String,
    /// Whether system notifications are usable.
    pub system_notifications: bool,
    /// Version of the companion.
    pub version: String,
}

/// One gallery card.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description shown on the card.
    pub description: String,
    /// Rarity, used for colour and ordering.
    pub rarity: String,
    /// Whether the achievement is hidden until unlocked.
    pub hidden: bool,
    /// Whether it has been unlocked.
    pub unlocked: bool,
    /// Unlock time, if unlocked.
    pub unlocked_at: Option<i64>,
    /// realtime | delayed | backfill | import.
    pub unlock_source: Option<String>,
    /// Whether the user has seen it; drives the new marker.
    pub viewed: bool,
}

/// Everything the About page shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutView {
    /// Companion version.
    pub version: String,
    /// Database health.
    pub storage: devchievements_storage::read_models::StorageHealth,
    /// Capability set, as measured.
    pub capabilities: devchievements_platform::Capabilities,
    /// Chosen presentation channel.
    pub channel: String,
    /// The privacy disclaimer the UI must show (spec 26.2).
    pub privacy_disclaimer: String,
    /// Sound ids that resolved to a file.
    pub sounds: Vec<String>,
    /// Outbox health.
    pub outbox: devchievements_storage::observability::OutboxHealth,
    /// Features that need the browser extension in Desktop Only mode (spec 34.1).
    pub needs_extension: Vec<String>,
}

async fn pool() -> Result<sqlx::SqlitePool, String> {
    let data_dir = default_data_dir();
    let pool = devchievements_storage::open(&data_dir.join("companion.db"))
        .await
        .map_err(|e| e.to_string())?;
    devchievements_storage::ingest::migrate(&pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(pool)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn channel_name() -> String {
    serde_json::to_value(current_capabilities().channel())
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "in_app_only".to_string())
}

#[tauri::command]
pub async fn get_dashboard() -> Result<DashboardView, String> {
    let pool = pool().await?;
    let statistics = devchievements_storage::read_models::statistics(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let recent = devchievements_storage::read_models::activity(&pool, 14)
        .await
        .map_err(|e| e.to_string())?;

    let data_dir = default_data_dir();
    let storage = devchievements_storage::read_models::storage_health(
        &pool,
        devchievements_privacy::redact_path(
            &data_dir.to_string_lossy(),
            std::env::var("USERPROFILE").ok().as_deref(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(DashboardView {
        statistics,
        recent,
        storage,
        channel: channel_name(),
        system_notifications: crate::toast::registration_state().is_ready(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn get_gallery() -> Result<Vec<GalleryItem>, String> {
    let pool = pool().await?;

    // The catalogue comes from the rule set, so locked achievements appear as goals.
    let builtins = devchievements_achievements::builtin_achievements();
    let ids: Vec<String> = builtins.iter().map(|a| a.achievement_id.clone()).collect();

    let entries = devchievements_storage::read_models::gallery(&pool, &ids)
        .await
        .map_err(|e| e.to_string())?;

    Ok(entries
        .into_iter()
        .map(|entry| {
            let meta = builtins
                .iter()
                .find(|a| a.achievement_id == entry.achievement_id);
            GalleryItem {
                id: entry.achievement_id.clone(),
                name: meta
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| entry.achievement_id.clone()),
                description: meta.map(|a| a.description.clone()).unwrap_or_default(),
                rarity: meta
                    .map(|a| format!("{:?}", a.rarity).to_lowercase())
                    .unwrap_or_else(|| "common".to_string()),
                hidden: meta.map(|a| a.hidden).unwrap_or(false),
                unlocked: entry.unlocked,
                unlocked_at: entry.unlocked_at,
                unlock_source: entry.unlock_source,
                viewed: entry.viewed,
            }
        })
        .collect())
}

/// Clear the new marker on the given achievements.
#[tauri::command]
pub async fn mark_achievements_viewed(ids: Vec<String>) -> Result<u64, String> {
    let pool = pool().await?;
    devchievements_storage::read_models::mark_viewed(&pool, &ids, now_ms())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_activity(
    limit: Option<i64>,
) -> Result<Vec<devchievements_storage::read_models::ActivityDay>, String> {
    let pool = pool().await?;
    devchievements_storage::read_models::activity(&pool, limit.unwrap_or(60))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_repositories(
) -> Result<Vec<devchievements_storage::read_models::RepositoryEntry>, String> {
    let pool = pool().await?;
    devchievements_storage::read_models::repositories(&pool, devchievements_privacy::stable_alias)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_about() -> Result<AboutView, String> {
    let pool = pool().await?;
    let data_dir = default_data_dir();
    let storage = devchievements_storage::read_models::storage_health(
        &pool,
        devchievements_privacy::redact_path(
            &data_dir.to_string_lossy(),
            std::env::var("USERPROFILE").ok().as_deref(),
        ),
    )
    .await
    .map_err(|e| e.to_string())?;

    let outbox = devchievements_storage::observability::outbox_health(&pool, now_ms())
        .await
        .map_err(|e| e.to_string())?;

    Ok(AboutView {
        version: env!("CARGO_PKG_VERSION").to_string(),
        storage,
        capabilities: current_capabilities(),
        channel: channel_name(),
        privacy_disclaimer: devchievements_privacy::PRIVACY_MODE_DISCLAIMER.to_string(),
        sounds: crate::audio::available_sound_ids(),
        outbox,
        // Spec 34.1: Desktop Only mode must say which features need the extension
        // rather than hiding them.
        needs_extension: vec![
            "GitHub pull requests".to_string(),
            "GitHub releases".to_string(),
            "GitHub issues".to_string(),
            "Browser notifications".to_string(),
        ],
    })
}

/// Where a backup should go, derived so the user does not have to pick a folder.
#[tauri::command]
pub async fn backup_database() -> Result<devchievements_storage::backup::BackupReport, String> {
    let pool = pool().await?;
    let stamp = now_ms();
    let destination = default_data_dir()
        .join("backups")
        .join(format!("companion-{stamp}.db"));
    devchievements_storage::backup::backup(&pool, &destination)
        .await
        .map_err(|e| e.to_string())
}

/// Restore from a backup file. Only safe while nothing else is writing.
#[tauri::command]
pub async fn restore_database(path: String) -> Result<(), String> {
    let data_dir = default_data_dir();
    devchievements_storage::backup::restore(
        std::path::Path::new(&path),
        &data_dir.join("companion.db"),
    )
    .map_err(|e| e.to_string())
}

/// Import one repository's history from a folder the user picked (spec 11.1).
///
/// Silent by construction: the storage layer suppresses per-commit notifications,
/// so an import of ten thousand commits cannot flood the user.
#[tauri::command]
pub async fn import_history(
    path: String,
) -> Result<devchievements_storage::history_scan::ImportReport, String> {
    let repo_dir = std::path::PathBuf::from(&path);
    if !repo_dir.is_dir() {
        return Err(format!("not a directory: {path}"));
    }

    let stored = crate::settings::Settings::load(&settings_path());
    let identity = stored.to_identity();
    if !identity.is_configured() {
        return Err("configure a git email before importing (spec 11.2)".to_string());
    }

    let pool = pool().await?;
    // Same id the hook-captured path uses, so both land on one repository.
    let repository_id = format!("local:{}", repo_dir.to_string_lossy().replace('\\', "/"));

    let clock = crate::FixedClock;
    let tz = devchievements_core::BundledTimezoneProvider::new();
    let mut ids = crate::PlainIds::default();
    let cfg = crate::StaticConfig;
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

    devchievements_storage::history_scan::import_repository(
        &pool,
        &repository_id,
        &repo_dir,
        &identity,
        &options,
        &std::sync::atomic::AtomicBool::new(false),
        &ctx,
    )
    .await
    .map_err(|e| e.to_string())
}

// ------------------------------------------------------------- onboarding
//
// Spec 25: the first run must explain what the companion needs and why, rather
// than starting silently and recording nothing.

/// What the wizard needs to know.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    /// Whether the wizard has been completed before.
    pub completed: bool,
    /// Emails Git already knows about, offered as suggestions.
    pub suggested_emails: Vec<String>,
    /// Whether Git itself has a user.email configured.
    pub git_has_identity: bool,
    /// Whether the helper binary was found next to the app.
    pub helper_available: bool,
}

#[tauri::command]
pub fn onboarding_status() -> OnboardingStatus {
    let stored = crate::settings::Settings::load(&settings_path());

    // Global Git identity is what a new user most likely wants; reading it from the
    // home directory picks up the global config without needing a repository.
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let identity = devchievements_git_hooks::gitread::config_read::suggest_identity(&home);
    let suggested: Vec<String> = identity.emails.into_iter().map(|e| e.email).collect();

    let helper_available = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("devch-helper.exe").is_file()))
        .unwrap_or(false);

    OnboardingStatus {
        completed: stored.onboarding_completed,
        suggested_emails: suggested,
        git_has_identity: devchievements_git_hooks::gitread::config_read::git_has_identity(&home),
        helper_available,
    }
}

/// One hook the installer handled.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInstallResult {
    /// Hook name.
    pub hook: String,
    /// What happened, as the installer reported it.
    pub outcome: String,
}

/// Install the managed hooks into one repository.
#[tauri::command]
pub fn install_hooks(repo_path: String) -> Result<Vec<HookInstallResult>, String> {
    let repo = std::path::PathBuf::from(&repo_path);
    let git_dir = repo.join(".git");
    if !git_dir.is_dir() {
        return Err(format!("not a git repository: {repo_path}"));
    }

    let helper = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(|p| p.join("devch-helper.exe"))
        .ok_or_else(|| "cannot locate the companion directory".to_string())?;
    if !helper.is_file() {
        return Err("devch-helper.exe is missing next to the app".to_string());
    }

    let site = devchievements_git_hooks::hooks::HookSite::resolve(&git_dir, None);
    let mut results = Vec::new();
    for name in ["post-commit", "post-rewrite", "post-merge"] {
        match devchievements_git_hooks::hooks::install(&site, name, &helper) {
            Ok(outcome) => results.push(HookInstallResult {
                hook: name.to_string(),
                outcome: format!("{outcome:?}"),
            }),
            Err(e) => return Err(format!("{name}: {e}")),
        }
    }

    // The helper refuses to create the data directory (spec 34.1); the installer
    // owns that, so do it here before any commit can happen.
    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(data_dir.join("spool")).map_err(|e| e.to_string())?;

    Ok(results)
}

/// Report a JS error to the process log.
///
/// Without this a renderer exception leaves a blank window and nothing else: the
/// console of a webview is not something a user can open, and the Rust side sees
/// only that no view was ever reported ready.
#[tauri::command]
pub fn ui_error(message: String) {
    println!("UI ERROR {message}");
}

/// Open a URL in the user's default browser.
///
/// Only http(s) reaches the shell: the URL comes from the preflight report, but a
/// renderer that has been compromised should not be able to launch `file:` or a
/// custom scheme through this door (AGENTS.md 7.4: validate what crosses the bridge).
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    let lowered = url.trim().to_ascii_lowercase();
    if !(lowered.starts_with("https://") || lowered.starts_with("http://")) {
        return Err("only http and https links can be opened".to_string());
    }
    #[cfg(windows)]
    {
        // explorer.exe is the documented way to hand a URL to the shell without a
        // second crate; it launches the default browser for http(s).
        std::process::Command::new("explorer.exe")
            .arg(url.trim())
            .creation_flags(0x0800_0000)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("opening links is only implemented on Windows".to_string())
    }
}

/// Write a diagnostics document into the data directory and report where it went.
///
/// The window cannot silently choose the raw form: `raw` is a parameter the caller
/// has to ask for, and the reply carries the sentence the UI shows afterwards.
#[tauri::command]
pub fn export_diagnostics(raw: bool) -> Result<serde_json::Value, String> {
    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let report = crate::diagnostics_report(&data_dir, raw)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = if raw {
        format!("diagnostics-raw-{stamp}.json")
    } else {
        format!("diagnostics-{stamp}.json")
    };
    let path = data_dir.join(&name);
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    println!("DIAGNOSTICS EXPORTED path={} raw={raw}", path.display());
    Ok(serde_json::json!({
        "path": path.to_string_lossy(),
        "raw": raw,
    }))
}

/// Report which prerequisites this machine has (spec 25).
///
/// Advisory, never blocking: a missing Git is reported loudly, but the app still
/// opens so the user can see what the product is before being sent to an
/// installer.
#[tauri::command]
pub fn check_dependencies() -> crate::preflight::Preflight {
    let report = crate::preflight::check();
    for dependency in &report.dependencies {
        println!(
            "PREFLIGHT {} ok={} version={}",
            dependency.id,
            dependency.ok,
            dependency.version.as_deref().unwrap_or("none")
        );
    }
    report
}

/// Record which language the window actually rendered in.
///
/// The setting can say `system`; only the page can resolve that, because the answer
/// lives in the webview's locale. Logging the resolved value makes "the UI is in the
/// wrong language" diagnosable, and gives the end-to-end tests something to assert
/// instead of trusting the setting file.
///
/// `sample` is one already-translated word. Sending it means the log shows the
/// dictionary was actually consulted, which no assertion on the setting can prove.
#[tauri::command]
pub fn ui_language(lang: String, sample: String) {
    println!("UI LANGUAGE lang={lang} sample={sample}");
}

/// Called by the popup page once its first frame has been painted (spec 44.4).
///
/// The latency budget for "commit to notification" ends here. Nothing in Rust can
/// know when WebView2 painted, so the page says so, once, and the timestamp is
/// taken on arrival: a page that never calls this makes the measurement fail
/// rather than quietly reporting a number that stops at window creation.
#[tauri::command]
pub fn popup_painted() {
    println!("POPUP painted at={}", crate::now_millis());
}

/// Report the update situation without pretending to have checked (spec 40.3).
#[tauri::command]
pub fn update_status() -> serde_json::Value {
    update::status(update::ENDPOINTS.len())
}

/// Copy the database aside before an update installs (spec 40.3).
///
/// The UI calls this before handing control to the updater; without it a failed
/// migration would have no way back.
#[tauri::command]
pub fn prepare_update() -> Result<serde_json::Value, String> {
    let data_dir = default_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let snapshot = update::snapshot_database(&data_dir)?;
    println!(
        "UPDATE prepared snapshot={}",
        snapshot
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    Ok(serde_json::json!({
        "snapshot": snapshot.map(|p| p.to_string_lossy().to_string()),
    }))
}

/// Drop the pre-update copy after the new version has proven itself.
#[tauri::command]
pub fn clear_update_snapshot() -> Result<(), String> {
    update::clear_snapshot(&default_data_dir())
}

/// Clear the "setup done" flag so the wizard runs again.
///
/// Deliberately not a data reset: re-running setup is about identity and hooks, and
/// an event that says so must not sound like it will delete history. The flag is
/// the only thing this touches, and the caller reloads the window afterwards.
#[tauri::command]
pub fn reset_onboarding() -> Result<(), String> {
    let path = settings_path();
    let mut stored = crate::settings::Settings::load(&path);
    stored.onboarding_completed = false;
    stored.save(&path).map_err(|e| e.to_string())?;
    println!("ONBOARDING reset");
    Ok(())
}

/// Mark the wizard as done.
#[tauri::command]
pub fn complete_onboarding() -> Result<(), String> {
    let path = settings_path();
    let mut stored = crate::settings::Settings::load(&path);
    stored.onboarding_completed = true;
    stored.save(&path).map_err(|e| e.to_string())?;
    println!("ONBOARDING completed");
    Ok(())
}

/// Open the data directory in the file manager.
#[tauri::command]
pub fn open_data_folder() -> Result<(), String> {
    let dir = default_data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Opening a folder is a shell action, not something to reimplement.
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Delete every recorded statistic, keeping the settings file.
///
/// This is the destructive half of data management, so it is a separate command
/// that the UI only calls after an explicit confirmation.
#[tauri::command]
pub async fn clear_data() -> Result<(), String> {
    let dir = default_data_dir();
    let db = dir.join("companion.db");

    // Deleting while a pool has the file open fails on Windows, so the files are
    // removed rather than truncated.
    for suffix in ["", "-wal", "-shm"] {
        let path = std::path::PathBuf::from(format!("{}{suffix}", db.to_string_lossy()));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }

    let spool = dir.join("spool");
    if spool.exists() {
        std::fs::remove_dir_all(&spool).map_err(|e| e.to_string())?;
    }

    println!("DATA cleared");
    Ok(())
}

/// Render one popup with sample content, using the saved appearance.
///
/// Saves nothing itself: the caller saves first, so what the preview shows is what
/// the next real notification would use.
#[tauri::command]
pub fn show_popup_preview(app: tauri::AppHandle) -> Result<(), String> {
    let payload = crate::popup::PopupPayload {
        title: "Preview".to_string(),
        body: "This is how your notifications will look".to_string(),
        rarity: "Rare".to_string(),
        others: vec![],

        // Filled from the saved style inside popup::show, so a caller cannot get it wrong.
        style: Default::default(),
        icon_url: crate::popup::icon_for("preview"),
    };
    let handle = app.clone();
    app.run_on_main_thread(move || match crate::popup::show(&handle, &payload, 1) {
        Ok(()) => println!("POPUP preview shown"),
        Err(e) => println!("POPUP preview failed: {e}"),
    })
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- assets
//
// Imported files are untrusted input (spec 20). The dialog picks a path; the
// bytes are validated by the assets crate before anything is stored, so a file
// cannot enter the app on the strength of its name.

/// One stored asset.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAsset {
    /// File name inside the assets directory.
    pub name: String,
    /// Detected format.
    pub format: String,
    /// Size in bytes.
    pub bytes: i64,
}

/// Where user assets live.
fn assets_dir() -> std::path::PathBuf {
    default_data_dir().join("assets")
}

/// Everything currently stored, newest name last.
#[tauri::command]
pub fn list_assets() -> Vec<StoredAsset> {
    let dir = assets_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut assets: Vec<StoredAsset> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let path = e.path();
            let bytes = std::fs::read(&path).ok()?;
            let format = devchievements_assets::detect(&bytes)?;
            Some(StoredAsset {
                name: e.file_name().to_string_lossy().to_string(),
                format: format.extension().to_string(),
                bytes: bytes.len() as i64,
            })
        })
        .collect();
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    assets
}

/// Import one file into the assets directory.
///
/// The dialog provides the path; the bytes decide whether it is accepted.
#[tauri::command]
pub fn import_asset(path: String) -> Result<StoredAsset, String> {
    let source = std::path::Path::new(&path);
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;

    let dir = assets_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Quota is checked against what is already stored, so the total cannot creep
    // past the limit one file at a time.
    let current: u64 = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok().map(|m| m.len()))
                .sum()
        })
        .unwrap_or(0);

    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "the selected file has no name".to_string())?;

    let accepted =
        devchievements_assets::accept(&name, &bytes, current).map_err(|e| e.to_string())?;

    let destination = dir.join(&name);
    std::fs::write(&destination, &bytes).map_err(|e| e.to_string())?;

    println!("ASSET imported name={name} format={:?}", accepted.format);
    Ok(StoredAsset {
        name,
        format: accepted.format.extension().to_string(),
        bytes: accepted.bytes as i64,
    })
}

/// Remove a stored asset.
#[tauri::command]
pub fn remove_asset(name: String) -> Result<(), String> {
    // Only a bare file name is accepted: a path would let the UI delete anything.
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("asset name must be a plain file name".to_string());
    }
    let path = assets_dir().join(&name);
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// Import a sound file into the assets directory.
///
/// Same validation as any asset: the bytes decide the type, so a renamed document
/// is refused. Only audio formats are accepted here, because this command is the
/// one the Sound Manager calls.
#[tauri::command]
pub fn import_sound(path: String) -> Result<StoredAsset, String> {
    let source = std::path::Path::new(&path);
    let bytes = std::fs::read(source).map_err(|e| e.to_string())?;

    let dir = assets_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let current: u64 = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok().map(|m| m.len()))
                .sum()
        })
        .unwrap_or(0);

    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| "the selected file has no name".to_string())?;

    let accepted =
        devchievements_assets::accept(&name, &bytes, current).map_err(|e| e.to_string())?;

    // A sound manager must not quietly accept an image.
    if accepted.format.is_image() {
        return Err(format!("{name} is an image, not a sound"));
    }

    let destination = dir.join(&name);
    std::fs::write(&destination, &bytes).map_err(|e| e.to_string())?;

    println!("SOUND imported name={name}");
    Ok(StoredAsset {
        name,
        format: accepted.format.extension().to_string(),
        bytes: accepted.bytes as i64,
    })
}

/// Import a sound from a path, for the CLI and tests.
///
/// Also reports which sound ids are now available, so a caller can see the effect.
/// The dialog is the normal route; a test cannot click one.
#[tauri::command]
pub fn available_sounds() -> Vec<String> {
    crate::audio::available_sound_ids()
}

/// Open the settings window (used by the dashboard shortcut).
#[tauri::command]
pub fn open_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("main") {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }
    crate::open_main_window(&app)
}

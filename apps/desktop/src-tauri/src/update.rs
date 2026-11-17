//! Update safety net (spec 40.3).
//!
//! Tauri's updater already verifies the signature of every artifact it installs,
//! so the trust question is solved by the plugin plus the pubkey in
//! `tauri.conf.json`. What the plugin cannot know is that this app keeps a SQLite
//! database that a new version may migrate.
//!
//! So the rule implemented here is: before a version change is allowed to touch
//! the database, the database is copied aside. The copy is deleted only after the
//! new version has opened and migrated it successfully. If the open or the
//! migration fails, the copy is put back, which turns "a bad update" into "an
//! update that did not happen" instead of "a user who lost their history".

use std::path::{Path, PathBuf};

/// Update feed endpoints (spec 40.3).
///
/// Empty on a local build: there is no published feed yet, and inventing a URL
/// would make `configured: true` a lie. `tauri.conf.json` carries the same list,
/// and `the_endpoint_list_matches_the_config` keeps the two from drifting.
pub(crate) const ENDPOINTS: &[&str] = &[];

/// Name of the pre-update copy, next to the database it protects.
const SNAPSHOT_NAME: &str = "companion.db.pre-update";

/// Where the pre-update copy lives for a given data directory.
pub(crate) fn snapshot_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SNAPSHOT_NAME)
}

/// Copy the database aside before an update.
///
/// Returns `Ok(None)` when there is no database yet: a fresh install has nothing
/// to protect, and creating an empty snapshot would make the rollback path think
/// there is something to go back to.
pub(crate) fn snapshot_database(data_dir: &Path) -> Result<Option<PathBuf>, String> {
    let live = data_dir.join("companion.db");
    if !live.exists() {
        return Ok(None);
    }
    let target = snapshot_path(data_dir);
    std::fs::copy(&live, &target).map_err(|e| format!("copy {}: {e}", live.display()))?;
    Ok(Some(target))
}

/// Whether a pre-update copy is waiting to be either confirmed or restored.
pub(crate) fn has_snapshot(data_dir: &Path) -> bool {
    snapshot_path(data_dir).exists()
}

/// Put the pre-update copy back over the live database.
///
/// The live file is removed first: SQLite is asked to overwrite a file it may
/// still have open, and on Windows a copy onto an open file fails outright.
pub(crate) fn restore_snapshot(data_dir: &Path) -> Result<(), String> {
    let snapshot = snapshot_path(data_dir);
    if !snapshot.exists() {
        return Err("no pre-update snapshot to restore".to_string());
    }
    let live = data_dir.join("companion.db");
    for suffix in ["-wal", "-shm", ""] {
        let path = if suffix.is_empty() {
            live.clone()
        } else {
            PathBuf::from(format!("{}{suffix}", live.display()))
        };
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
        }
    }
    std::fs::copy(&snapshot, &live).map_err(|e| format!("restore {}: {e}", live.display()))?;
    std::fs::remove_file(&snapshot).map_err(|e| format!("remove {}: {e}", snapshot.display()))?;
    Ok(())
}

/// Drop the pre-update copy: the new version started and migrated successfully.
pub(crate) fn clear_snapshot(data_dir: &Path) -> Result<(), String> {
    let snapshot = snapshot_path(data_dir);
    if !snapshot.exists() {
        return Ok(());
    }
    std::fs::remove_file(&snapshot).map_err(|e| format!("remove {}: {e}", snapshot.display()))
}

/// What the UI and `--update-status` report.
///
/// `configured` is false when no endpoint is set in `tauri.conf.json`. That is the
/// honest answer for a local build: there is no feed to ask, so no check happens
/// and nothing pretends to have checked.
pub(crate) fn status(endpoints: usize) -> serde_json::Value {
    let data_dir = crate::default_data_dir();
    serde_json::json!({
        "configured": endpoints > 0,
        "endpoints": endpoints,
        "currentVersion": env!("CARGO_PKG_VERSION"),
        "snapshotPresent": has_snapshot(&data_dir),
        "snapshotPath": snapshot_path(&data_dir).to_string_lossy(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("devch-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_fresh_install_has_nothing_to_snapshot() {
        let dir = scratch("fresh");
        assert!(!has_snapshot(&dir));
        assert_eq!(snapshot_database(&dir).expect("snapshot"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_snapshot_is_a_copy_not_a_move() {
        let dir = scratch("copy");
        std::fs::write(dir.join("companion.db"), b"v1").expect("write");
        let snapshot = snapshot_database(&dir).expect("snapshot").expect("some");
        assert!(has_snapshot(&dir));
        assert_eq!(std::fs::read(&snapshot).expect("read"), b"v1");
        assert_eq!(
            std::fs::read(dir.join("companion.db")).expect("read live"),
            b"v1"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restoring_brings_back_the_old_database_and_drops_the_copy() {
        let dir = scratch("restore");
        std::fs::write(dir.join("companion.db"), b"v1").expect("write");
        snapshot_database(&dir).expect("snapshot");
        std::fs::write(dir.join("companion.db"), b"broken after update").expect("write");
        restore_snapshot(&dir).expect("restore");
        assert_eq!(
            std::fs::read(dir.join("companion.db")).expect("read"),
            b"v1"
        );
        assert!(!has_snapshot(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_successful_start_drops_the_copy_without_touching_the_database() {
        let dir = scratch("clear");
        std::fs::write(dir.join("companion.db"), b"v1").expect("write");
        snapshot_database(&dir).expect("snapshot");
        std::fs::write(dir.join("companion.db"), b"v2 migrated").expect("write");
        clear_snapshot(&dir).expect("clear");
        assert!(!has_snapshot(&dir));
        assert_eq!(
            std::fs::read(dir.join("companion.db")).expect("read"),
            b"v2 migrated"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_endpoint_list_matches_the_config() {
        let raw = include_str!("../tauri.conf.json");
        let config: serde_json::Value = serde_json::from_str(raw).expect("tauri.conf.json parses");
        let configured = config["plugins"]["updater"]["endpoints"]
            .as_array()
            .expect("plugins.updater.endpoints is an array");
        assert_eq!(
            configured.len(),
            ENDPOINTS.len(),
            "tauri.conf.json and update::ENDPOINTS disagree about the update feed"
        );
        assert!(
            !config["plugins"]["updater"]["pubkey"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "spec 40.3: updates are signature-verified, so a pubkey is required"
        );
    }

    #[test]
    fn restoring_without_a_copy_is_an_error_not_a_silent_no_op() {
        let dir = scratch("nothing");
        assert!(restore_snapshot(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

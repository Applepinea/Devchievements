//! Keep the Git hook helper somewhere a hook is allowed to run it.
//!
//! # Why the helper cannot stay next to the executable
//!
//! A hook is started by git, which is not the app and has no relationship with the
//! package. Measured on 2026-11-17 with a real MSIX installed: executing
//! `devch-helper.exe` from `C:\Program Files\WindowsApps\...` fails with access
//! denied, while the same file copied to `%LOCALAPPDATA%` runs and records normally.
//! The restriction lives above the ACL (it is part of the package model), so no
//! permission change can lift it.
//!
//! The fix is therefore a copy: the packaged helper is copied into the data
//! directory, hooks point at the copy, and both distribution routes -- an .exe
//! installer and an MSIX -- behave the same way. The copy is refreshed when the
//! packaged file changes, so an update to the app updates what commits invoke.

use std::path::{Path, PathBuf};

/// Folder inside the data directory that holds the runnable helper.
pub(crate) const BIN_DIR: &str = "bin";

/// Name of the helper executable.
pub(crate) const HELPER_NAME: &str = "devch-helper.exe";

/// Where the helper shipped with the app lives.
pub(crate) fn packaged_helper() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(HELPER_NAME);
    candidate.is_file().then_some(candidate)
}

/// Where the runnable copy lives.
pub(crate) fn managed_helper(data_dir: &Path) -> PathBuf {
    data_dir.join(BIN_DIR).join(HELPER_NAME)
}

/// Whether `target` already holds the same bytes description as `source`.
///
/// Length plus modification time rather than a hash: the files are the same
/// executable copied moments apart, and hashing a few hundred kilobytes on every
/// launch to answer the same question is work with no benefit.
fn is_current(source: &Path, target: &Path) -> bool {
    let (Ok(source_meta), Ok(target_meta)) = (std::fs::metadata(source), std::fs::metadata(target))
    else {
        return false;
    };
    if source_meta.len() != target_meta.len() {
        return false;
    }
    match (source_meta.modified(), target_meta.modified()) {
        (Ok(source_time), Ok(target_time)) => source_time <= target_time,
        // A filesystem without timestamps cannot answer the question, so refresh.
        _ => false,
    }
}

/// Make sure a runnable helper exists, and return where it is.
///
/// Returns `Err` with a sentence a user could act on: the packaged file is missing,
/// or the copy failed. A hook that points at nothing would break every commit.
pub(crate) fn ensure(data_dir: &Path) -> Result<PathBuf, String> {
    let target = managed_helper(data_dir);
    let source = packaged_helper();

    if let Some(source) = &source {
        if is_current(source, &target) {
            return Ok(target);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        // Copy aside first, then rename: a half-written executable that a commit
        // happens to run is worse than no helper at all.
        let staging = target.with_extension("exe.tmp");
        std::fs::copy(source, &staging)
            .map_err(|e| format!("cannot copy the helper from {}: {e}", source.display()))?;
        std::fs::rename(&staging, &target)
            .map_err(|e| format!("cannot replace {}: {e}", target.display()))?;
        println!("HELPER synced from package to {}", target.display());
        return Ok(target);
    }

    // No packaged helper: a development build or a partial install may only have
    // the copy. It is only acceptable if it is already there.
    if target.is_file() {
        return Ok(target);
    }
    Err(format!(
        "the hook helper is missing: no {} next to the app and no copy at {}",
        HELPER_NAME,
        target.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("devch-helper-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn the_managed_helper_lives_under_the_data_directory() {
        let dir = Path::new(r"C:\data\devchievements");
        assert_eq!(
            managed_helper(dir),
            Path::new(r"C:\data\devchievements\bin\devch-helper.exe")
        );
    }

    #[test]
    fn a_newer_packaged_helper_replaces_an_older_copy() {
        let dir = scratch("refresh");
        let source = dir.join("packaged.exe");
        let target = dir.join("bin").join("copy.exe");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("bin dir");
        std::fs::write(&target, b"old").expect("old copy");
        // Make the packaged file unambiguously newer.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&source, b"new and longer").expect("new copy");
        assert!(!is_current(&source, &target));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_identical_copy_is_not_rewritten() {
        let dir = scratch("same");
        let source = dir.join("packaged.exe");
        let target = dir.join("copy.exe");
        std::fs::write(&source, b"same bytes").expect("source");
        std::fs::write(&target, b"same bytes").expect("target");
        // Written after the source, so its timestamp is not older.
        assert!(is_current(&source, &target));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_copy_is_never_current() {
        let dir = scratch("missing");
        let source = dir.join("packaged.exe");
        std::fs::write(&source, b"x").expect("source");
        assert!(!is_current(&source, &dir.join("absent.exe")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

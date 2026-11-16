//! Backup and restore of the database (spec 26.4).
//!
//! # Why VACUUM INTO
//!
//! Copying the file while the app runs can capture a torn write. VACUUM INTO asks
//! SQLite for a consistent snapshot into a new file, which is exactly the guarantee a
//! backup needs and costs one statement.
//!
//! # Why restore is not automatic
//!
//! Replacing the database under a running process is not safe, so restore is meant to
//! run while the app is closed. It refuses any file that does not look like a SQLite
//! database, because overwriting user data with the wrong file is unrecoverable.

use std::path::Path;

use sqlx::SqlitePool;

use crate::StorageResult;

/// What a backup produced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackupReport {
    /// Where it was written.
    pub path: String,
    /// Size of the written file.
    pub bytes: i64,
}

/// Write a consistent snapshot of the database to the given destination.
pub async fn backup(pool: &SqlitePool, destination: &Path) -> StorageResult<BackupReport> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| crate::StorageError::DirectoryUnavailable(e.to_string()))?;
    }
    // VACUUM INTO refuses to overwrite, and silently replacing an older backup would
    // destroy it, so an existing file is removed deliberately rather than by accident.
    if destination.exists() {
        std::fs::remove_file(destination)
            .map_err(|e| crate::StorageError::DirectoryUnavailable(e.to_string()))?;
    }

    let dest = destination.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{dest}'"))
        .execute(pool)
        .await?;

    let bytes = std::fs::metadata(destination)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    Ok(BackupReport {
        path: destination.to_string_lossy().to_string(),
        bytes,
    })
}

/// Why a restore was refused.
#[derive(Debug)]
pub enum RestoreError {
    /// The source file does not start with the SQLite header.
    NotADatabase(String),
    /// The file could not be copied.
    Io(String),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestoreError::NotADatabase(p) => write!(f, "not a SQLite database: {p}"),
            RestoreError::Io(m) => write!(f, "restore io error: {m}"),
        }
    }
}

impl std::error::Error for RestoreError {}

/// Replace the target database with the contents of the source file.
///
/// Refuses anything that is not a SQLite file, because overwriting the user's data with
/// an arbitrary file is unrecoverable.
pub fn restore(source: &Path, target: &Path) -> Result<(), RestoreError> {
    use std::io::Read;

    let mut header = [0u8; 16];
    let mut file = std::fs::File::open(source).map_err(|e| RestoreError::Io(e.to_string()))?;
    file.read_exact(&mut header)
        .map_err(|e| RestoreError::Io(e.to_string()))?;
    if &header != b"SQLite format 3\0" {
        return Err(RestoreError::NotADatabase(
            source.to_string_lossy().to_string(),
        ));
    }
    drop(file);

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| RestoreError::Io(e.to_string()))?;
    }
    // Stage beside the target, so an interrupted restore cannot leave a half-written
    // database in place.
    let staging = target.with_extension("restore-tmp");
    std::fs::copy(source, &staging).map_err(|e| RestoreError::Io(e.to_string()))?;
    std::fs::rename(&staging, target).map_err(|e| RestoreError::Io(e.to_string()))?;
    Ok(())
}

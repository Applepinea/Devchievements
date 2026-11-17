//! Reading a repository's history and importing it (spec 11).
//!
//! # Why the walk shells out to git
//!
//! The hook path reads objects directly because it runs on every commit (ADR-0002).
//! An import runs once per repository and walks thousands of commits, so one git
//! process returning the whole log is both simpler and faster than opening the pack for
//! every object. This is a deliberate exception, confined to this module.
//!
//! # What makes import safe to repeat
//!
//! Two things: the event id is derived from the commit ("import:<repo>:<sha>"), and
//! ingestion dedups on (repository, sha, event type). Re-running an import therefore
//! adds nothing, which is what spec 11.2 asks for. Resumability falls out of the same
//! property: a cancelled import can simply be run again.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use devchievements_core::InjectedContext;
use devchievements_events::Identity;
use sqlx::SqlitePool;

use crate::history_import::{begin_batch, build_import_event, finish_batch, BatchOutcome};
use crate::ingest::{self, IngestOutcome};
use crate::projections;

/// One commit as read from the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedCommit {
    /// Commit id.
    pub sha: String,
    /// How many parents it has; two or more means a merge.
    pub parent_count: usize,
    /// Author email, used for the identity filter.
    pub author_email: String,
    /// Committer timestamp in milliseconds.
    pub committed_at_ms: i64,
}

/// How an import behaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    /// How many commits to ingest before closing a batch record.
    pub batch_size: usize,
    /// IANA zone used for local_date (spec 12).
    pub timezone: String,
    /// Day boundary in minutes from local midnight.
    pub day_start_minutes: u16,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            // Large enough to keep batch bookkeeping cheap, small enough that a
            // cancelled import loses little work.
            batch_size: 200,
            timezone: "UTC".to_string(),
            day_start_minutes: 0,
        }
    }
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImportReport {
    /// Commits found in the repository.
    pub scanned: usize,
    /// Commits whose author matched a configured email.
    pub matched: usize,
    /// Commits actually recorded.
    pub ingested: usize,
    /// Commits already known, from live capture or a previous import.
    pub duplicates: usize,
    /// Commits refused by the identity gate.
    pub refused: usize,
    /// Batch records written.
    pub batches: usize,
    /// Whether the caller asked to stop early.
    pub cancelled: bool,
}

/// Read a repository's history.
///
/// Uses an explicit separator so a signed commit cannot inject extra lines into the
/// output, and a single git process for the whole walk.
pub fn scan_repository(repo_dir: &Path) -> Result<Vec<ScannedCommit>, String> {
    use std::process::Command;

    // 0x1f separates fields and 0x1e separates records. The subject is deliberately
    // omitted: importing does not use it, and it needlessly enlarges Git's output for
    // large histories.
    let format = "--format=%H%x1f%P%x1f%ae%x1f%ct%x1e";
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args([
            "-c",
            "core.quotepath=false",
            "--no-pager",
            "log",
            "--no-show-signature",
            format,
            "HEAD",
        ])
        .output()
        .map_err(|e| format!("git log failed to start: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for record in text.split('\u{1e}') {
        let record = record.trim_matches(['\n', '\r']);
        if record.is_empty() {
            continue;
        }
        let mut fields = record.split('\u{1f}');
        let (Some(sha), Some(parents), Some(email), Some(seconds)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };

        let Ok(seconds) = seconds.trim().parse::<i64>() else {
            continue;
        };

        commits.push(ScannedCommit {
            sha: sha.trim().to_string(),
            parent_count: parents.split_whitespace().count(),
            author_email: email.trim().to_string(),
            committed_at_ms: seconds.saturating_mul(1000),
        });
    }

    Ok(commits)
}

/// Import one repository's history (spec 11.2).
///
/// Silent by construction: it calls the ingestion path that suppresses outbox rows, so
/// no per-commit notification is queued. Achievements still unlock and still record
/// their source and rule version.
pub async fn import_repository(
    pool: &SqlitePool,
    repository_id: &str,
    repo_dir: &Path,
    identity: &Identity,
    options: &ImportOptions,
    cancel: &AtomicBool,
    ctx: &InjectedContext<'_>,
) -> Result<ImportReport, String> {
    let scanned = scan_repository(repo_dir)?;
    let mut report = ImportReport {
        scanned: scanned.len(),
        ..Default::default()
    };

    // Spec 11.2: only commits authored by a configured email are imported.
    let batch_size = options.batch_size.max(1);
    report.matched = scanned
        .iter()
        .filter(|commit| identity.owns_email(&commit.author_email))
        .count();

    // Keep only one bounded chunk of references. The source scan remains a Vec for
    // now, but avoids an additional allocation proportional to the user's history.
    let mut batch = Vec::with_capacity(batch_size);
    let mut batch_index = 0usize;
    for commit in scanned
        .iter()
        .filter(|commit| identity.owns_email(&commit.author_email))
    {
        if batch.is_empty() && cancel.load(Ordering::SeqCst) {
            report.cancelled = true;
            break;
        }
        batch.push(commit);
        if batch.len() < batch_size {
            continue;
        }

        import_batch(
            pool,
            repository_id,
            &batch,
            batch_index,
            identity,
            options,
            ctx,
            &mut report,
        )
        .await?;
        batch_index += 1;
        batch.clear();
    }

    if !report.cancelled && !batch.is_empty() {
        if cancel.load(Ordering::SeqCst) {
            report.cancelled = true;
        } else {
            import_batch(
                pool,
                repository_id,
                &batch,
                batch_index,
                identity,
                options,
                ctx,
                &mut report,
            )
            .await?;
        }
    }

    // Spec 11.2: rebuild projections once the import finishes, so backfill and the live
    // counters agree.
    projections::rebuild_from_events(pool, ctx.clock.now_unix_millis())
        .await
        .map_err(|e| e.to_string())?;

    Ok(report)
}

#[allow(clippy::too_many_arguments)]
async fn import_batch(
    pool: &SqlitePool,
    repository_id: &str,
    commits: &[&ScannedCommit],
    index: usize,
    identity: &Identity,
    options: &ImportOptions,
    ctx: &InjectedContext<'_>,
    report: &mut ImportReport,
) -> Result<(), String> {
    let batch_id = format!("import:{repository_id}:{index}");
    begin_batch(pool, &batch_id, repository_id, ctx.clock.now_unix_millis())
        .await
        .map_err(|e| e.to_string())?;

    let mut outcome = BatchOutcome::default();
    for commit in commits {
        let imported = crate::history_import::ImportCommit {
            sha: commit.sha.clone(),
            author_email: commit.author_email.clone(),
            committed_at_ms: commit.committed_at_ms,
            parent_count: commit.parent_count,
            // The tree comparison is not available from a log walk; false means
            // "not known to be empty", which keeps a commit that might be empty.
            // Empty commits are excluded from counting by the engine, not here.
            tree_equals_parent: false,
        };
        let Some(event) = build_import_event(
            repository_id,
            &imported,
            &options.timezone,
            options.day_start_minutes,
            ctx,
        ) else {
            // A commit outside the supported date range is skipped rather than
            // given an invented date (spec 12).
            continue;
        };

        let facts = ingest::commit_facts(commit.parent_count, false);
        let now = ctx.clock.now_unix_millis();
        match ingest::ingest_import(pool, &event, &facts, now, identity, ctx).await {
            Ok(IngestOutcome::Ingested { .. }) | Ok(IngestOutcome::IngestedSilently) => {
                report.ingested += 1;
                outcome.imported += 1;
            }
            Ok(IngestOutcome::Duplicate) => {
                report.duplicates += 1;
                outcome.duplicates += 1;
            }
            Ok(IngestOutcome::Refused(_)) => report.refused += 1,
            Ok(IngestOutcome::Rejected(reason)) => {
                return Err(format!("ingest rejected {}: {reason}", commit.sha));
            }
            Err(e) => return Err(format!("ingest failed for {}: {e}", commit.sha)),
        }
    }

    finish_batch(pool, &batch_id, &outcome, ctx.clock.now_unix_millis())
        .await
        .map_err(|e| e.to_string())?;
    report.batches += 1;
    Ok(())
}

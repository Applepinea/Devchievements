//! Cross-checks the direct .git reader against real git output.
//!
//! WHY: ADR-0002 chose to parse git's on-disk format ourselves. That is only safe
//! if we continuously prove our parser agrees with git itself. These tests build
//! throw-away repositories with the system `git` (a test-only dependency; the
//! shipped Helper never spawns git) and compare field by field.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_git_hooks::gitread::Repository;
use std::path::Path;
use std::process::Command;

/// Run git in `dir` and return trimmed stdout, or None if git is missing.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn make_repo(name: &str) -> Option<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("devch-test-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).ok()?;
    git(&dir, &["init", "-q", "-b", "main"])?;
    git(&dir, &["config", "user.name", "Test User"])?;
    git(&dir, &["config", "user.email", "test@devchievements.local"])?;
    Some(dir)
}

fn commit(dir: &Path, message: &str, file: &str, content: &str) -> Option<()> {
    std::fs::write(dir.join(file), content).ok()?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", message])?;
    Some(())
}

#[test]
fn git_is_available_for_the_cross_checks() {
    // Makes an accidental "everything silently skipped" situation visible.
    if !git_available() {
        eprintln!("WARNING: git not found; cross-check tests will be skipped");
    }
}

#[test]
fn reader_matches_git_for_head_identity() {
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("head") else { return };
    if commit(&dir, "first commit", "a.txt", "hello\n").is_none() {
        return;
    }

    let repo = Repository::discover(&dir).expect("repository must be discovered");
    let facts = repo.head_commit().expect("HEAD commit must be readable");

    // Capture both sides FIRST. Inline comparison made the assertions depend on
    // argument evaluation order (the git() helper runs before the reader field is
    // read), which produced misleading empty-vs-sha failures.
    let want_sha = git(&dir, &["rev-parse", "HEAD"]).unwrap_or_default();
    let want_tree = git(&dir, &["rev-parse", "HEAD^{tree}"]).unwrap_or_default();
    let want_an = git(&dir, &["log", "-1", "--pretty=%an"]).unwrap_or_default();
    let want_ae = git(&dir, &["log", "-1", "--pretty=%ae"]).unwrap_or_default();
    let want_cn = git(&dir, &["log", "-1", "--pretty=%cn"]).unwrap_or_default();
    let want_subject = git(&dir, &["log", "-1", "--pretty=%s"]).unwrap_or_default();
    let want_ts = git(&dir, &["log", "-1", "--pretty=%at"]).unwrap_or_default();

    assert_eq!(facts.sha, want_sha, "sha");
    assert_eq!(facts.tree, want_tree, "tree");
    assert_eq!(facts.author_name, want_an, "author name");
    assert_eq!(facts.author_email, want_ae, "author email");
    assert_eq!(facts.committer_name, want_cn, "committer name");
    assert_eq!(facts.subject, want_subject, "subject");
    assert_eq!(
        facts.author_seconds.to_string(),
        want_ts,
        "author timestamp"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reader_distinguishes_merge_commits() {
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("merge") else {
        return;
    };
    if commit(&dir, "base", "a.txt", "a\n").is_none() {
        return;
    }
    if git(&dir, &["checkout", "-q", "-b", "feature"]).is_none() {
        return;
    }
    if commit(&dir, "feature", "b.txt", "b\n").is_none() {
        return;
    }
    if git(&dir, &["checkout", "-q", "main"]).is_none() {
        return;
    }
    if commit(&dir, "main", "c.txt", "c\n").is_none() {
        return;
    }
    if git(&dir, &["merge", "--no-ff", "--no-edit", "feature"]).is_none() {
        return;
    }

    let repo = Repository::discover(&dir).expect("repository must be discovered");
    let facts = repo.head_commit().expect("merge commit must be readable");
    assert_eq!(facts.parents.len(), 2, "a --no-ff merge has two parents");
    assert!(facts.is_merge());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reader_follows_the_gitdir_file_of_a_worktree() {
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("worktree") else {
        return;
    };
    if commit(&dir, "base", "a.txt", "a\n").is_none() {
        return;
    }
    let wt = dir.join("wt");
    let Some(wt_str) = wt.to_str() else { return };
    if git(&dir, &["worktree", "add", "-q", "-b", "wtb", wt_str]).is_none() {
        return;
    }
    if commit(&wt, "from worktree", "w.txt", "w\n").is_none() {
        return;
    }

    let repo = Repository::discover(&wt).expect("worktree repository must be discovered");
    let facts = repo.head_commit().expect("worktree HEAD must be readable");
    let want_sha = git(&wt, &["rev-parse", "HEAD"]);
    assert_eq!(facts.subject, "from worktree");
    assert_eq!(facts.sha, want_sha.unwrap_or_default(), "worktree HEAD sha");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reader_reports_missing_objects_instead_of_returning_data() {
    let Some(dir) = make_repo("missing") else {
        return;
    };
    if commit(&dir, "base", "a.txt", "a\n").is_none() {
        return;
    }
    let repo = Repository::discover(&dir).expect("repository must be discovered");

    // Spec: never silently drop an event. An unreadable object must be a
    // distinguishable error, not an empty result.
    let res = repo.read_commit("0000000000000000000000000000000000000000");
    assert!(
        res.is_err(),
        "reading a non-existent object must fail, not return data"
    );

    std::fs::remove_dir_all(&dir).ok();
}

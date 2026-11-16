//! Packfile tests against a real, gc'd repository.
//!
//! # Why a real `git gc` and not a fixture
//!
//! A hand-built pack would test my understanding of the format, not git's
//! implementation of it. Running the actual `git gc` produces exactly the object
//! layering (including delta chains) that breaks the naive reader, so the test fails
//! for the right reason if the reader regresses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_git_hooks::gitread::{pack, Repository};
use std::path::{Path, PathBuf};
use std::process::Command;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

fn make_repo(name: &str) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!("devch-pack-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).ok()?;
    git(&dir, &["init", "-q", "-b", "main"])?;
    git(&dir, &["config", "user.name", "Pack Test"])?;
    git(&dir, &["config", "user.email", "pack@devchievements.local"])?;
    Some(dir)
}

fn commit(dir: &Path, message: &str, file: &str, content: &str) -> Option<()> {
    std::fs::write(dir.join(file), content).ok()?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", message])?;
    Some(())
}

/// Count loose object directories; zero means everything is packed.
fn loose_object_dirs(repo: &Path) -> usize {
    let objects = repo.join(".git").join("objects");
    std::fs::read_dir(objects)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.len() == 2 && n.chars().all(|c| c.is_ascii_hexdigit())
                })
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn reading_a_commit_survives_git_gc() {
    // THE regression this whole module exists for.
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("gc") else { return };
    for i in 0..6 {
        commit(
            &dir,
            &format!("commit {i}"),
            &format!("f{i}.txt"),
            &format!("content {i}\n"),
        );
    }

    let head_before = git(&dir, &["rev-parse", "HEAD"]).expect("head");
    let repo = Repository::discover(&dir).expect("discover");
    let before = repo.head_commit().expect("readable before gc");
    assert_eq!(before.sha, head_before);

    // Pack everything.
    git(&dir, &["gc", "--aggressive", "--prune=now"]);
    assert_eq!(
        loose_object_dirs(&dir),
        0,
        "git gc should have packed every object; the test premise requires it"
    );

    // Now the reader must still work, via the packfile path.
    let repo = Repository::discover(&dir).expect("discover");
    let after = repo.head_commit().expect("must be readable after gc");
    assert_eq!(after.sha, head_before, "same commit");
    assert_eq!(after.subject, "commit 5");
    assert_eq!(after.author_email, "pack@devchievements.local");
    assert_eq!(after.parents.len(), 1);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reading_an_older_commit_from_history_survives_gc() {
    // The HEAD object is usually stored undeltified; older objects are the ones most
    // likely to be encoded as deltas, so exercise those too.
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("history") else {
        return;
    };
    for i in 0..10 {
        // Repeated, near-identical content maximises delta compression.
        commit(
            &dir,
            &format!("c{i}"),
            "shared.txt",
            &format!("line a\nline b\nvalue {i}\n"),
        );
    }

    let shas: Vec<String> = git(&dir, &["rev-list", "HEAD"])
        .expect("rev-list")
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(shas.len(), 10);

    git(&dir, &["gc", "--aggressive", "--prune=now"]);
    assert_eq!(loose_object_dirs(&dir), 0);

    let repo = Repository::discover(&dir).expect("discover");
    for sha in &shas {
        let facts = repo
            .read_commit(sha)
            .unwrap_or_else(|e| panic!("must read {sha} from the pack: {e}"));
        assert_eq!(&facts.sha, sha);
        assert_eq!(facts.author_email, "pack@devchievements.local");
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_pack_exposes_its_objects_by_id() {
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("idx") else { return };
    for i in 0..4 {
        commit(
            &dir,
            &format!("c{i}"),
            &format!("g{i}.txt"),
            &format!("g{i}\n"),
        );
    }
    git(&dir, &["gc", "--prune=now"]);
    let head = git(&dir, &["rev-parse", "HEAD"]).expect("head");

    let packs = pack::packs_in(&dir.join(".git").join("objects"));
    assert_eq!(packs.len(), 1, "expected exactly one pack");
    let pack = pack::PackFile::open(&packs[0]).expect("open pack");
    assert!(pack.contains(&head), "the pack index must list HEAD");

    let object = pack.read(&head).expect("decode HEAD");
    assert_eq!(object.kind, pack::ObjectKind::Commit);
    let text = String::from_utf8_lossy(&object.data);
    assert!(
        text.contains("tree "),
        "a commit object starts with its tree"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_object_still_reports_a_precise_error_after_gc() {
    // Degrading to "not found" must not become "silently no commit".
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("missing") else {
        return;
    };
    commit(&dir, "one", "a.txt", "a\n");
    git(&dir, &["gc", "--prune=now"]);

    let repo = Repository::discover(&dir).expect("discover");
    let err = repo.read_commit(&"0".repeat(40)).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("not") || message.contains("packfile"),
        "the error should say the object is absent, got: {message}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_repository_still_works_when_one_pack_is_corrupt() {
    // A single unreadable pack must not make the whole repository unreadable: an
    // object may live in another pack or loose.
    if !git_available() {
        return;
    }
    let Some(dir) = make_repo("corrupt") else {
        return;
    };
    for i in 0..3 {
        commit(
            &dir,
            &format!("c{i}"),
            &format!("h{i}.txt"),
            &format!("h{i}\n"),
        );
    }
    git(&dir, &["gc", "--prune=now"]);

    let packs = pack::packs_in(&dir.join(".git").join("objects"));
    assert_eq!(packs.len(), 1);

    // Make a new commit AFTER the gc so it stays loose.
    commit(&dir, "after gc", "loose.txt", "loose\n").expect("commit after gc");
    let head = git(&dir, &["rev-parse", "HEAD"]).expect("head");

    // Truncate the pack: it becomes unreadable.
    // git marks pack files read-only, so the truncation has to clear that first.
    // Without this the test only passed on a filesystem that ignores the flag.
    let original = std::fs::read(&packs[0]).expect("read pack");
    let mut permissions = std::fs::metadata(&packs[0])
        .expect("pack metadata")
        .permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&packs[0], permissions).expect("make pack writable");
    std::fs::write(&packs[0], &original[0..64]).expect("truncate");

    let repo = Repository::discover(&dir).expect("discover");
    let facts = repo
        .head_commit()
        .expect("the loose HEAD must still be readable despite a broken pack");
    assert_eq!(facts.sha, head);

    std::fs::remove_dir_all(&dir).ok();
}

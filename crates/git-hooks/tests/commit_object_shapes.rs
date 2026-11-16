//! Regression tests for the two shapes a commit object can arrive in.
//!
//! The pack path hands over the bare payload; the loose path keeps the
//! "commit <size>\0" header. Requiring the header made every packed commit
//! unreadable, and the integration test only caught it on a CI runner, where the
//! object happened to be packed first.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use devchievements_git_hooks::gitread::parse_commit_object;

const PAYLOAD: &str = "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
author Pack Test <pack@devchievements.local> 1700000000 +0800\n\
committer Pack Test <pack@devchievements.local> 1700000000 +0800\n\
\nsubject line\n";

#[test]
fn parses_a_packed_commit_without_header() {
    let facts = parse_commit_object(PAYLOAD.as_bytes()).expect("packed form");
    assert_eq!(facts.subject, "subject line");
    assert_eq!(facts.author_email, "pack@devchievements.local");
    assert_eq!(facts.author_seconds, 1_700_000_000);
    assert_eq!(facts.parents.len(), 0);
}

#[test]
fn parses_a_loose_commit_with_header() {
    let loose = format!("commit {}\0{}", PAYLOAD.len(), PAYLOAD);
    let facts = parse_commit_object(loose.as_bytes()).expect("loose form");
    assert_eq!(facts.subject, "subject line");
}

#[test]
fn rejects_objects_that_are_not_commits() {
    assert!(parse_commit_object(b"blob 5\0hello").is_err());
    assert!(parse_commit_object(b"not a commit at all").is_err());
}

//! Validates the recorded Git hook trace fixtures (spec 9.7).
//!
//! The fixtures in packages/golden-tests/fixtures/git-traces are the replayable
//! record of Phase 0's Git behaviour experiments. Phase 1's Golden Tests consume
//! them, so a broken or drifting fixture must fail loudly here rather than
//! silently weaken those tests later.
//!
//! We deliberately avoid a JSON dependency in this crate: the fixtures are read as
//! text and checked for the structural invariants that matter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/crates/git-hooks
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // repo root
    p.join("packages")
        .join("golden-tests")
        .join("fixtures")
        .join("git-traces")
}

#[test]
fn fixtures_directory_and_index_exist() {
    let dir = fixtures_dir();
    assert!(dir.is_dir(), "fixtures dir missing at {}", dir.display());
    let index = dir.join("index.json");
    assert!(index.is_file(), "index.json missing");
    let schema = dir.join("schema.json");
    assert!(schema.is_file(), "schema.json missing");
}

#[test]
fn every_fixture_listed_in_the_index_exists() {
    let dir = fixtures_dir();
    let index = std::fs::read_to_string(dir.join("index.json")).expect("index readable");
    // Each fixture file name appears in the index and must exist on disk.
    let mut checked = 0;
    for line in index.lines() {
        if let Some(rest) = line.trim().strip_prefix("\"scenario\": \"") {
            if let Some(name) = rest.strip_suffix("\",") {
                let path = dir.join(format!("{name}.json"));
                assert!(
                    path.is_file(),
                    "indexed fixture missing: {}",
                    path.display()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "index listed no fixtures");
}

#[test]
fn every_trace_event_exposes_the_fields_golden_tests_need() {
    let dir = fixtures_dir();
    let needed = [
        "\"hook\"",
        "\"head_sha\"",
        "\"reflog_gs\"",
        "\"parents\"",
        "\"tree\"",
        "\"author_email\"",
        "\"committer_email\"",
        "\"reflog_has_head\"",
        "\"git_version\"",
    ];

    let entries = std::fs::read_dir(&dir).expect("fixtures dir readable");
    let mut files = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name == "index.json" || name == "schema.json" {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("fixture readable");

        // An empty trace is a legitimate recorded outcome: scenario
        // 13-existing-hook-chaining could not run a shell hook at all on the
        // Phase 0 host, so it has zero events. That emptiness is itself the
        // finding, so such fixtures are validated for shape but skip field checks.
        let empty = text.contains("\"events\": []");
        if !empty {
            for field in needed {
                assert!(text.contains(field), "{} is missing field {}", name, field);
            }
        }
        files += 1;
    }
    assert!(
        files >= 10,
        "expected at least 10 fixture files, found {files}"
    );
}

#[test]
fn post_rewrite_events_carry_their_mapping_on_stdin() {
    let dir = fixtures_dir();
    let amend = dir.join("02-amend.json");
    let text = std::fs::read_to_string(&amend).expect("amend fixture readable");
    // The amend trace must record a post-rewrite event and the old->new mapping.
    assert!(
        text.contains("\"post-rewrite\""),
        "amend fixture must contain a post-rewrite event"
    );
    assert!(
        text.contains("\"stdin\": \""),
        "amend fixture must record stdin"
    );
    assert!(
        !text.contains("\"stdin\": \"\"]"),
        "the post-rewrite event in the amend fixture must carry a non-empty mapping"
    );
}

#[test]
fn not_a_repository_is_reported_and_never_a_panic() {
    // Even outside a repo, discovery must return an error value. This protects the
    // hook path: a Helper that panics can break a user's commit (spec 9.2).
    let tmp = std::env::temp_dir().join(format!("devch-nonrepo-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).ok();
    // Only meaningful if tmp is not inside a repository.
    match devchievements_git_hooks::gitread::Repository::discover(&tmp) {
        Ok(_) => { /* temp dir happens to live inside a repo; nothing to assert */ }
        Err(e) => {
            let msg = e.to_string();
            assert!(!msg.is_empty(), "error must carry a message");
        }
    }
    std::fs::remove_dir_all(&tmp).ok();
}

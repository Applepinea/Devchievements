//! Golden Tests: replay the recorded Phase 0 Git traces.
//!
//! These tests are the regression net for the Hook classification rules. Each
//! assertion below is tied to a specific spec clause and to a fact that is
//! genuinely present in the recording. Nothing here invents data.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use devchievements_golden_tests::trace::{self, TraceScenario};
use std::path::PathBuf;

/// Locate the fixtures directory from the crate manifest dir.
fn fixtures_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // repo root
    p.join("packages")
        .join("golden-tests")
        .join("fixtures")
        .join("git-traces")
}

/// Load one scenario by name.
fn load(scenario: &str) -> Option<TraceScenario> {
    let path = fixtures_dir().join(format!("{scenario}.json"));
    let text = std::fs::read_to_string(path).ok()?;
    Some(trace::parse(&text).expect("fixture must be valid JSON"))
}

/// All scenario names present on disk.
fn all_scenarios() -> Vec<String> {
    let dir = fixtures_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)
        .expect("fixtures dir readable")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".json") {
            if stem != "index" && stem != "schema" {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_fixture_parses_into_the_expected_shape() {
    let scenarios = all_scenarios();
    assert!(
        scenarios.len() >= 10,
        "expected the full Phase 0 set, found {}",
        scenarios.len()
    );
    for name in scenarios {
        let s = load(&name).unwrap_or_else(|| panic!("{name} must load"));
        assert_eq!(
            s.scenario, name,
            "{name}: scenario field must match its filename"
        );
        for e in &s.events {
            assert!(
                ["post-commit", "post-rewrite", "post-merge"].contains(&e.hook.as_str()),
                "{name}: unknown hook {}",
                e.hook
            );
        }
    }
}

#[test]
fn post_commit_always_sees_head_already_in_the_reflog() {
    // Phase 0 finding: in Git 2.54.0 the reflog already contains HEAD when
    // post-commit runs, in every scenario. If a future Git version breaks this, the
    // classification design (ADR-0004) must be revisited, so this test is the alarm.
    let mut checked = 0;
    for name in all_scenarios() {
        let Some(s) = load(&name) else { continue };
        for e in s.events_for("post-commit") {
            assert!(
                e.reflog_has_head,
                "{}: post-commit seq {} did not see HEAD in the reflog",
                name, e.seq
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 10,
        "expected many post-commit events, saw {checked}"
    );
}

#[test]
fn amend_and_rebase_report_a_rewrite_mapping() {
    // Spec 9.6: post-rewrite carries the old->new mapping that lets us avoid
    // double-counting a rewritten commit.
    for scenario in ["02-amend", "04-rebase"] {
        let s = load(scenario).unwrap_or_else(|| panic!("{scenario} must exist"));
        let rewrites = s.events_for("post-rewrite");
        assert_eq!(
            rewrites.len(),
            1,
            "{scenario}: exactly one post-rewrite expected"
        );

        let mappings = rewrites[0].rewrite_mappings();
        assert!(
            !mappings.is_empty(),
            "{scenario}: post-rewrite must carry a mapping"
        );
        for (old, new) in &mappings {
            assert_eq!(old.len(), 40, "{scenario}: old sha must be 40 hex chars");
            assert_eq!(new.len(), 40, "{scenario}: new sha must be 40 hex chars");
            assert_ne!(
                old, new,
                "{scenario}: a rewrite maps an id to a DIFFERENT id"
            );
        }
    }
}

#[test]
fn amend_is_reported_with_the_amend_rewrite_kind() {
    let s = load("02-amend").expect("fixture");
    let rw = s.events_for("post-rewrite");
    assert_eq!(rw[0].rewrite_kind(), Some("amend"));
}

#[test]
fn rebase_is_reported_with_the_rebase_rewrite_kind() {
    let s = load("04-rebase").expect("fixture");
    let rw = s.events_for("post-rewrite");
    assert_eq!(rw[0].rewrite_kind(), Some("rebase"));
}

#[test]
fn amend_produces_one_post_commit_and_one_post_rewrite() {
    // The shape that makes classification possible: the commit hook fires first,
    // then the rewrite notification explains what it was.
    let s = load("02-amend").expect("fixture");
    let order = s.hook_order();
    assert_eq!(
        order,
        vec!["post-commit", "post-commit", "post-rewrite"],
        "expected: original commit, amended commit, then the rewrite"
    );
}

#[test]
fn interactive_reword_does_not_emit_post_rewrite() {
    // IMPORTANT recorded limitation (ADR-0004): Git 2.54.0 does NOT fire
    // post-rewrite for an interactive reword. This test pins the finding so nobody
    // later "fixes" the classification by assuming it does.
    let s = load("05-interactive-rebase-reword").expect("fixture");
    assert!(
        s.events_for("post-rewrite").is_empty(),
        "recorded behaviour: reword emits no post-rewrite; a change here invalidates ADR-0004"
    );
    assert!(
        !s.events_for("post-commit").is_empty(),
        "commits still fire"
    );
}

#[test]
fn merge_traces_distinguish_ff_from_no_ff() {
    // Spec 8.1: merge commits are classified separately, and a fast-forward creates
    // no commit at all.
    let no_ff = load("06-merge-no-ff").expect("fixture");
    assert_eq!(
        no_ff.events_for("post-merge").len(),
        1,
        "no-ff fires post-merge"
    );

    let ff = load("07-merge-ff").expect("fixture");
    assert_eq!(
        ff.events_for("post-merge").len(),
        1,
        "ff also fires post-merge"
    );
}

#[test]
fn squash_merge_is_reported_in_post_merge_argv() {
    // Phase 0 finding: post-merge receives "1" for a squash merge, "0" otherwise.
    // That is free information the Helper does not have to probe for.
    let squash = load("10-squash").expect("fixture");
    let pm = squash.events_for("post-merge");
    assert_eq!(pm.len(), 1);
    assert_eq!(pm[0].argv.trim(), "1", "squash merges report argv=1");

    let normal = load("06-merge-no-ff").expect("fixture");
    let pm_normal = normal.events_for("post-merge");
    assert_eq!(
        pm_nm_guard(&pm_normal),
        "0",
        "a normal merge reports argv=0"
    );
}

fn pm_nm_guard(events: &[&devchievements_golden_tests::trace::TraceEvent]) -> String {
    events[0].argv.trim().to_string()
}

#[test]
fn replaying_a_trace_twice_yields_identical_facts() {
    // Determinism is the whole premise of Golden Tests (spec 31.3).
    for name in all_scenarios() {
        let Some(a) = load(&name) else { continue };
        let b = load(&name).expect("second load");
        assert_eq!(
            a.hook_order(),
            b.hook_order(),
            "{name}: order must be stable"
        );
        let head_a: Vec<&str> = a.events.iter().map(|e| e.head_sha.as_str()).collect();
        let head_b: Vec<&str> = b.events.iter().map(|e| e.head_sha.as_str()).collect();
        assert_eq!(head_a, head_b, "{name}: shas must be stable");
    }
}

#[test]
fn the_engine_reacts_only_to_commits_that_should_count() {
    // End-to-end: feed recorded commit facts into the Engine and check the counting
    // rule from spec 8.1 holds for the recorded shapes.
    use devchievements_core::{achievements, daily_rank, CommitFacts, Engine, StateSnapshot};

    let s = load("01-plain-commit").expect("fixture");
    let commits = s.events_for("post-commit");
    assert_eq!(commits.len(), 2);

    let mut snapshot = StateSnapshot::default();
    for (i, e) in commits.iter().enumerate() {
        let facts = CommitFacts {
            counts_as_commit: !e.is_merge(),
            is_merge: e.is_merge(),
            is_empty: false,
        };
        assert!(
            !facts.is_merge,
            "plain commits are not merges in this fixture"
        );
        // The Engine needs a context; build a fixed one for the replay.
        let clock = devchievements_core::FixedClock(1_700_000_000_000);
        let tz = fixed_tz();
        let mut ids = CountingIds::default();
        let cfg = FixedConfig;
        let ctx = devchievements_core::InjectedContext {
            clock: &clock,
            timezone: &tz,
            ids: &mut ids,
            config: &cfg,
        };

        let out = Engine.evaluate_commit(&snapshot, &facts, &format!("golden-{i}"), &ctx);
        assert!(!out.is_empty(), "a counting commit must produce output");

        snapshot.repository_commit_count += 1;
        snapshot.daily_commit_count += 1;
        if let Some(rank) =
            daily_rank::DailyRankThresholds::official().rank_for(snapshot.daily_commit_count)
        {
            snapshot.daily_rank_today = Some(rank);
        }
        for intent in &out.side_effect_intents {
            if let devchievements_core::SideEffectKind::AchievementUnlocked {
                achievement_id, ..
            } = &intent.kind
            {
                snapshot
                    .unlocked_achievement_ids
                    .push(achievement_id.clone());
            }
        }
    }

    assert_eq!(snapshot.repository_commit_count, 2);
    assert!(
        snapshot
            .unlocked_achievement_ids
            .contains(&"first_commit".to_string()),
        "two commits must unlock First Commit"
    );
    assert_eq!(
        snapshot.unlocked_achievement_ids.len(),
        1,
        "two commits must not unlock anything beyond First Commit"
    );
    assert_eq!(
        snapshot.daily_rank_today,
        Some(daily_rank::DailyRank::Uncommon)
    );
    assert_eq!(achievements::builtin_achievements().len(), 9);
}

#[derive(Debug)]
struct FixedTz;
impl devchievements_core::TimezoneProvider for FixedTz {
    fn canonicalize(&self, n: &str) -> Option<String> {
        Some(n.to_string())
    }
    fn local_date(&self, _: i64, _: &str, _: u16) -> Option<String> {
        Some("2026-11-16".to_string())
    }
}
fn fixed_tz() -> FixedTz {
    FixedTz
}

#[derive(Debug, Default)]
struct CountingIds(u64);
impl devchievements_core::IdGenerator for CountingIds {
    fn next_id(&mut self) -> String {
        self.0 += 1;
        format!("id-{}", self.0)
    }
}

#[derive(Debug)]
struct FixedConfig;
impl devchievements_core::ConfigurationProvider for FixedConfig {
    fn config_version(&self) -> u64 {
        1
    }
}

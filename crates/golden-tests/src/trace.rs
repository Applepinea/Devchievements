//! Parsing recorded hook traces.
//!
//! The fixture schema is documented in
//! `packages/golden-tests/fixtures/git-traces/schema.json`. This module reads the
//! subset the replay needs and refuses to guess when something is missing.

use serde::Deserialize;

/// One recorded hook invocation.
#[derive(Debug, Clone, Deserialize)]
pub struct TraceEvent {
    /// Which hook fired.
    pub hook: String,
    /// Order within the scenario.
    pub seq: i64,
    /// Hook argv as one string (post-merge: "0"/"1"; post-rewrite: the kind).
    #[serde(default)]
    pub argv: String,
    /// Raw stdin with newlines flattened to '|'.
    #[serde(default)]
    pub stdin: String,
    /// HEAD at hook time.
    pub head_sha: String,
    /// "<commit> <parent>..." from rev-list.
    #[serde(default)]
    pub parents: String,
    /// Author email.
    #[serde(default)]
    pub author_email: String,
    /// Author timestamp seconds.
    #[serde(default)]
    pub author_ts: String,
    /// HEAD reflog %gs at hook time.
    #[serde(default)]
    pub reflog_gs: String,
    /// Whether HEAD was already in the reflog when the hook ran.
    #[serde(default)]
    pub reflog_has_head: bool,
    /// Git version that produced the trace.
    #[serde(default)]
    pub git_version: String,
}

impl TraceEvent {
    /// Number of parents implied by the recorded parent list.
    ///
    /// The string is "<commit> <parent>...", so parents = tokens - 1.
    pub fn parent_count(&self) -> usize {
        let tokens: Vec<&str> = self.parents.split_whitespace().collect();
        tokens.len().saturating_sub(1)
    }

    /// Whether this is a merge commit by the recorded parent count.
    pub fn is_merge(&self) -> bool {
        self.parent_count() >= 2
    }

    /// The rewrite kind reported on argv for post-rewrite traces.
    pub fn rewrite_kind(&self) -> Option<&str> {
        if self.hook != "post-rewrite" {
            return None;
        }
        let kind = self.argv.trim();
        if kind.is_empty() {
            None
        } else {
            Some(kind)
        }
    }

    /// The old->new SHA pairs recorded on stdin for post-rewrite.
    ///
    /// The fixture flattens newlines to '|', so each pair is one segment.
    pub fn rewrite_mappings(&self) -> Vec<(String, String)> {
        if self.hook != "post-rewrite" || self.stdin.trim().is_empty() {
            return Vec::new();
        }
        self.stdin
            .split('|')
            .filter(|seg| !seg.trim().is_empty())
            .filter_map(|seg| {
                let mut parts = seg.split_whitespace();
                match (parts.next(), parts.next()) {
                    (Some(old), Some(new)) => Some((old.to_string(), new.to_string())),
                    _ => None,
                }
            })
            .collect()
    }
}

/// A recorded scenario.
#[derive(Debug, Clone, Deserialize)]
pub struct TraceScenario {
    /// Scenario name.
    pub scenario: String,
    /// Human description of what was driven.
    #[serde(default)]
    pub description: String,
    /// Git version.
    #[serde(default)]
    pub git_version: String,
    /// Recorded events, in order.
    #[serde(default)]
    pub events: Vec<TraceEvent>,
}

impl TraceScenario {
    /// Events for a given hook name, in recorded order.
    pub fn events_for(&self, hook: &str) -> Vec<&TraceEvent> {
        self.events.iter().filter(|e| e.hook == hook).collect()
    }

    /// The order hooks fired in.
    pub fn hook_order(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.hook.as_str()).collect()
    }
}

/// Parse a scenario fixture from JSON text.
pub fn parse(text: &str) -> Result<TraceScenario, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_count_excludes_the_commit_itself() {
        let e = TraceEvent {
            hook: "post-commit".into(),
            seq: 0,
            argv: String::new(),
            stdin: String::new(),
            head_sha: "a".into(),
            parents: "aaa bbb ccc".into(),
            author_email: String::new(),
            author_ts: "0".into(),
            reflog_gs: String::new(),
            reflog_has_head: true,
            git_version: String::new(),
        };
        assert_eq!(e.parent_count(), 2);
        assert!(e.is_merge());
    }

    #[test]
    fn a_single_parent_is_not_a_merge() {
        let e = TraceEvent {
            hook: "post-commit".into(),
            seq: 0,
            argv: String::new(),
            stdin: String::new(),
            head_sha: "a".into(),
            parents: "aaa bbb".into(),
            author_email: String::new(),
            author_ts: "0".into(),
            reflog_gs: String::new(),
            reflog_has_head: true,
            git_version: String::new(),
        };
        assert_eq!(e.parent_count(), 1);
        assert!(!e.is_merge());
    }

    #[test]
    fn rewrite_mappings_split_on_the_flattened_newline() {
        let e = TraceEvent {
            hook: "post-rewrite".into(),
            seq: 3,
            argv: "amend".into(),
            stdin: "aaa bbb|ccc ddd".into(),
            head_sha: "x".into(),
            parents: String::new(),
            author_email: String::new(),
            author_ts: "0".into(),
            reflog_gs: String::new(),
            reflog_has_head: true,
            git_version: String::new(),
        };
        assert_eq!(e.rewrite_kind(), Some("amend"));
        assert_eq!(
            e.rewrite_mappings(),
            vec![
                ("aaa".to_string(), "bbb".to_string()),
                ("ccc".to_string(), "ddd".to_string())
            ]
        );
    }

    #[test]
    fn non_rewrite_events_report_no_mappings() {
        let e = TraceEvent {
            hook: "post-commit".into(),
            seq: 0,
            argv: String::new(),
            stdin: String::new(),
            head_sha: "x".into(),
            parents: String::new(),
            author_email: String::new(),
            author_ts: "0".into(),
            reflog_gs: String::new(),
            reflog_has_head: true,
            git_version: String::new(),
        };
        assert!(e.rewrite_kind().is_none());
        assert!(e.rewrite_mappings().is_empty());
    }
}

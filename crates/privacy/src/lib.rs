//! Privacy mode and data handling (spec 26).
//!
//! # What this module is responsible for
//!
//! Spec 26.2 asks privacy mode to hide repository names, commit messages and PR
//! titles, and to use generic notification wording. Spec 26.4 adds quota limits and
//! validated config import.
//!
//! All of that is pure transformation, so it lives here and is fully testable
//! without a window -- which matters, because a privacy feature that is only
//! "probably right" is worse than none.
//!
//! # The rule that is easy to get wrong
//!
//! Spec 26.2 keeps the ACHIEVEMENT NAME visible in privacy mode and replaces only the
//! detail around it ("A new milestone has been reached"). Privacy mode is therefore
//! not "show nothing"; it is "show the achievement, hide the context". Implementing
//! it as a blanket mute would silently break the feature the user still wants.
//!
//! # Honesty requirement
//!
//! Spec 26.2 states plainly that privacy mode is NOT encryption. Nothing here claims
//! otherwise, and PRIVACY_MODE_DISCLAIMER carries the wording the UI must show.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// The disclaimer the UI must display (spec 26.2).
pub const PRIVACY_MODE_DISCLAIMER: &str = "Privacy mode hides details in notifications and diagnostic exports. It does not encrypt your data and does not detect screen sharing.";

/// The generic body used instead of a specific detail (spec 26.2).
pub const GENERIC_BODY: &str = "A new milestone has been reached";

/// Per-file limit for custom sounds and images (spec 26.4): 5 MB.
pub const CUSTOM_ASSET_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Total limit for custom assets (spec 26.4): 200 MB.
pub const CUSTOM_ASSET_TOTAL_MAX_BYTES: u64 = 200 * 1024 * 1024;

/// Fraction of the quota at which the user is warned (spec 26.4: 80%).
pub const QUOTA_WARN_PERCENT: u64 = 80;

/// Whether privacy mode is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// Show repository names, messages and titles.
    #[default]
    Normal,
    /// Hide them and use generic wording.
    Privacy,
}

impl PrivacyMode {
    /// Whether detail is hidden.
    pub fn hides_detail(self) -> bool {
        matches!(self, PrivacyMode::Privacy)
    }

    /// A control to toggle between modes.
    pub fn toggled(self) -> Self {
        match self {
            PrivacyMode::Normal => PrivacyMode::Privacy,
            PrivacyMode::Privacy => PrivacyMode::Normal,
        }
    }
}

/// Context that a notification would normally show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationContext<'a> {
    /// The achievement, rank or milestone name. Shown in BOTH modes (spec 26.2).
    pub headline: &'a str,
    /// Repository display name.
    pub repository: Option<&'a str>,
    /// Commit subject or PR title.
    pub detail: Option<&'a str>,
}

/// The text actually shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationCopy {
    /// Title line.
    pub title: String,
    /// Body line.
    pub body: String,
}

/// Build the copy for a notification under the current mode.
///
/// In normal mode the detail is passed through. In privacy mode the headline is kept
/// and everything that identifies the work is replaced with GENERIC_BODY.
pub fn notification_copy(mode: PrivacyMode, context: &NotificationContext<'_>) -> NotificationCopy {
    match mode {
        PrivacyMode::Normal => NotificationCopy {
            title: context.headline.to_string(),
            body: match (context.repository, context.detail) {
                (Some(repo), Some(detail)) => format!("{repo}: {detail}"),
                (Some(repo), None) => repo.to_string(),
                (None, Some(detail)) => detail.to_string(),
                (None, None) => String::new(),
            },
        },
        PrivacyMode::Privacy => NotificationCopy {
            title: context.headline.to_string(),
            body: GENERIC_BODY.to_string(),
        },
    }
}
/// A stable, non-reversible alias for a repository (spec 26.2).
///
/// Diagnostic exports keep the aliases so two entries can still be told apart and
/// correlated across exports, which a plain "(hidden)" would destroy. The alias must:
///
/// - be identical for the same input, across runs and machines;
/// - differ between different repositories;
/// - not allow recovering the name.
pub fn stable_alias(repository_id: &str) -> String {
    // Two independent FNV-1a passes give 64 bits without pulling in a hash crate.
    // This is an alias, not a security boundary: the guarantee is "not obviously
    // reversible and collision-unlikely", not "cryptographically safe".
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x9e37_79b9_7f4a_7c15;
    for (i, byte) in repository_id.as_bytes().iter().enumerate() {
        a ^= u64::from(*byte);
        a = a.wrapping_mul(0x0100_0000_01b3);
        b = b.rotate_left(9) ^ (u64::from(*byte).wrapping_add(i as u64));
        b = b.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("repo-{a:08x}{:04x}", (b >> 48) as u16)
}

/// Mask a filesystem path so it can appear in a diagnostic export (spec 26.2).
///
/// The user's home directory becomes a tilde, and anything that looks like an email
/// is replaced. Only the home prefix is collapsed: matching a shared prefix by
/// accident would corrupt unrelated paths.
pub fn redact_path(path: &str, home: Option<&str>) -> String {
    let mut out = path.replace('\\', "/");

    if let Some(home) = home {
        let home_norm = home.replace('\\', "/");
        // The home prefix only counts when it ends at a separator or at the end of
        // the path. Matching a raw prefix would turn "/home/alice-two/work" into
        // "~-two/work", silently attributing one user's path to another.
        let is_home = !home_norm.is_empty()
            && out.starts_with(&home_norm)
            && out[home_norm.len()..]
                .chars()
                .next()
                .is_none_or(|c| c == '/');
        if is_home {
            out = format!("~{}", &out[home_norm.len()..]);
        }
    }

    mask_emails(&out)
}

fn is_email_local(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
}

/// Replace every local@domain occurrence with a placeholder.
fn mask_emails(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;

    while i < chars.len() {
        let start = i;
        while i < chars.len() && is_email_local(chars[i]) {
            i += 1;
        }
        let local_len = i - start;

        let has_at = i < chars.len() && chars[i] == '@';
        let domain_has_dot = has_at
            && chars.get(i + 1).is_some_and(|c| c.is_ascii_alphanumeric())
            && chars[i + 1..]
                .iter()
                .take_while(|c| !c.is_whitespace() && **c != '"')
                .any(|c| *c == '.');

        if local_len > 0 && has_at && domain_has_dot {
            out.push_str("***@***");
            i += 1;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '"' {
                i += 1;
            }
        } else {
            out.extend(&chars[start..i]);
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
        }
    }

    out
}

/// Whether a diagnostic export includes raw values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    /// Repository names replaced by stable aliases, paths and emails masked.
    #[default]
    Redacted,
    /// Raw values, only after an explicit warning (spec 26.2).
    Raw,
}

/// The warning that must be shown before a raw export.
pub const RAW_EXPORT_WARNING: &str = "This export contains repository names, paths and possibly email addresses. Anyone you share it with will see them.";

/// Why an export was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// Raw export was requested without acknowledging the warning.
    RawExportNotAcknowledged,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::RawExportNotAcknowledged => write!(
                f,
                "raw export requires the user to acknowledge the warning first"
            ),
        }
    }
}

impl std::error::Error for ExportError {}

/// Decide the export mode, refusing raw unless it was explicitly acknowledged.
///
/// Spec 26.4: imported and exported data must not be trusted implicitly. A caller
/// cannot switch to raw by passing a flag alone; it must also pass the exact warning
/// text the user was shown, which makes forgetting the warning a visible choice
/// rather than an accident.
pub fn resolve_export_mode(
    requested: ExportMode,
    acknowledged_warning: Option<&str>,
) -> Result<ExportMode, ExportError> {
    match requested {
        ExportMode::Redacted => Ok(ExportMode::Redacted),
        ExportMode::Raw => match acknowledged_warning {
            Some(text) if text == RAW_EXPORT_WARNING => Ok(ExportMode::Raw),
            _ => Err(ExportError::RawExportNotAcknowledged),
        },
    }
}

/// A log line that structurally cannot carry a commit message (spec 26.2).
///
/// The field is private on purpose: obtaining one requires with_commit_message(),
/// which exists only for an explicitly enabled verbose log. Nothing attaches a
/// message casually.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// The event being logged.
    pub event: String,
    /// Repository identity, already aliased where required.
    pub repository: String,
    /// Optional message, only present when explicitly permitted.
    commit_message: Option<String>,
}

impl LogLine {
    /// A log line without any commit message (the default).
    pub fn new(event: impl Into<String>, repository: impl Into<String>) -> Self {
        LogLine {
            event: event.into(),
            repository: repository.into(),
            commit_message: None,
        }
    }

    /// Attach a commit message, for an explicitly enabled verbose log only.
    pub fn with_commit_message(mut self, message: impl Into<String>) -> Self {
        self.commit_message = Some(message.into());
        self
    }

    /// Read the attached message, if any.
    pub fn commit_message(&self) -> Option<&str> {
        self.commit_message.as_deref()
    }

    /// Render for output.
    pub fn render(&self) -> String {
        match &self.commit_message {
            Some(message) => format!("[{}] {} :: {message}", self.event, self.repository),
            None => format!("[{}] {}", self.event, self.repository),
        }
    }
}

/// Storage quota state (spec 26.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaState {
    /// Comfortably under the limit.
    Ok,
    /// At or above the warning threshold but not full.
    Warning,
    /// At or over the limit; further additions must be refused.
    Exceeded,
}

/// Evaluate total custom-asset usage against the quota.
pub fn quota_state(used_bytes: u64, limit_bytes: u64) -> QuotaState {
    if used_bytes >= limit_bytes {
        return QuotaState::Exceeded;
    }
    // Compare with multiplication to avoid a rounding edge at the exact boundary.
    if used_bytes.saturating_mul(100) >= limit_bytes.saturating_mul(QUOTA_WARN_PERCENT) {
        QuotaState::Warning
    } else {
        QuotaState::Ok
    }
}

/// Why a custom asset was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// A single file is over the per-file limit.
    FileTooLarge {
        /// Actual size.
        size: u64,
        /// Allowed size.
        limit: u64,
    },
    /// Adding it would exceed the total limit.
    TotalWouldExceed {
        /// Usage after the addition.
        would_be: u64,
        /// Allowed total.
        limit: u64,
    },
}

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetError::FileTooLarge { size, limit } => {
                write!(
                    f,
                    "asset of {size} bytes exceeds the {limit}-byte per-file limit"
                )
            }
            AssetError::TotalWouldExceed { would_be, limit } => write!(
                f,
                "adding this asset would reach {would_be} bytes, over the {limit}-byte total limit"
            ),
        }
    }
}

impl std::error::Error for AssetError {}

/// Check whether a custom asset may be added.
pub fn check_asset(size: u64, current_total: u64) -> Result<(), AssetError> {
    if size > CUSTOM_ASSET_MAX_BYTES {
        return Err(AssetError::FileTooLarge {
            size,
            limit: CUSTOM_ASSET_MAX_BYTES,
        });
    }
    let would_be = current_total.saturating_add(size);
    if would_be > CUSTOM_ASSET_TOTAL_MAX_BYTES {
        return Err(AssetError::TotalWouldExceed {
            would_be,
            limit: CUSTOM_ASSET_TOTAL_MAX_BYTES,
        });
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_mode_keeps_the_achievement_name() {
        // Spec 26.2: privacy mode shows the achievement plus a generic body. A blanket
        // mute would break the feature the user still wants.
        let context = NotificationContext {
            headline: "First Commit",
            repository: Some("acme/secret-project"),
            detail: Some("Add payment integration"),
        };
        let copy = notification_copy(PrivacyMode::Privacy, &context);

        assert_eq!(copy.title, "First Commit", "the achievement name survives");
        assert_eq!(copy.body, GENERIC_BODY);
        assert!(!copy.body.contains("secret-project"), "no repository name");
        assert!(!copy.body.contains("payment"), "no commit subject");
    }

    #[test]
    fn normal_mode_shows_repository_and_detail() {
        let context = NotificationContext {
            headline: "First Commit",
            repository: Some("acme/site"),
            detail: Some("Fix the login form"),
        };
        let copy = notification_copy(PrivacyMode::Normal, &context);
        assert_eq!(copy.title, "First Commit");
        assert_eq!(copy.body, "acme/site: Fix the login form");
    }

    #[test]
    fn normal_mode_handles_missing_context() {
        let copy = notification_copy(
            PrivacyMode::Normal,
            &NotificationContext {
                headline: "Rank Up",
                repository: None,
                detail: None,
            },
        );
        assert_eq!(copy.title, "Rank Up");
        assert!(copy.body.is_empty());
    }

    #[test]
    fn the_two_modes_never_produce_identical_bodies_when_context_exists() {
        // Regression guard: if privacy mode stopped redacting, this fails.
        let context = NotificationContext {
            headline: "X",
            repository: Some("r"),
            detail: Some("d"),
        };
        assert_ne!(
            notification_copy(PrivacyMode::Normal, &context).body,
            notification_copy(PrivacyMode::Privacy, &context).body
        );
    }

    #[test]
    fn toggling_privacy_mode_round_trips() {
        assert_eq!(PrivacyMode::Normal.toggled(), PrivacyMode::Privacy);
        assert_eq!(PrivacyMode::Privacy.toggled(), PrivacyMode::Normal);
        assert!(PrivacyMode::Privacy.hides_detail());
        assert!(!PrivacyMode::Normal.hides_detail());
    }

    #[test]
    fn the_disclaimer_does_not_claim_encryption() {
        // Spec 26.2: privacy mode must never be described as encryption.
        let lowered = PRIVACY_MODE_DISCLAIMER.to_lowercase();
        assert!(lowered.contains("does not encrypt"));
        assert!(!lowered.contains("is encrypted"));
    }

    #[test]
    fn a_stable_alias_is_deterministic() {
        let a = stable_alias("github:acme/site");
        for _ in 0..5 {
            assert_eq!(stable_alias("github:acme/site"), a);
        }
    }

    #[test]
    fn different_repositories_get_different_aliases() {
        // Exports keep aliases so entries stay distinguishable; collapsing them to one
        // placeholder would destroy that.
        assert_ne!(
            stable_alias("github:acme/site"),
            stable_alias("github:acme/other")
        );
    }

    #[test]
    fn an_alias_does_not_contain_the_original_name() {
        let alias = stable_alias("github:acme/secret-project");
        assert!(!alias.contains("acme"));
        assert!(!alias.contains("secret"));
        assert!(alias.starts_with("repo-"));
    }

    #[test]
    fn aliases_stay_short_enough_for_a_diagnostic_table() {
        assert!(stable_alias("x").len() <= 32);
    }

    #[test]
    fn a_home_directory_is_replaced_with_a_tilde() {
        let redacted = redact_path("C:\\Users\\alice\\code\\repo", Some("C:\\Users\\alice"));
        assert_eq!(redacted, "~/code/repo");
        assert!(!redacted.contains("alice"));
    }

    #[test]
    fn an_email_anywhere_in_a_path_is_masked() {
        let redacted = redact_path("/logs/alice@example.com/run.log", None);
        assert!(!redacted.contains("alice@example.com"));
        assert!(redacted.contains("***@***"));
    }

    #[test]
    fn redaction_leaves_a_path_without_identifiers_alone() {
        assert_eq!(redact_path("/var/tmp/build", None), "/var/tmp/build");
    }

    #[test]
    fn a_path_outside_the_home_directory_is_not_mangled() {
        assert_eq!(
            redact_path("/opt/other/thing", Some("/home/alice")),
            "/opt/other/thing"
        );
    }

    #[test]
    fn a_partial_home_prefix_is_not_collapsed() {
        // "/home/alice-two" starts with "/home/alice" but is a different user.
        let redacted = redact_path("/home/alice-two/work", Some("/home/alice"));
        assert!(
            !redacted.starts_with("~-two"),
            "must not corrupt a shared prefix"
        );
    }

    #[test]
    fn redacted_export_is_allowed_by_default() {
        assert_eq!(
            resolve_export_mode(ExportMode::Redacted, None),
            Ok(ExportMode::Redacted)
        );
    }

    #[test]
    fn raw_export_requires_the_warning_to_be_acknowledged() {
        // Spec 26.2: raw export exists, but only behind an explicit warning.
        assert_eq!(
            resolve_export_mode(ExportMode::Raw, None),
            Err(ExportError::RawExportNotAcknowledged)
        );
        assert_eq!(
            resolve_export_mode(ExportMode::Raw, Some("something else")),
            Err(ExportError::RawExportNotAcknowledged)
        );
        assert_eq!(
            resolve_export_mode(ExportMode::Raw, Some(RAW_EXPORT_WARNING)),
            Ok(ExportMode::Raw)
        );
    }

    #[test]
    fn a_log_line_carries_no_commit_message_by_default() {
        // Spec 26.2: logs must not contain commit messages by default.
        let line = LogLine::new("commit", "repo-abc");
        assert!(line.commit_message().is_none());
        assert_eq!(line.render(), "[commit] repo-abc");
    }

    #[test]
    fn a_commit_message_appears_only_when_explicitly_attached() {
        let line = LogLine::new("commit", "repo-abc").with_commit_message("Fix typo");
        assert_eq!(line.commit_message(), Some("Fix typo"));
        assert!(line.render().contains("Fix typo"));
    }

    #[test]
    fn quota_is_ok_well_below_the_threshold() {
        assert_eq!(quota_state(0, 1000), QuotaState::Ok);
        assert_eq!(quota_state(500, 1000), QuotaState::Ok);
    }

    #[test]
    fn quota_warns_at_exactly_eighty_percent() {
        assert_eq!(quota_state(800, 1000), QuotaState::Warning);
        assert_eq!(quota_state(799, 1000), QuotaState::Ok);
    }

    #[test]
    fn quota_is_exceeded_at_the_limit() {
        assert_eq!(quota_state(1000, 1000), QuotaState::Exceeded);
        assert_eq!(quota_state(1001, 1000), QuotaState::Exceeded);
    }

    #[test]
    fn the_default_limits_match_the_spec() {
        assert_eq!(CUSTOM_ASSET_MAX_BYTES, 5 * 1024 * 1024);
        assert_eq!(CUSTOM_ASSET_TOTAL_MAX_BYTES, 200 * 1024 * 1024);
        assert_eq!(QUOTA_WARN_PERCENT, 80);
    }

    #[test]
    fn a_file_over_five_megabytes_is_refused() {
        assert!(check_asset(CUSTOM_ASSET_MAX_BYTES + 1, 0).is_err());
        assert!(check_asset(CUSTOM_ASSET_MAX_BYTES, 0).is_ok());
    }

    #[test]
    fn an_asset_that_would_exceed_the_total_is_refused() {
        let near_full = CUSTOM_ASSET_TOTAL_MAX_BYTES - 10;
        assert!(check_asset(1024, near_full).is_err());
        assert!(check_asset(10, near_full).is_ok());
    }

    #[test]
    fn the_eighty_percent_warning_fires_on_the_real_total() {
        let eighty = CUSTOM_ASSET_TOTAL_MAX_BYTES * 80 / 100;
        assert_eq!(
            quota_state(eighty, CUSTOM_ASSET_TOTAL_MAX_BYTES),
            QuotaState::Warning
        );
        assert_eq!(
            quota_state(eighty - 1, CUSTOM_ASSET_TOTAL_MAX_BYTES),
            QuotaState::Ok
        );
    }
}

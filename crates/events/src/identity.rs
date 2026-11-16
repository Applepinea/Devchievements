//! User identity (spec 7).
//!
//! # Why identity is a first-class model and not a config string
//!
//! Every statistic answers the question "did *I* do this?". Git records an author
//! email, which anyone can set to anything, so attribution is a heuristic that must
//! be explicit about its confidence (spec 5.4 says the same thing). A single
//! `user.email` field hides three real situations:
//!
//! - the same person commits from a personal and a work machine;
//! - GitHub's noreply address is what appears for web-authored commits;
//! - a second GitHub account exists.
//!
//! So identity is a *set* of emails with a *priority*, plus an explicit end-to-end
//! GitHub-login mapping for Browser Only mode, where no local email exists at all.
//!
//! # The hard rule
//!
//! Spec 7: with no email configured, statistics and history import must NOT begin.
//! [`Identity::is_configured`] exists so that guard cannot be forgotten: every entry
//! point checks it before counting anything.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// Where an email came from, which determines how much to trust it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailOrigin {
    /// A GitHub noreply address (`1234+user@users.noreply.github.com`).
    ///
    /// Lowest priority: it identifies the account, not the person's real address,
    /// and it changes if the account is renamed.
    GitHubNoreply,
    /// Added manually by the user.
    UserConfigured,
    /// Read from the repository's own `user.email`.
    RepositoryConfig,
    /// Read from the global git config.
    GlobalConfig,
}

impl EmailOrigin {
    /// Higher is trusted more when two entries could both match.
    ///
    /// The ordering is the spec's priority list (spec 7): personal/work addresses the
    /// user explicitly configured beat values inherited from config, and a noreply
    /// address is the weakest signal of all.
    pub fn priority(self) -> u8 {
        match self {
            EmailOrigin::GitHubNoreply => 0,
            EmailOrigin::GlobalConfig => 1,
            EmailOrigin::RepositoryConfig => 2,
            EmailOrigin::UserConfigured => 3,
        }
    }

    /// A short label for the settings UI.
    pub fn label(self) -> &'static str {
        match self {
            EmailOrigin::GitHubNoreply => "GitHub noreply",
            EmailOrigin::UserConfigured => "Configured by you",
            EmailOrigin::RepositoryConfig => "From this repository",
            EmailOrigin::GlobalConfig => "From your git config",
        }
    }
}

/// One configured email address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownEmail {
    /// The address, stored as given.
    pub email: String,
    /// How it was obtained.
    pub origin: EmailOrigin,
}

impl KnownEmail {
    /// Construct and normalise.
    ///
    /// Empty addresses are rejected here rather than filtered later: an empty entry
    /// would silently match commits with a blank author field.
    pub fn new(email: impl Into<String>, origin: EmailOrigin) -> Option<Self> {
        let email = email.into();
        let trimmed = email.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(KnownEmail {
            email: trimmed.to_string(),
            origin,
        })
    }
}

/// Why identity configuration is unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// No emails configured: statistics must not start (spec 7).
    NotConfigured,
    /// Two GitHub logins mapped to the same email set in conflicting ways.
    ConflictingGitHubMapping(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentityError::NotConfigured => write!(
                f,
                "no email configured: statistics and history import require at least one"
            ),
            IdentityError::ConflictingGitHubMapping(login) => {
                write!(f, "conflicting email mapping for GitHub login {login}")
            }
        }
    }
}

impl std::error::Error for IdentityError {}

/// The complete identity configuration (spec 7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Every address that counts as "me".
    ///
    /// Ordered by the engine, not by insertion: see [`Self::prioritised`].
    pub emails: Vec<KnownEmail>,
    /// GitHub login to the emails that login authors commits as (Browser Only).
    ///
    /// Spec 7 explicitly forbids assuming a login equals a local Git email, so this
    /// mapping is always explicit and never inferred.
    pub github_logins: Vec<GitHubLoginMapping>,
}

/// A Browser Only identity mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubLoginMapping {
    /// The GitHub login (case-insensitive comparison).
    pub login: String,
    /// Emails this login is known to author commits as.
    pub emails: Vec<String>,
}

impl Identity {
    /// Whether statistics may begin (spec 7).
    pub fn is_configured(&self) -> bool {
        !self.emails.is_empty()
    }

    /// Enforce the "must be configured first" rule.
    pub fn require_configured(&self) -> Result<(), IdentityError> {
        if self.is_configured() {
            Ok(())
        } else {
            Err(IdentityError::NotConfigured)
        }
    }

    /// Emails ordered by trust, most trusted first.
    ///
    /// Ties keep insertion order so the result is deterministic; a set that reorders
    /// itself between runs would make diagnostics unreproducible.
    pub fn prioritised(&self) -> Vec<&KnownEmail> {
        let mut refs: Vec<(usize, &KnownEmail)> = self.emails.iter().enumerate().collect();
        refs.sort_by(|a, b| {
            b.1.origin
                .priority()
                .cmp(&a.1.origin.priority())
                .then(a.0.cmp(&b.0))
        });
        refs.into_iter().map(|(_, e)| e).collect()
    }

    /// The email to use when one must be picked (e.g. a default in the UI).
    pub fn primary_email(&self) -> Option<&str> {
        self.prioritised().first().map(|e| e.email.as_str())
    }

    /// Whether an author email belongs to this identity.
    ///
    /// Case-insensitive, as Git treats addresses case-insensitively in practice.
    pub fn owns_email(&self, author_email: &str) -> bool {
        let needle = author_email.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return false;
        }
        self.emails
            .iter()
            .any(|known| known.email.to_ascii_lowercase() == needle)
    }

    /// Add an email, refusing duplicates (case-insensitive) and refusing to downgrade
    /// an existing entry's origin to a less trusted one.
    pub fn add_email(&mut self, email: impl Into<String>, origin: EmailOrigin) -> bool {
        let Some(candidate) = KnownEmail::new(email, origin) else {
            return false;
        };
        let lowered = candidate.email.to_ascii_lowercase();

        if let Some(existing) = self
            .emails
            .iter_mut()
            .find(|e| e.email.to_ascii_lowercase() == lowered)
        {
            if candidate.origin.priority() > existing.origin.priority() {
                existing.origin = candidate.origin;
                return true;
            }
            return false;
        }

        self.emails.push(candidate);
        true
    }

    /// Emails associated with a GitHub login, if mapped.
    pub fn emails_for_login(&self, login: &str) -> Option<&[String]> {
        let needle = login.trim().to_ascii_lowercase();
        self.github_logins
            .iter()
            .find(|m| m.login.to_ascii_lowercase() == needle)
            .map(|m| m.emails.as_slice())
    }

    /// Add a login mapping, merging with an existing one instead of duplicating it.
    pub fn add_login_mapping(&mut self, login: impl Into<String>, emails: Vec<String>) {
        let login = login.into();
        let needle = login.trim().to_ascii_lowercase();

        if let Some(existing) = self
            .github_logins
            .iter_mut()
            .find(|m| m.login.to_ascii_lowercase() == needle)
        {
            for email in emails {
                let lowered = email.to_ascii_lowercase();
                if !existing
                    .emails
                    .iter()
                    .any(|e| e.to_ascii_lowercase() == lowered)
                {
                    existing.emails.push(email);
                }
            }
            return;
        }

        self.github_logins
            .push(GitHubLoginMapping { login, emails });
    }
}

/// Detect whether an address is a GitHub noreply address.
///
/// Both historical forms exist: `user@users.noreply.github.com` and the newer
/// `1234+user@users.noreply.github.com`.
pub fn is_github_noreply(email: &str) -> bool {
    let lowered = email.trim().to_ascii_lowercase();
    lowered.ends_with("@users.noreply.github.com")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(emails: &[(&str, EmailOrigin)]) -> Identity {
        let mut id = Identity::default();
        for (email, origin) in emails {
            id.add_email(*email, *origin);
        }
        id
    }

    #[test]
    fn an_empty_identity_is_not_configured_and_refuses_to_start() {
        // Spec 7: statistics and history import must not begin without an email.
        let id = Identity::default();
        assert!(!id.is_configured());
        assert_eq!(id.require_configured(), Err(IdentityError::NotConfigured));
        assert!(id.primary_email().is_none());
    }

    #[test]
    fn one_email_is_enough_to_start() {
        let id = identity(&[("me@example.com", EmailOrigin::UserConfigured)]);
        assert!(id.is_configured());
        assert!(id.require_configured().is_ok());
    }

    #[test]
    fn empty_addresses_are_refused_rather_than_matching_blank_authors() {
        // An empty entry would match every commit whose author field is empty.
        let mut id = Identity::default();
        assert!(!id.add_email("", EmailOrigin::UserConfigured));
        assert!(!id.add_email("   ", EmailOrigin::UserConfigured));
        assert!(!id.is_configured());
    }

    #[test]
    fn author_matching_is_case_insensitive() {
        let id = identity(&[("Me@Example.COM", EmailOrigin::UserConfigured)]);
        assert!(id.owns_email("me@example.com"));
        assert!(id.owns_email("ME@EXAMPLE.COM"));
        assert!(!id.owns_email("someone@else.com"));
    }

    #[test]
    fn a_blank_author_email_never_matches() {
        let id = identity(&[("me@example.com", EmailOrigin::UserConfigured)]);
        assert!(!id.owns_email(""));
        assert!(!id.owns_email("   "));
    }

    #[test]
    fn user_configured_emails_outrank_config_derived_ones() {
        // Spec 7 priority: personal/work addresses the user set explicitly first.
        let id = identity(&[
            ("from-global@example.com", EmailOrigin::GlobalConfig),
            ("from-repo@example.com", EmailOrigin::RepositoryConfig),
            ("mine@example.com", EmailOrigin::UserConfigured),
            (
                "1234+me@users.noreply.github.com",
                EmailOrigin::GitHubNoreply,
            ),
        ]);
        assert_eq!(id.primary_email(), Some("mine@example.com"));

        let order: Vec<&str> = id.prioritised().iter().map(|e| e.email.as_str()).collect();
        assert_eq!(
            order,
            vec![
                "mine@example.com",
                "from-repo@example.com",
                "from-global@example.com",
                "1234+me@users.noreply.github.com",
            ]
        );
    }

    #[test]
    fn prioritisation_is_deterministic_for_equal_origins() {
        // A set that reorders itself between runs makes diagnostics unreproducible.
        let id = identity(&[
            ("b@example.com", EmailOrigin::UserConfigured),
            ("a@example.com", EmailOrigin::UserConfigured),
        ]);
        let first: Vec<String> = id.prioritised().iter().map(|e| e.email.clone()).collect();
        for _ in 0..5 {
            let again: Vec<String> = id.prioritised().iter().map(|e| e.email.clone()).collect();
            assert_eq!(first, again);
        }
        assert_eq!(
            first,
            vec!["b@example.com", "a@example.com"],
            "insertion order preserved on ties"
        );
    }

    #[test]
    fn the_same_email_is_not_added_twice_even_with_different_case() {
        let mut id = Identity::default();
        assert!(id.add_email("me@example.com", EmailOrigin::GlobalConfig));
        assert!(!id.add_email("ME@Example.com", EmailOrigin::GlobalConfig));
        assert_eq!(id.emails.len(), 1);
    }

    #[test]
    fn re_adding_an_email_can_upgrade_its_trust_but_not_downgrade_it() {
        let mut id = Identity::default();
        id.add_email("me@example.com", EmailOrigin::GlobalConfig);
        assert_eq!(id.emails[0].origin, EmailOrigin::GlobalConfig);

        // Upgrading is meaningful: the user confirmed it explicitly.
        assert!(id.add_email("me@example.com", EmailOrigin::UserConfigured));
        assert_eq!(id.emails[0].origin, EmailOrigin::UserConfigured);

        // Downgrading is not: we would lose information the user provided.
        assert!(!id.add_email("me@example.com", EmailOrigin::GitHubNoreply));
        assert_eq!(id.emails[0].origin, EmailOrigin::UserConfigured);
    }

    #[test]
    fn noreply_addresses_are_recognised_in_both_known_forms() {
        assert!(is_github_noreply("user@users.noreply.github.com"));
        assert!(is_github_noreply("1234+user@users.noreply.github.com"));
        assert!(is_github_noreply("  1234+User@Users.NoReply.GitHub.com  "));
        assert!(!is_github_noreply("me@example.com"));
        assert!(!is_github_noreply("user@github.com"));
    }

    #[test]
    fn github_logins_are_matched_case_insensitively() {
        // Spec 7: the login-to-email mapping is explicit; Git and GitHub treat logins
        // case-insensitively.
        let mut id = Identity::default();
        id.add_login_mapping("OctoCat", vec!["me@example.com".to_string()]);
        assert_eq!(
            id.emails_for_login("octocat").map(|e| e.to_vec()),
            Some(vec!["me@example.com".to_string()])
        );
        assert!(id.emails_for_login("someone-else").is_none());
    }

    #[test]
    fn adding_a_login_mapping_merges_instead_of_duplicating() {
        let mut id = Identity::default();
        id.add_login_mapping("octocat", vec!["a@example.com".to_string()]);
        id.add_login_mapping(
            "OctoCat",
            vec!["b@example.com".to_string(), "A@Example.com".to_string()],
        );
        assert_eq!(id.github_logins.len(), 1, "no duplicate login rows");
        assert_eq!(
            id.github_logins[0].emails,
            vec!["a@example.com".to_string(), "b@example.com".to_string()],
            "emails are unioned case-insensitively"
        );
    }

    #[test]
    fn a_login_is_never_assumed_to_equal_an_email() {
        // Spec 7 forbids the shortcut. With no mapping, lookup must fail rather than
        // guessing that the login prefixes an email.
        let id = identity(&[("octocat@example.com", EmailOrigin::UserConfigured)]);
        assert!(id.emails_for_login("octocat").is_none());
    }

    #[test]
    fn identity_survives_a_json_round_trip() {
        let mut id = identity(&[("me@example.com", EmailOrigin::UserConfigured)]);
        id.add_login_mapping("octocat", vec!["me@example.com".to_string()]);
        let json = serde_json::to_string(&id).expect("serialise");
        let back: Identity = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(id, back);
    }

    #[test]
    fn origin_labels_are_all_distinct() {
        // The settings UI shows these; duplicates would be confusing.
        let labels = [
            EmailOrigin::GitHubNoreply.label(),
            EmailOrigin::UserConfigured.label(),
            EmailOrigin::RepositoryConfig.label(),
            EmailOrigin::GlobalConfig.label(),
        ];
        let unique: std::collections::BTreeSet<&str> = labels.iter().copied().collect();
        assert_eq!(unique.len(), labels.len());
    }
}

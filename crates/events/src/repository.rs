//! Canonical repository identity (spec 6.1).
//!
//! # Why this is fussy
//!
//! Every statistic and achievement is keyed by repository identity. If identity
//! flaps, history splits in two; if unrelated repositories collide, statistics
//! merge. The spec therefore fixes a priority order and forbids two specific
//! shortcuts: using the local folder name, and merging same-named repos across
//! hosting platforms.
//!
//! Priority (spec 6.1):
//!   1. GitHub repository id (stable across renames);
//!   2. normalised `github.com/owner/repo`;
//!   3. normalised other remote URL;
//!   4. a locally generated, persisted UUID when there is no remote at all.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use serde::{Deserialize, Serialize};

/// Why a repository identity could not be established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryIdentityError {
    /// A URL that looks like it belongs to a host we handle could not be parsed.
    MalformedUrl(String),
    /// The only candidate identifier was empty.
    EmptyIdentifier,
}

impl std::fmt::Display for RepositoryIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryIdentityError::MalformedUrl(u) => write!(f, "malformed repository URL: {u}"),
            RepositoryIdentityError::EmptyIdentifier => write!(f, "empty repository identifier"),
        }
    }
}

impl std::error::Error for RepositoryIdentityError {}

/// A resolved repository identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryIdentity {
    /// The canonical identifier string persisted with every event.
    pub canonical_id: String,
    /// How it was derived; recorded so diagnostics can explain a split or merge.
    pub origin: IdentityOrigin,
}

/// How a canonical id was derived (spec 6.1 priority order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityOrigin {
    /// Priority 1: GitHub numeric repository id.
    GitHubRepositoryId,
    /// Priority 2: normalised github.com owner/repo.
    GitHubSlug,
    /// Priority 3: normalised other remote URL.
    OtherRemoteUrl,
    /// Priority 4: generated locally because there is no remote.
    LocalUuid,
}

impl RepositoryIdentity {
    /// Build from a GitHub numeric id.
    pub fn from_github_id(id: u64) -> Result<Self, RepositoryIdentityError> {
        if id == 0 {
            return Err(RepositoryIdentityError::EmptyIdentifier);
        }
        Ok(RepositoryIdentity {
            canonical_id: format!("github:{id}"),
            origin: IdentityOrigin::GitHubRepositoryId,
        })
    }

    /// Build from a remote URL, applying spec 6.1 normalisation.
    ///
    /// Handles SSH and HTTPS forms, a trailing `.git`, and case differences in the
    /// host and owner/repo segments. Returns `None` for inputs it does not
    /// recognise, so the caller can fall back to a local UUID instead of inventing
    /// a wrong identity.
    pub fn from_remote_url(raw: &str) -> Option<Self> {
        let (host, path) = split_remote(raw)?;
        let host = host.to_ascii_lowercase();

        // Strip a trailing .git and any trailing slashes.
        let path = path
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .trim_end_matches('/');
        if path.is_empty() {
            return None;
        }

        if host == "github.com" {
            let lower = path.to_ascii_lowercase();
            let parts: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() != 2 {
                return None;
            }
            return Some(RepositoryIdentity {
                canonical_id: format!("github:{}/{}", parts[0], parts[1]),
                origin: IdentityOrigin::GitHubSlug,
            });
        }

        // Other hosts: keep the host and path, normalised, but never merge with
        // GitHub. Spec 6.1 forbids casually merging same-named repos across hosts.
        Some(RepositoryIdentity {
            canonical_id: format!("remote:{}:{}", host, path.to_ascii_lowercase()),
            origin: IdentityOrigin::OtherRemoteUrl,
        })
    }

    /// Build a local-only identity for a repository with no remote.
    ///
    /// `seed` must be stable for the same repository (e.g. a persisted id). Spec
    /// 6.1 explicitly forbids using the folder name, because two different
    /// repositories can share a name.
    pub fn from_local_seed(seed: &str) -> Result<Self, RepositoryIdentityError> {
        let seed = seed.trim();
        if seed.is_empty() {
            return Err(RepositoryIdentityError::EmptyIdentifier);
        }
        Ok(RepositoryIdentity {
            canonical_id: format!("local:{seed}"),
            origin: IdentityOrigin::LocalUuid,
        })
    }
}

/// Split a remote URL into (host, path).
///
/// Supports the forms Git itself accepts:
///   `https://github.com/owner/repo.git`
///   `git@github.com:owner/repo.git`
///   `ssh://git@github.com/owner/repo`
///   `git://host/path`
fn split_remote(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // scheme://[user@]host[:port]/path
    if let Some(scheme_end) = raw.find("://") {
        let rest = &raw[scheme_end + 3..];
        let (authority, path) = match rest.split_once('/') {
            Some((a, p)) => (a, p),
            None => return None,
        };
        let host = authority.rsplit('@').next().unwrap_or(authority);
        let host = host.split(':').next().unwrap_or(host);
        return Some((host.to_string(), path.to_string()));
    }

    // scp-like: [user@]host:path
    if let Some(colon) = raw.find(':') {
        let (authority, path) = raw.split_at(colon);
        let path = &path[1..];
        // Guard against Windows drive letters like "C:\repo".
        if authority.len() == 1
            && authority
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return None;
        }
        let host = authority.rsplit('@').next().unwrap_or(authority);
        return Some((host.to_string(), path.to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_and_ssh_forms_resolve_to_the_same_identity() {
        // The whole point of normalisation: one repository, one identity.
        let a = RepositoryIdentity::from_remote_url("https://github.com/Owner/Repo.git").unwrap();
        let b = RepositoryIdentity::from_remote_url("git@github.com:owner/repo.git").unwrap();
        let c = RepositoryIdentity::from_remote_url("ssh://git@github.com/owner/repo").unwrap();
        assert_eq!(a.canonical_id, b.canonical_id);
        assert_eq!(b.canonical_id, c.canonical_id);
        assert_eq!(a.canonical_id, "github:owner/repo");
    }

    #[test]
    fn trailing_git_suffix_and_slashes_are_normalised() {
        for url in [
            "https://github.com/o/r",
            "https://github.com/o/r.git",
            "https://github.com/o/r/",
            "https://github.com/o/r.git/",
            "git@github.com:o/r.git",
        ] {
            assert_eq!(
                RepositoryIdentity::from_remote_url(url).map(|i| i.canonical_id),
                Some("github:o/r".to_string()),
                "failed for {url}"
            );
        }
    }

    #[test]
    fn case_differences_do_not_split_identity() {
        let a = RepositoryIdentity::from_remote_url("https://GitHub.com/Foo/Bar").unwrap();
        assert_eq!(a.canonical_id, "github:foo/bar");
    }

    #[test]
    fn different_hosts_never_collide_even_with_the_same_slug() {
        // Spec 6.1: do not casually merge same-named repositories across platforms.
        let gh = RepositoryIdentity::from_remote_url("https://github.com/o/r").unwrap();
        let gl = RepositoryIdentity::from_remote_url("https://gitlab.com/o/r").unwrap();
        assert_ne!(gh.canonical_id, gl.canonical_id);
    }

    #[test]
    fn github_id_wins_over_slug_when_available() {
        // Spec 6.1 priority 1: the numeric id survives renames.
        let by_id = RepositoryIdentity::from_github_id(12345).unwrap();
        let by_slug = RepositoryIdentity::from_remote_url("https://github.com/o/r").unwrap();
        assert_eq!(by_id.canonical_id, "github:12345");
        assert_ne!(by_id.canonical_id, by_slug.canonical_id);
        assert_eq!(by_id.origin, IdentityOrigin::GitHubRepositoryId);
    }

    #[test]
    fn local_repositories_use_a_seed_not_a_folder_name() {
        let a = RepositoryIdentity::from_local_seed("11111111-2222").unwrap();
        assert_eq!(a.canonical_id, "local:11111111-2222");
        assert_eq!(a.origin, IdentityOrigin::LocalUuid);
        assert!(RepositoryIdentity::from_local_seed("").is_err());
    }

    #[test]
    fn windows_paths_are_not_mistaken_for_scp_remotes() {
        assert!(RepositoryIdentity::from_remote_url("C:\\Users\\me\\repo").is_none());
    }

    #[test]
    fn unrecognised_input_returns_none_rather_than_a_wrong_identity() {
        assert!(RepositoryIdentity::from_remote_url("").is_none());
        assert!(RepositoryIdentity::from_remote_url("not a url").is_none());
        assert!(RepositoryIdentity::from_remote_url("https://github.com").is_none());
        assert!(
            RepositoryIdentity::from_remote_url("https://github.com/onlyowner").is_none(),
            "a github URL needs owner AND repo"
        );
    }

    #[test]
    fn zero_github_id_is_rejected() {
        assert!(RepositoryIdentity::from_github_id(0).is_err());
    }
}

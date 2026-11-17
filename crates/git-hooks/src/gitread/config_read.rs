//! Reading identity information from Git configuration (spec 7).
//!
//! # Why read config at all, if the user configures emails anyway
//!
//! To offer sensible defaults on first run: most people have `user.email` set
//! already, and asking them to retype it is friction that leads to typos. But what
//! we read is only a *suggestion*, tagged with its origin and lower trust than a
//! value the user confirmed explicitly.
//!
//! # Why two sources
//!
//! A repository can set its own `user.email` (common for work repositories
//! with corporate addresses). Spec 7 lists "personal email, work email, different
//! devices" as distinct entries, so both are collected.

use std::path::Path;
use std::process::Command;

use devchievements_events::{EmailOrigin, Identity};

/// Where a config value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValue {
    /// The value.
    pub value: String,
    /// Where it was read from.
    pub origin: EmailOrigin,
}

/// Read `user.email` from the repository config and the global config.
///
/// Returns whatever was found; an empty vector is normal and means "ask the user".
/// Failure to read config is not an error: it just yields no suggestions.
pub fn read_user_emails(repo_dir: &Path) -> Vec<ConfigValue> {
    let mut found = Vec::new();

    // Repository-local value first: for this repository it is the one actually used.
    if let Some(value) = git_config(repo_dir, &["config", "--local", "--get", "user.email"]) {
        found.push(ConfigValue {
            value,
            origin: EmailOrigin::RepositoryConfig,
        });
    }

    // Then the global value, which may be a different (personal) address.
    if let Some(value) = git_config(repo_dir, &["config", "--global", "--get", "user.email"]) {
        found.push(ConfigValue {
            value,
            origin: EmailOrigin::GlobalConfig,
        });
    }

    found
}

/// Run a git config query and return a non-empty trimmed value.
fn git_config(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        // Never prompt: this runs during startup and must not block.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;

    if !output.status.success() {
        // 'git config --get' exits 1 when the key is absent. That is expected.
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Build an [Identity] from whatever Git configuration is available.
///
/// The result is NOT auto-committed anywhere: the caller shows it as a suggestion.
/// If Git has nothing configured, the returned identity is empty and
/// [Identity::is_configured] is false, so statistics stay blocked (spec 7).
/// The configured remote URL, preferring `origin`.
///
/// Spec 6.1 puts the remote above the local path when deriving repository
/// identity, because the same repository is cloned to different paths on
/// different machines and must still be one repository.
pub fn remote_url(repo_dir: &Path) -> Option<String> {
    // origin first: it is the conventional upstream, and a fork adds others.
    if let Some(url) = git_config(repo_dir, &["remote", "origin", "url"]) {
        let url = url.trim().to_string();
        if !url.is_empty() {
            return Some(url);
        }
    }

    // Any other remote beats nothing: a repository with only `upstream` still has
    // a stable identity.
    let remotes = git_config(repo_dir, &["remote"])?;
    for name in remotes.lines().map(str::trim).filter(|n| !n.is_empty()) {
        if let Some(url) = git_config(repo_dir, &["remote", name, "url"]) {
            let url = url.trim().to_string();
            if !url.is_empty() {
                return Some(url);
            }
        }
    }
    None
}

pub fn suggest_identity(repo_dir: &Path) -> Identity {
    let mut identity = Identity::default();
    for config in read_user_emails(repo_dir) {
        identity.add_email(config.value, config.origin);
    }
    identity
}

/// Whether Git itself has an identity configured, independent of our model.
///
/// Used by onboarding to explain *why* an email is needed.
pub fn git_has_identity(repo_dir: &Path) -> bool {
    git_config(repo_dir, &["config", "--get", "user.email"]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use devchievements_events::EmailOrigin;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// A throwaway repo with its own local config.
    fn isolated_repo(name: &str) -> Option<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("devch-id-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).ok()?;
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .ok()?;
        Some(dir)
    }

    #[test]
    fn a_fresh_repository_has_no_local_email_to_suggest() {
        // This is the state that must BLOCK statistics, so it must be reachable.
        if !git_available() {
            return;
        }
        let Some(dir) = isolated_repo("unset") else {
            return;
        };
        let identity = suggest_identity(&dir);
        let has_local = identity
            .emails
            .iter()
            .any(|e| e.origin == EmailOrigin::RepositoryConfig);
        assert!(!has_local, "a fresh repository has no local user.email");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_repository_email_is_read_and_tagged_with_its_source() {
        if !git_available() {
            return;
        }
        let Some(dir) = isolated_repo("local") else {
            return;
        };
        Command::new("git")
            .args(["config", "--local", "user.email", "work@corp.example"])
            .current_dir(&dir)
            .output()
            .expect("set config");

        let identity = suggest_identity(&dir);
        let found = identity
            .emails
            .iter()
            .find(|e| e.email == "work@corp.example");
        assert!(found.is_some(), "the repository email must be suggested");
        assert_eq!(
            found.map(|e| e.origin),
            Some(EmailOrigin::RepositoryConfig),
            "and tagged so the UI can explain where it came from"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_repository_email_satisfies_the_precondition() {
        if !git_available() {
            return;
        }
        let Some(dir) = isolated_repo("configured") else {
            return;
        };
        Command::new("git")
            .args(["config", "--local", "user.email", "me@example.com"])
            .current_dir(&dir)
            .output()
            .expect("set config");

        assert!(
            suggest_identity(&dir).is_configured(),
            "an email found in config satisfies spec 7's precondition"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_non_repository_yields_no_repository_scoped_email() {
        // Also exercises the GIT_TERMINAL_PROMPT path on a directory git rejects.
        let dir = std::env::temp_dir().join(format!("devch-id-notrepo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let identity = suggest_identity(&dir);
        assert!(!identity
            .emails
            .iter()
            .any(|e| e.origin == EmailOrigin::RepositoryConfig));
        std::fs::remove_dir_all(&dir).ok();
    }
}

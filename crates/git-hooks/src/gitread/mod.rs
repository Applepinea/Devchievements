//! Direct \`.git\` reader: the Hook Helper's fact source (ADR-0002).
//!
//! WHY NOT \`git\` SUBPROCESSES: measured at ~33 ms per invocation on the Phase 0
//! host, versus ~9 ms for an empty native process. A Helper needs ~17 facts, so
//! shelling out costs ~600 ms and busts both the 50 ms p95 target and the 200 ms
//! hard timeout. Reading files directly costs ~10 ms total.
//!
//! SCOPE AND HONEST LIMITATIONS (see ADR-0002):
//! - loose objects are decoded in full (including all DEFLATE block types);
//! - PACKED objects are **not** decoded (no packfile / delta support yet);
//!   callers must treat that as a distinguishable error, never as "no commit";
//! - SHA-256 repositories are not supported;
//! - worktrees and submodules ARE supported via the \`gitdir:\` and \`commondir\` files.

mod inflate;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Errors that the Helper must be able to tell apart.
#[derive(Debug)]
pub enum GitReadError {
    /// No \`.git\` found walking up from the start directory.
    NotARepository,
    /// The repository was found but a required file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// HEAD points at a ref that does not exist (e.g. unborn branch).
    RefNotFound(String),
    /// The object is in a packfile; this reader cannot decode packs yet.
    PackedObjectNotSupported(String),
    /// The object exists but its bytes did not parse.
    Malformed(String),
    /// Decompression failed.
    Inflate(String),
}

impl fmt::Display for GitReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitReadError::NotARepository => write!(f, "not a git repository"),
            GitReadError::Io { path, source } => {
                write!(f, "io error at {}: {source}", path.display())
            }
            GitReadError::RefNotFound(r) => write!(f, "ref not found: {r}"),
            GitReadError::PackedObjectNotSupported(o) => write!(
                f,
                "object {o} is packed; packfile decoding is not implemented yet (ADR-0002)"
            ),
            GitReadError::Malformed(m) => write!(f, "malformed git data: {m}"),
            GitReadError::Inflate(m) => write!(f, "inflate failed: {m}"),
        }
    }
}

impl std::error::Error for GitReadError {}

fn io_err(path: &Path, source: std::io::Error) -> GitReadError {
    GitReadError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A commit as the Hook Helper needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFacts {
    /// Commit object id (hex).
    pub sha: String,
    /// Tree object id (hex).
    pub tree: String,
    /// Parent commit ids, in order.
    pub parents: Vec<String>,
    /// Author name (NOT merged with committer -- spec 7).
    pub author_name: String,
    /// Author email.
    pub author_email: String,
    /// Author timestamp, seconds since the Unix epoch.
    pub author_seconds: i64,
    /// Author UTC offset in minutes, as recorded in the commit.
    pub author_offset_minutes: i32,
    /// Committer name.
    pub committer_name: String,
    /// Committer email.
    pub committer_email: String,
    /// Committer timestamp, seconds since the Unix epoch.
    pub committer_seconds: i64,
    /// Committer UTC offset in minutes.
    pub committer_offset_minutes: i32,
    /// Message subject (first line).
    pub subject: String,
}

impl CommitFacts {
    /// A commit with two or more parents is a merge commit.
    ///
    /// Spec 8.1: merge commits do not count toward ordinary commit statistics.
    pub fn is_merge(&self) -> bool {
        self.parents.len() >= 2
    }
}

/// An opened repository.
#[derive(Debug, Clone)]
pub struct Repository {
    /// The resolved git directory (\`.git\`, or the \`gitdir:\` target for worktrees).
    pub git_dir: PathBuf,
    /// The common directory refs/objects actually live in.
    pub common_dir: PathBuf,
}

impl Repository {
    /// Discover the repository containing \`start\`, walking upward.
    ///
    /// Handles the \`gitdir: <path>\` file form used by worktrees and submodules.
    pub fn discover(start: &Path) -> Result<Self, GitReadError> {
        let mut cur = start.to_path_buf();
        loop {
            let dot_git = cur.join(".git");
            if dot_git.is_dir() {
                let common = common_dir_of(&dot_git);
                return Ok(Repository {
                    git_dir: dot_git,
                    common_dir: common,
                });
            }
            if dot_git.is_file() {
                let text = fs::read_to_string(&dot_git).map_err(|e| io_err(&dot_git, e))?;
                if let Some(rest) = text.trim().strip_prefix("gitdir:") {
                    let p = PathBuf::from(rest.trim());
                    let p = if p.is_absolute() { p } else { cur.join(p) };
                    let common = common_dir_of(&p);
                    return Ok(Repository {
                        git_dir: p,
                        common_dir: common,
                    });
                }
            }
            cur = match cur.parent() {
                Some(p) => p.to_path_buf(),
                None => return Err(GitReadError::NotARepository),
            };
        }
    }

    /// Resolve HEAD to a commit id, following symbolic refs and packed-refs.
    pub fn head_oid(&self) -> Result<String, GitReadError> {
        let head_path = self.git_dir.join("HEAD");
        let head = fs::read_to_string(&head_path).map_err(|e| io_err(&head_path, e))?;
        let head = head.trim();

        let Some(refname) = head.strip_prefix("ref:") else {
            // Detached HEAD: the file contains the oid itself.
            return Ok(head.to_string());
        };
        let refname = refname.trim();

        for root in self.ref_roots() {
            let loose = root.join(refname);
            if let Ok(s) = fs::read_to_string(&loose) {
                return Ok(s.trim().to_string());
            }
            let packed = root.join("packed-refs");
            if let Ok(text) = fs::read_to_string(&packed) {
                for line in text.lines() {
                    if line.starts_with('#') || line.starts_with('^') || line.trim().is_empty() {
                        continue;
                    }
                    if let Some((oid, name)) = line.split_once(' ') {
                        if name.trim() == refname {
                            return Ok(oid.to_string());
                        }
                    }
                }
            }
        }
        Err(GitReadError::RefNotFound(refname.to_string()))
    }

    /// The short branch name, or \`None\` when HEAD is detached.
    pub fn head_branch_name(&self) -> Option<String> {
        let head = fs::read_to_string(self.git_dir.join("HEAD")).ok()?;
        let refname = head.trim().strip_prefix("ref:")?.trim();
        Some(refname.trim_start_matches("refs/heads/").to_string())
    }

    fn ref_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.git_dir.clone()];
        if self.common_dir != self.git_dir {
            roots.push(self.common_dir.clone());
        }
        roots
    }

    /// Read a commit object from the loose object store.
    pub fn read_commit(&self, oid: &str) -> Result<CommitFacts, GitReadError> {
        if oid.len() < 3 {
            return Err(GitReadError::Malformed(format!(
                "object id too short: {oid}"
            )));
        }
        let (dir_part, file_part) = oid.split_at(2);
        let obj_path = self
            .common_dir
            .join("objects")
            .join(dir_part)
            .join(file_part);

        if !obj_path.exists() {
            // Distinguish "missing" from "packed" so callers never silently drop events.
            return Err(GitReadError::PackedObjectNotSupported(oid.to_string()));
        }
        let raw = fs::read(&obj_path).map_err(|e| io_err(&obj_path, e))?;
        let inflated = inflate::zlib_inflate(&raw).map_err(GitReadError::Inflate)?;
        parse_commit_object(&inflated)
    }

    /// Read HEAD's commit facts.
    pub fn head_commit(&self) -> Result<CommitFacts, GitReadError> {
        let oid = self.head_oid()?;
        let mut facts = self.read_commit(&oid)?;
        // read_commit already sets this, but state it here as well so the contract
        // is obvious: facts returned for HEAD always carry HEAD's own id.
        facts.sha = oid;
        Ok(facts)
    }
}

/// Read \`commondir\` if present (worktrees); otherwise the git dir itself.
fn common_dir_of(git_dir: &Path) -> PathBuf {
    let cd_path = git_dir.join("commondir");
    match fs::read_to_string(&cd_path) {
        Ok(text) => {
            let p = PathBuf::from(text.trim());
            let p = if p.is_absolute() { p } else { git_dir.join(p) };
            p.canonicalize().unwrap_or(p)
        }
        Err(_) => git_dir.to_path_buf(),
    }
}

/// Parse a decompressed commit object: header lines, blank line, message.
pub fn parse_commit_object(bytes: &[u8]) -> Result<CommitFacts, GitReadError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| GitReadError::Malformed("commit object is not valid UTF-8".into()))?;

    let body = match text.find('\0') {
        Some(i) => &text[i + 1..],
        None => return Err(GitReadError::Malformed("missing object header".into())),
    };
    if !text.starts_with("commit ") {
        return Err(GitReadError::Malformed("object is not a commit".into()));
    }

    let mut tree = String::new();
    let mut parents: Vec<String> = Vec::new();
    let mut author = (String::new(), String::new(), 0i64, 0i32);
    let mut committer = (String::new(), String::new(), 0i64, 0i32);
    let mut subject = String::new();
    let mut in_message = false;

    for line in body.split('\n') {
        if in_message {
            if subject.is_empty() && !line.trim().is_empty() {
                subject = line.to_string();
            }
            continue;
        }
        if line.is_empty() {
            in_message = true;
            continue;
        }
        if let Some(v) = line.strip_prefix("tree ") {
            tree = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("parent ") {
            parents.push(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("author ") {
            author = parse_ident(v);
        } else if let Some(v) = line.strip_prefix("committer ") {
            committer = parse_ident(v);
        }
    }

    if tree.is_empty() {
        return Err(GitReadError::Malformed("commit has no tree".into()));
    }

    Ok(CommitFacts {
        sha: String::new(), // filled by the caller, which knows the oid
        tree,
        parents,
        author_name: author.0,
        author_email: author.1,
        author_seconds: author.2,
        author_offset_minutes: author.3,
        committer_name: committer.0,
        committer_email: committer.1,
        committer_seconds: committer.2,
        committer_offset_minutes: committer.3,
        subject,
    })
}

/// Parse \`Name <email> 1700000000 +0800\`.
fn parse_ident(s: &str) -> (String, String, i64, i32) {
    let (name, rest) = match s.split_once(" <") {
        Some((n, r)) => (n.to_string(), r),
        None => (s.to_string(), ""),
    };
    let (email, tail): (String, &str) = match rest.split_once("> ") {
        Some((e, t)) => (e.to_string(), t),
        None => (rest.trim_end_matches('>').to_string(), ""),
    };
    let mut parts = tail.split_whitespace();
    let seconds = parts
        .next()
        .and_then(|t| t.parse::<i64>().ok())
        .unwrap_or(0);
    let offset = parts.next().map(parse_offset).unwrap_or(0);
    (name, email, seconds, offset)
}

/// Parse a git UTC offset like \`+0800\` or \`-0130\` into minutes.
fn parse_offset(s: &str) -> i32 {
    let bytes = s.as_bytes();
    if bytes.len() != 5 {
        return 0;
    }
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i32 = s[1..3].parse().unwrap_or(0);
    let minutes: i32 = s[3..5].parse().unwrap_or(0);
    sign * (hours * 60 + minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "commit 233\0tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
parent 1111111111111111111111111111111111111111\n\
parent 2222222222222222222222222222222222222222\n\
author Alice <alice@example.com> 1700000000 +0800\n\
committer Bob <bob@example.com> 1700000060 -0130\n\
\n\
Merge branch 'x'\n";

    #[test]
    fn parses_identity_without_merging_author_and_committer() {
        let c = parse_commit_object(SAMPLE.as_bytes()).expect("sample must parse");
        assert_eq!(c.author_name, "Alice");
        assert_eq!(c.author_email, "alice@example.com");
        assert_eq!(c.committer_name, "Bob");
        assert_eq!(c.committer_email, "bob@example.com");
    }

    #[test]
    fn parses_offsets_including_negative_half_hours() {
        let c = parse_commit_object(SAMPLE.as_bytes()).expect("sample must parse");
        assert_eq!(c.author_offset_minutes, 8 * 60);
        assert_eq!(c.committer_offset_minutes, -90);
    }

    #[test]
    fn detects_merge_commits_by_parent_count() {
        let c = parse_commit_object(SAMPLE.as_bytes()).expect("sample must parse");
        assert_eq!(c.parents.len(), 2);
        assert!(c.is_merge());
    }

    #[test]
    fn extracts_first_line_of_message_as_subject() {
        let c = parse_commit_object(SAMPLE.as_bytes()).expect("sample must parse");
        assert_eq!(c.subject, "Merge branch 'x'");
    }

    #[test]
    fn rejects_non_commit_objects() {
        let e = parse_commit_object(b"blob 3\0abc").unwrap_err();
        assert!(matches!(e, GitReadError::Malformed(_)));
    }

    #[test]
    fn single_parent_commit_is_not_a_merge() {
        let one = "commit 100\0tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\n\
parent 1111111111111111111111111111111111111111\n\
author A <a@b.c> 1 +0000\n\
committer A <a@b.c> 1 +0000\n\n\
feat: x\n";
        let c = parse_commit_object(one.as_bytes()).expect("must parse");
        assert!(!c.is_merge());
    }
}

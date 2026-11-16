//! Phase 0 Git hook recorder (probe only -- never shipped).
//!
//! WHY A RUST BINARY AND NOT A SHELL SCRIPT
//! ----------------------------------------
//! Two independent reasons:
//!
//! 1. The product itself must not depend on sh/bash/Node/Python for Git hooks
//!    (spec 9.2). Installing a native binary is the architecture we actually
//!    intend to measure.
//!
//! 2. On this development host, MSYS sh.exe cannot start at all
//!    ("couldn't create signal pipe, Win32 error 5") because the sandbox forbids
//!    the named pipes MSYS uses for its runtime. Native PE executables run fine,
//!    and Git for Windows resolves 'post-commit' to 'post-commit.exe'. So a Rust
//!    probe is both architecturally correct AND the only thing that works here.
//!
//! WHAT IT RECORDS
//! ---------------
//! One JSON object per invocation, appended to $DEVCHIEVEMENTS_TRACE_FILE:
//! hook name, its argv, the stdin it received (post-rewrite sends the old->new
//! mapping there), HEAD facts, the HEAD reflog tail, and the git dirs.
//!
//! It must never fail the Git operation: main returns () and every failure is
//! swallowed. The product hard budget is 200 ms (spec 9.2); we keep the work to
//! a handful of cheap plumbing calls.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Run a git command in a directory and return trimmed stdout, or "" on failure.
///
/// Output is captured through files rather than pipes: this sandbox denies pipe
/// creation to some process trees, and file redirection is always available.
fn git(dir: &PathBuf, args: &[&str]) -> String {
    let out_file = std::env::temp_dir().join(format!(
        "devch-git-out-{}-{}",
        std::process::id(),
        args.join("_").replace(['/', '\\', ' ', '^', '{', '}'], "_")
    ));
    let out = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&out_file);
    let Ok(out) = out else {
        return String::new();
    };
    let res = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::null())
        .status();
    let text = match res {
        Ok(_) => std::fs::read_to_string(&out_file).unwrap_or_default(),
        Err(_) => String::new(),
    };
    let _ = std::fs::remove_file(&out_file);
    text.trim_end_matches(['\r', '\n']).to_string()
}

/// Minimal JSON string escaping.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

fn main() {
    // Entry timestamp: the spec's Hook timing starts when Git invokes the hook,
    // so this must be the very first thing we do.
    let entry = std::time::Instant::now();

    let trace_path = match std::env::var("DEVCHIEVEMENTS_TRACE_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => return,
    };

    let argv: Vec<String> = std::env::args().collect();

    // Hook name resolution.
    //
    // IMPORTANT: git passes NO hook-name argument to these hooks. post-rewrite
    // does receive one argument, but it is the *rewrite kind* ("amend" /
    // "rebase"), not the hook name; an earlier version of this probe mistook it
    // for the hook name and recorded bogus "amend" hooks.
    //
    // The only reliable source is the executable's own file name, because that
    // is exactly how git locates the hook (post-commit.exe / post-rewrite.exe /
    // post-merge.exe). DEVCHIEVEMENTS_HOOK_NAME exists as an escape hatch for
    // installers that use a wrapper.
    let hook = std::env::var("DEVCHIEVEMENTS_HOOK_NAME")
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        })
        .unwrap_or_else(|| "unknown".into());

    // Everything after argv[0] belongs to git.
    let hook_args: Vec<String> = argv.iter().skip(1).cloned().collect();

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // post-rewrite carries the old->new mapping on stdin.
    let mut stdin_payload = String::new();
    if hook == "post-rewrite" {
        use std::io::Read;
        let _ = std::io::stdin().read_to_string(&mut stdin_payload);
        stdin_payload = stdin_payload.trim().replace('\n', "|");
    }

    let ts_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let head_sha = git(&cwd, &["rev-parse", "HEAD"]);
    let parents = git(&cwd, &["rev-list", "--parents", "-n", "1", "HEAD"]);
    let tree = git(&cwd, &["rev-parse", "HEAD^{tree}"]);
    let branch = git(&cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let author_name = git(&cwd, &["log", "-1", "--pretty=%an", "HEAD"]);
    let author_email = git(&cwd, &["log", "-1", "--pretty=%ae", "HEAD"]);
    let author_ts = git(&cwd, &["log", "-1", "--pretty=%at", "HEAD"]);
    let committer_name = git(&cwd, &["log", "-1", "--pretty=%cn", "HEAD"]);
    let committer_email = git(&cwd, &["log", "-1", "--pretty=%ce", "HEAD"]);
    let committer_ts = git(&cwd, &["log", "-1", "--pretty=%ct", "HEAD"]);
    let subject = git(&cwd, &["log", "-1", "--pretty=%s", "HEAD"]);
    let reflog_gs = git(&cwd, &["reflog", "-1", "--format=%gs"]);
    let reflog_lines = git(&cwd, &["reflog", "--format=%H"]);
    let reflog_count = reflog_lines.lines().filter(|l| !l.is_empty()).count();
    // The question that decides the classification design: when post-commit runs,
    // has the reflog already recorded HEAD?
    let reflog_has_head = if head_sha.is_empty() {
        false
    } else {
        reflog_lines.lines().any(|l| l == head_sha)
    };
    let total_commits = git(&cwd, &["rev-list", "--count", "HEAD"]);
    let git_dir = git(&cwd, &["rev-parse", "--absolute-git-dir"]);
    let common_dir = git(&cwd, &["rev-parse", "--git-common-dir"]);
    let git_version = git(&cwd, &["--version"]);

    let exit_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let self_micros = entry.elapsed().as_micros();

    let line = format!(
        concat!(
            "{{\"hook\":\"{}\",\"ts_ns\":{},\"ts_ns_end\":{},\"self_micros\":{},\"argv\":\"{}\",",
            "\"stdin\":\"{}\",\"head_sha\":\"{}\",\"parents\":\"{}\",\"tree\":\"{}\",",
            "\"branch\":\"{}\",\"author_name\":\"{}\",\"author_email\":\"{}\",\"author_ts\":\"{}\",",
            "\"committer_name\":\"{}\",\"committer_email\":\"{}\",\"committer_ts\":\"{}\",",
            "\"subject\":\"{}\",\"reflog_gs\":\"{}\",\"reflog_count\":{},\"reflog_has_head\":{},",
            "\"total_commits\":\"{}\",\"git_dir\":\"{}\",\"common_dir\":\"{}\",",
            "\"cwd\":\"{}\",\"git_version\":\"{}\"}}\n"
        ),
        esc(&hook),
        ts_ns,
        exit_ns,
        self_micros,
        esc(&hook_args.join(" ")),
        esc(&stdin_payload),
        esc(&head_sha),
        esc(&parents),
        esc(&tree),
        esc(&branch),
        esc(&author_name),
        esc(&author_email),
        esc(&author_ts),
        esc(&committer_name),
        esc(&committer_email),
        esc(&committer_ts),
        esc(&subject),
        esc(&reflog_gs),
        reflog_count,
        if reflog_has_head { "true" } else { "false" },
        esc(&total_commits),
        esc(&git_dir),
        esc(&common_dir),
        esc(&cwd.display().to_string()),
        esc(&git_version),
    );

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&trace_path) {
        let _ = f.write_all(line.as_bytes());
    }
}

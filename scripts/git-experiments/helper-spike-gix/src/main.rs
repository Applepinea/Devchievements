//! Approach B for ADR-0002: open the repository with `gix` and read the commit
//! in-process, with no git subprocess.
//!
//! WHY THIS EXISTS
//! ---------------
//! ADR-0002 selected approach C (parse .git directly) partly because approach B
//! could not even be BUILT on the Phase 0 host: `gix` pulls `windows-sys`, whose
//! build needed `dlltool` -> `as.exe`, which the GNU toolchain lacked.
//!
//! That toolchain gap has since been fixed (scripts/install-toolchain.ps1), so the
//! comparison can finally be made on equal terms. The ADR must rest on measured
//! numbers, not on an environment accident.
//!
//! OUTPUT: one JSON line, matching the shape of the other two spikes so the
//! benchmark can diff them.

use std::time::Instant;

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let started = Instant::now();

    let open_start = Instant::now();
    let repo = match gix::open(".") {
        Ok(r) => r,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"open: {e}\"}}");
            return;
        }
    };
    let open_micros = open_start.elapsed().as_micros();

    let head_start = Instant::now();
    let head = match repo.head_commit() {
        Ok(c) => c,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"head: {e}\"}}");
            return;
        }
    };
    let head_micros = head_start.elapsed().as_micros();

    let sha = head.id().to_string();
    let tree = match head.tree_id() {
        Ok(t) => t.to_string(),
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"tree: {e}\"}}");
            return;
        }
    };
    let parents: Vec<String> = head.parent_ids().map(|id| id.to_string()).collect();

    // gix 0.74 exposes author/committer as Results: signatures can be malformed
    // in the object, so decoding is fallible. Surface that instead of unwrapping.
    let author = match head.author() {
        Ok(a) => a,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"author: {e}\"}}");
            return;
        }
    };
    let committer = match head.committer() {
        Ok(c) => c,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"committer: {e}\"}}");
            return;
        }
    };

    let summary = String::from_utf8_lossy(head.message_raw().unwrap_or_default().as_ref())
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

    let branch = match repo.head_name() {
        Ok(Some(name)) => name.shorten().to_string(),
        Ok(None) => "<detached>".to_string(),
        Err(_) => "<unknown>".to_string(),
    };

    let total_micros = started.elapsed().as_micros();

    println!(
        concat!(
            "{{\"ok\":true,\"approach\":\"gix\",\"sha\":\"{}\",\"tree\":\"{}\",\"parents\":{},",
            "\"branch\":\"{}\",\"author_name\":\"{}\",\"author_email\":\"{}\",\"author_time\":\"{}\",",
            "\"committer_name\":\"{}\",\"committer_email\":\"{}\",\"committer_time\":\"{}\",",
            "\"summary\":\"{}\",",
            "\"open_micros\":{},\"head_micros\":{},\"total_micros\":{}}}"
        ),
        sha,
        tree,
        parents.len(),
        branch,
        json_escape(&author.name.to_string()),
        json_escape(&author.email.to_string()),
        json_escape(author.time.to_string().as_str()),
        json_escape(&committer.name.to_string()),
        json_escape(&committer.email.to_string()),
        json_escape(committer.time.to_string().as_str()),
        json_escape(&summary),
        open_micros,
        head_micros,
        total_micros
    );
}

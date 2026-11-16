//! Phase 0 spike for ADR-0002 (Helper implementation approach).
//!
//! Question: can the Git Hook Helper gather everything it needs WITHOUT spawning
//! 'git', and how fast is that compared to shelling out?
//!
//! The probe benchmark showed one 'git' invocation costs ~33 ms on this host
//! while a trivial native binary costs ~9 ms. So every avoided process spawn is
//! worth ~33 ms inside a hook that has a 200 ms hard timeout and a 50 ms p95
//! target.
//!
//! This binary:
//!   1. opens the current repository in-process with gix,
//!   2. extracts HEAD sha, parents, tree, branch, author/committer identity+time,
//!   3. reads the HEAD reflog tail,
//!   4. reports how long it took.
//!
//! It is a spike, not the product Helper: it only proves feasibility and cost.

use std::time::Instant;

fn main() {
    let total_start = Instant::now();

    // --- open repo (in-process, no git binary) ---
    let open_start = Instant::now();
    let repo = match gix::open(".") {
        Ok(r) => r,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"open: {}\",\"open_micros\":{}}}", e, open_start.elapsed().as_micros());
            return;
        }
    };
    let open_micros = open_start.elapsed().as_micros();

    // --- HEAD commit facts ---
    let head_start = Instant::now();
    let head = match repo.head_commit() {
        Ok(c) => c,
        Err(e) => {
            println!("{{\"ok\":false,\"error\":\"head: {}\",\"open_micros\":{}}}", e, open_micros);
            return;
        }
    };
    let head_micros = head_start.elapsed().as_micros();

    let sha = head.id().to_string();
    let tree = head.tree_id().to_string();
    let parent_count = head.parent_ids().count();

    let author = head.author();
    let author_name = author.name.to_string();
    let author_email = author.email.to_string();
    let author_ts = author.time.seconds;

    let committer = head.committer();
    let committer_name = committer.name.to_string();
    let committer_email = committer.email.to_string();
    let committer_ts = committer.time.seconds;

    let summary = head.message_raw().map(|m| {
        m.lines().next().unwrap_or("").to_string()
    }).unwrap_or_default();

    // --- branch / detached-head state ---
    let branch = match repo.head_name() {
        Ok(Some(name)) => name.shorten().to_string(),
        Ok(None) => "<detached>".to_string(),
        Err(_) => "<unknown>".to_string(),
    };

    // --- reflog tail ---
    let reflog_start = Instant::now();
    let reflog_text = repo
        .head_reflog()
        .ok()
        .and_then(|mut log| {
            // newest entry last
            let all: Vec<_> = std::iter::from_fn(|| log.next()).flatten().collect();
            all.last().map(|e| {
                let msg = e.message.to_string();
                let old = e.previous_oid.map(|o| o.to_string()).unwrap_or_default();
                (old, msg)
            })
        })
        .unwrap_or_default();
    let reflog_micros = reflog_start.elapsed().as_micros();

    let total_micros = total_start.elapsed().as_micros();

    println!(
        concat!(
            "{{\"ok\":true,\"sha\":\"{}\",\"tree\":\"{}\",\"parent_count\":{},",
            "\"branch\":\"{}\",\"author_name\":\"{}\",\"author_email\":\"{}\",\"author_ts\":{},",
            "\"committer_name\":\"{}\",\"committer_email\":\"{}\",\"committer_ts\":{},",
            "\"summary\":\"{}\",\"reflog_prev\":\"{}\",\"reflog_msg\":\"{}\",",
            "\"open_micros\":{},\"head_micros\":{},\"reflog_micros\":{},\"total_micros\":{}}}"
        ),
        sha,
        tree,
        parent_count,
        branch,
        author_name.replace('"', "'"),
        author_email,
        author_ts,
        committer_name.replace('"', "'"),
        committer_email,
        committer_ts,
        summary.replace('"', "'"),
        reflog_text.0,
        reflog_text.1.replace('"', "'").replace('\n', " "),
        open_micros,
        head_micros,
        reflog_micros,
        total_micros
    );
}

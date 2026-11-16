#!/usr/bin/env node
/**
 * scripts/git-experiments/run-experiments.mjs
 *
 * Phase 0 Git behavior experiments (spec sections 9.6 / 9.7 / 30.3).
 *
 * What this does
 * --------------
 * For each scenario it creates a throw-away repository under .out/<scenario>/,
 * installs the hook-recorder as post-commit / post-rewrite / post-merge, drives
 * Git through the scenario with a fixed identity and fixed commit dates, and
 * records every hook invocation as one JSON line.
 *
 * What this proves (or disproves)
 * -------------------------------
 * - whether HEAD's reflog already contains the new commit when post-commit runs;
 * - what reflog message 'git commit --amend' actually writes on this Git version;
 * - the old->new SHA mapping post-rewrite reports;
 * - the order hooks fire in;
 * - whether amend/rebase ever produces a post-commit whose classification is
 *   unambiguous at the time it fires.
 *
 * Output
 * ------
 * .out/<scenario>/trace.jsonl   raw hook invocations
 * .out/<scenario>/summary.json  derived facts for that scenario
 * .out/all-summary.json         everything, for the ADR
 *
 * This script never touches the user's Git config: every setting is passed with
 * 'git -c' or written into the throw-away repo's own .git/config.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(__dirname, ".out");
const hookSource = path.join(__dirname, "hook-recorder.sh");

// Fixed identity and dates: traces must be reproducible.
const IDENTITY = {
  GIT_AUTHOR_NAME: "Devch Experi",
  GIT_AUTHOR_EMAIL: "experiments@devchievements.local",
  GIT_COMMITTER_NAME: "Devch Experi",
  GIT_COMMITTER_EMAIL: "experiments@devchievements.local",
};

// ------------------------------------------------------------------
// HOST CONSTRAINT: this sandbox does not allow a child process to have its
// stdout/stderr captured through pipes (spawn with stdio:'pipe' fails with
// EPERM). We therefore redirect the child's stdout/stderr to real files and
// read those files afterwards. Semantics are identical, only the transport
// differs.
// ------------------------------------------------------------------
const scratchDir = path.join(os.tmpdir(), "devch-git-exp");
fs.mkdirSync(scratchDir, { recursive: true });
let scratchSeq = 0;

function runProcess(cmd, args, opts = {}) {
  const id = `p${scratchSeq++}`;
  const outFile = path.join(scratchDir, id + '.out');
  const errFile = path.join(scratchDir, id + '.err');
  const outFd = fs.openSync(outFile, 'w');
  const errFd = fs.openSync(errFile, 'w');
  let res;
  try {
    res = spawnSync(cmd, args, {
      cwd: opts.cwd,
      env: opts.env,
      stdio: ['ignore', outFd, errFd],
      windowsHide: true,
    });
  } finally {
    fs.closeSync(outFd);
    fs.closeSync(errFd);
  }
  const out = fs.readFileSync(outFile, 'utf8');
  const err = fs.readFileSync(errFile, 'utf8');
  fs.rmSync(outFile, { force: true });
  fs.rmSync(errFile, { force: true });
  return { status: res.status, out, err };
}

function sh(cwd, args, env = {}) {
  const r = runProcess("git", args, { cwd, env: { ...process.env, ...IDENTITY, ...env } });
  if (r.status !== 0) {
    const e = new Error(`git ${args.join(' ')} failed (${r.status}): ${r.err || r.out}`);
    throw e;
  }
  return r.out;
}

function shSafe(cwd, args, env = {}) {
  const r = runProcess("git", args, { cwd, env: { ...process.env, ...IDENTITY, ...env } });
  return { ok: r.status === 0, out: r.status === 0 ? r.out : r.err || r.out };
}

function write(file, content) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content, "utf8");
}

// Where the compiled Rust probe lives.
// The recorder is a native binary, so its name depends on the platform.
const recorderName = process.platform === "win32" ? "devch-hook-recorder.exe" : "devch-hook-recorder";
const recorderExe = path.join(__dirname, "hook-recorder-rs", "target", "release", recorderName);

function installHooks(repo) {
  const hooksDir = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  if (!fs.existsSync(recorderExe)) {
    throw new Error(
      "recorder exe missing: " + recorderExe +
      "\nbuild it with: cargo build --release --manifest-path scripts/git-experiments/hook-recorder-rs/Cargo.toml"
    );
  }
  // HOST + ARCHITECTURE NOTE:
  //   - The product ships a native Rust helper, so a native binary is what we want to measure.
  //   - On this host MSYS sh.exe cannot start at all, and Git for Windows only finds a
  //     hook without an extension if MSYS sh is available. Naming the hook <name>.exe
  //     makes Git exec the PE directly, which works everywhere.
  //   - The experiment records this difference; see ADR-0002.
  for (const name of ["post-commit", "post-rewrite", "post-merge"]) {
    const dst = path.join(hooksDir, name + ".exe");
    fs.copyFileSync(recorderExe, dst);
  }
  // The recorder reads its hook name from argv[1]; git passes none for these hooks,
  // so we provide tiny per-name launchers is not possible without sh. Instead the
  // recorder infers the name from the executable basename when argv is empty.
}

/// Install the recorder into an arbitrary repository path (used for submodules,
/// whose git dir is not <repo>/.git but <parent>/.git/modules/<name>).
function installHooksAt(repoPath) {
  const gitDir = shSafe(repoPath, ["rev-parse", "--absolute-git-dir"]);
  if (!gitDir.ok) return false;
  const hooksDir = path.join(gitDir.out.trim(), "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  if (!fs.existsSync(recorderExe)) throw new Error("recorder exe missing");
  for (const name of ["post-commit", "post-rewrite", "post-merge"]) {
    fs.copyFileSync(recorderExe, path.join(hooksDir, name + ".exe"));
  }
  return true;
}


function newRepo(name) {
  const dir = path.join(outRoot, name);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  const repo = path.join(dir, "repo");
  fs.mkdirSync(repo, { recursive: true });
  sh(repo, ["init", "-q", "-b", "main"]);
  sh(repo, ["config", "user.name", IDENTITY.GIT_AUTHOR_NAME]);
  sh(repo, ["config", "user.email", IDENTITY.GIT_AUTHOR_EMAIL]);
  // Deterministic dates so traces diff cleanly between runs.
  sh(repo, ["config", "commit.gpgsign", "false"]);
  installHooks(repo);
  return { dir, repo, traceFile: path.join(dir, "trace.jsonl") };
}

function resetTrace(t) {
  write(t.traceFile, "");
}

function readTrace(t) {
  if (!fs.existsSync(t.traceFile)) return [];
  return fs
    .readFileSync(t.traceFile, "utf8")
    .split(/\r?\n/)
    .filter((l) => l.trim().length > 0)
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return { parse_error: true, raw: l };
      }
    });
}

let clock = 0;
function commit(repo, message, file = "f.txt", content = null) {
  const c = content ?? `content ${clock++}`;
  write(path.join(repo, file), c + "\n");
  sh(repo, ["add", "-A"]);
  const ts = `${1700000000 + clock * 60} +0000`;
  return sh(repo, ["commit", "-q", "-m", message], {
    GIT_AUTHOR_DATE: ts,
    GIT_COMMITTER_DATE: ts,
  });
}

function headSha(repo) {
  return sh(repo, ["rev-parse", "HEAD"]).trim();
}

// ---------------------------------------------------------------- scenarios

const scenarios = [];

function scenario(name, description, fn) {
  scenarios.push({ name, description, fn });
}

scenario("01-plain-commit", "Two plain commits: baseline hook behavior", (ctx) => {
  const { repo } = ctx;
  commit(repo, "first");
  commit(repo, "second");
  return {
    head: headSha(repo),
    observations: ["post-commit fires once per commit", "no post-rewrite"],
  };
});

scenario("02-amend", "git commit --amend: what reflog says and which hooks fire", (ctx) => {
  const { repo } = ctx;
  commit(repo, "original");
  const before = headSha(repo);
  write(path.join(repo, "f.txt"), "amended\n");
  sh(repo, ["add", "-A"]);
  const ts = `${1700000000 + clock++ * 60} +0000`;
  sh(repo, ["commit", "-q", "--amend", "-m", "amended message"], {
    GIT_AUTHOR_DATE: ts,
    GIT_COMMITTER_DATE: ts,
  });
  const after = headSha(repo);
  return { before, after, changed: before !== after };
});

scenario("03-amend-then-commit", "Amend immediately followed by a normal commit", (ctx) => {
  const { repo } = ctx;
  commit(repo, "c1");
  write(path.join(repo, "f.txt"), "amend me\n");
  sh(repo, ["add", "-A"]);
  const ts1 = `${1700000000 + clock++ * 60} +0000`;
  sh(repo, ["commit", "-q", "--amend", "-m", "c1 amended"], {
    GIT_AUTHOR_DATE: ts1,
    GIT_COMMITTER_DATE: ts1,
  });
  commit(repo, "c2");
  return { head: headSha(repo) };
});

scenario("04-rebase", "Plain rebase of a topic branch onto main", (ctx) => {
  const { repo } = ctx;
  commit(repo, "base");
  sh(repo, ["checkout", "-q", "-b", "topic"]);
  commit(repo, "topic-1");
  commit(repo, "topic-2");
  sh(repo, ["checkout", "-q", "main"]);
  commit(repo, "main-1", "other.txt");
  sh(repo, ["checkout", "-q", "topic"]);
  const before = sh(repo, ["rev-list", "--count", "HEAD"]).trim();
  const r = shSafe(repo, ["rebase", "main"], { GIT_SEQUENCE_EDITOR: "true", GIT_EDITOR: "true" });
  return { ok: r.ok, count_before: before, out: r.out.slice(0, 400) };
});

scenario("05-interactive-rebase-reword", "Interactive rebase with reword: mapping shape", (ctx) => {
  const { repo } = ctx;
  commit(repo, "r1");
  commit(repo, "r2");
  commit(repo, "r3");
  // Reword HEAD only, non-interactively.
  const script = path.join(ctx.dir, "seq-editor.sh");
  write(script, '#!/bin/sh\nsed -i "s/^pick/reword/" "$1"\n');
  fs.chmodSync(script, 0o755);
  const r = shSafe(repo, ["rebase", "-i", "HEAD~1"], {
    GIT_SEQUENCE_EDITOR: script,
    GIT_EDITOR: "true",
  });
  return { ok: r.ok, out: r.out.slice(0, 400) };
});

scenario("06-merge-no-ff", "Local merge --no-ff: post-merge and post-commit order", (ctx) => {
  const { repo } = ctx;
  commit(repo, "base");
  sh(repo, ["checkout", "-q", "-b", "feature"]);
  commit(repo, "feature work", "feature.txt");
  sh(repo, ["checkout", "-q", "main"]);
  commit(repo, "main work", "main.txt");
  const r = shSafe(repo, ["merge", "--no-ff", "--no-edit", "feature"]);
  return { ok: r.ok, out: r.out.slice(0, 400) };
});

scenario("07-merge-ff", "Fast-forward merge: no commit is created, so no post-commit", (ctx) => {
  const { repo } = ctx;
  commit(repo, "base");
  sh(repo, ["checkout", "-q", "-b", "topic"]);
  commit(repo, "topic", "t.txt");
  sh(repo, ["checkout", "-q", "main"]);
  const before = sh(repo, ["rev-parse", "HEAD"]).trim();
  const r = shSafe(repo, ["merge", "--ff-only", "topic"]);
  const after = sh(repo, ["rev-parse", "HEAD"]).trim();
  return {
    ok: r.ok,
    head_changed: before !== after,
    // The product question: a ff merge moves HEAD without creating a commit,
    // so post-merge still fires but post-commit does not.
    note: "ff-merge fast-forwards HEAD; git fires post-merge only",
  };
});

scenario("08-empty-commit", "git commit --allow-empty: is it distinguishable?", (ctx) => {
  const { repo } = ctx;
  commit(repo, "normal");
  const r = shSafe(repo, ["commit", "--allow-empty", "-m", "empty"], {
    GIT_AUTHOR_DATE: "1700000100 +0000",
    GIT_COMMITTER_DATE: "1700000100 +0000",
  });
  const tree1 = sh(repo, ["rev-parse", "HEAD^{tree}"]).trim();
  const tree2 = sh(repo, ["rev-parse", "HEAD~1^{tree}"]).trim();
  return { ok: r.ok, same_tree_as_parent: tree1 === tree2 };
});

scenario("09-cherry-pick", "Cherry-pick: new SHA, patch identical", (ctx) => {
  const { repo } = ctx;
  commit(repo, "base");
  sh(repo, ["checkout", "-q", "-b", "src"]);
  commit(repo, "pickable", "p.txt", "pick me");
  sh(repo, ["checkout", "-q", "main"]);
  const r = shSafe(repo, ["cherry-pick", "src"]);
  const patchSrc = shSafe(repo, ["show", "--format=", "src"]);
  const patchMain = shSafe(repo, ["show", "--format=", "HEAD"]);
  return {
    ok: r.ok,
    same_patch: patchSrc.ok && patchMain.ok && patchSrc.out === patchMain.out,
  };
});

scenario("10-squash", "Squash merge: N commits collapse into 1", (ctx) => {
  const { repo } = ctx;
  commit(repo, "base");
  sh(repo, ["checkout", "-q", "-b", "sq"]);
  commit(repo, "s1", "s1.txt");
  commit(repo, "s2", "s2.txt");
  sh(repo, ["checkout", "-q", "main"]);
  const r = shSafe(repo, ["merge", "--squash", "sq"]);
  const staged = sh(repo, ["diff", "--cached", "--name-only"]).trim();
  shSafe(repo, ["commit", "-q", "-m", "squashed"]);
  return { ok: r.ok, staged_files: staged };
});

scenario("11-worktree", "git worktree add: hook location and git dirs", (ctx) => {
  const { repo, dir } = ctx;
  commit(repo, "base");
  const wt = path.join(dir, "wt");
  const r = shSafe(repo, ["worktree", "add", "-q", "-b", "wtbranch", wt]);
  let wtCommit = null;
  if (r.ok) {
    write(path.join(wt, "wt.txt"), "from worktree\n");
    sh(wt, ["add", "-A"]);
    wtCommit = shSafe(wt, ["commit", "-q", "-m", "worktree commit"], {
      GIT_AUTHOR_DATE: "1700000200 +0000",
      GIT_COMMITTER_DATE: "1700000200 +0000",
    });
  }
  return { ok: r.ok, worktree_created: r.ok, wt_commit_ok: wtCommit?.ok ?? false };
});

scenario("12-custom-hooks-path", "core.hooksPath redirect: does the hook still fire?", (ctx) => {
  const { repo, dir } = ctx;
  const shared = path.join(dir, "shared-hooks");
  fs.mkdirSync(shared, { recursive: true });
  // NOTE: the tracer file is shared, so we can prove the hook ran from the new path.
  if (!fs.existsSync(recorderExe)) throw new Error("recorder exe missing");
  for (const name of ["post-commit", "post-rewrite", "post-merge"]) {
    fs.copyFileSync(recorderExe, path.join(shared, name + ".exe"));
  }
  sh(repo, ["config", "core.hooksPath", shared.replace(/\\/g, "/")]);
  commit(repo, "with custom hooksPath");
  return { hooksPath: shared };
});

scenario("13-existing-hook-chaining", "Pre-existing post-commit is preserved and chained", (ctx) => {
  const { repo } = ctx;
  const marker = path.join(ctx.dir, "user-hook-ran.txt");
  const hooksDir = path.join(repo, ".git", "hooks");
  // Remove the recorder so we observe the user hook alone.
  for (const name of ["post-commit", "post-rewrite", "post-merge"]) {
    for (const ext of ["", ".exe"]) {
      const f = path.join(hooksDir, name + ext);
      if (fs.existsSync(f)) fs.rmSync(f);
    }
  }
  // A "user hook" of the classic kind: a shell script. This is what real Husky /
  // Lefthook / hand-written hooks look like.
  write(path.join(hooksDir, "post-commit"), '#!/bin/sh\necho ran >> "' + marker.replace(/\\/g, "/") + '"\n');
  try {
    fs.chmodSync(path.join(hooksDir, "post-commit"), 0o755);
  } catch {
    /* chmod is a no-op on Windows */
  }
  const r = shSafe(repo, ["commit", "--allow-empty", "-m", "user hook should run"], {
    GIT_AUTHOR_DATE: "1700000500 +0000",
    GIT_COMMITTER_DATE: "1700000500 +0000",
  });
  return {
    commit_ok: r.ok,
    // On this host this MUST be false: the sandbox denies sh.exe.
    user_hook_ran: fs.existsSync(marker),
    // Whether git even tried to run it (exposed via GIT_TRACE in the summary).
    git_output: r.ok ? "" : r.out.slice(0, 200),
    note: "If sh.exe is unavailable, a classic shell hook cannot be chained at all; the installer design must account for that.",
  };
});

scenario("14-submodule", "Submodule: where do the submodule's hooks come from?", (ctx) => {
  const { repo, dir } = ctx;
  commit(repo, "base");
  const sub = path.join(dir, "subsrc");
  fs.mkdirSync(sub, { recursive: true });
  sh(sub, ["init", "-q", "-b", "main"]);
  sh(sub, ["config", "user.name", IDENTITY.GIT_AUTHOR_NAME]);
  sh(sub, ["config", "user.email", IDENTITY.GIT_AUTHOR_EMAIL]);
  write(path.join(sub, "s.txt"), "sub\n");
  sh(sub, ["add", "-A"]);
  sh(sub, ["commit", "-q", "-m", "sub init"], {
    GIT_AUTHOR_DATE: "1700000300 +0000",
    GIT_COMMITTER_DATE: "1700000300 +0000",
  });
  const r = shSafe(repo, [
    "-c",
    "protocol.file.allow=always",
    "submodule",
    "add",
    "-q",
    sub.replace(/\\/g, "/"),
    "sub",
  ]);
  let commitInSub = null;
  let subHooksInfo = null;
  const subPath = path.join(repo, "sub");
  if (r.ok) {
    // Where does git look for a submodule's hooks? Its own .git dir, which for a
    // submodule lives under the parent's .git/modules/<name>.
    const subGitDir = shSafe(subPath, ["rev-parse", "--absolute-git-dir"]).out.trim();
    installHooksAt(subPath);
    write(path.join(subPath, "x.txt"), "x\n");
    shSafe(subPath, ["add", "-A"]);
    commitInSub = shSafe(subPath, ["commit", "-q", "-m", "commit inside submodule"], {
      GIT_AUTHOR_DATE: "1700000400 +0000",
      GIT_COMMITTER_DATE: "1700000400 +0000",
    });
    subHooksInfo = { git_dir: subGitDir, inside_parent: subGitDir.includes("modules") };
  }
  const shBlocked = /couldn't create signal pipe/i.test(r.out ?? "");
  return {
    ok: r.ok,
    commit_in_submodule_ok: commitInSub?.ok ?? false,
    sub_hooks: subHooksInfo,
    // HOST LIMITATION, not a product finding: 'git submodule add' runs the
    // file:// transport through sh.exe, which this sandbox cannot start.
    host_blocked_by_shell: shBlocked,
    err: r.ok ? null : r.out.slice(0, 300),
  };
});

scenario("15-sparse-checkout", "Sparse checkout: hooks still fire normally", (ctx) => {
  const { repo } = ctx;
  commit(repo, "c1", "keep/a.txt");
  commit(repo, "c2", "drop/b.txt");
  sh(repo, ["sparse-checkout", "init", "--cone"]);
  sh(repo, ["sparse-checkout", "set", "keep"]);
  const r = shSafe(repo, ["reset", "--hard", "HEAD"]);
  commit(repo, "c3 in sparse", "keep/c.txt");
  return { ok: r.ok };
});

// ---------------------------------------------------------------- runner

function summarize(name, trace) {
  const byHook = {};
  for (const e of trace) {
    if (!e.hook) continue;
    byHook[e.hook] = (byHook[e.hook] ?? 0) + 1;
  }
  const postCommits = trace.filter((e) => e.hook === "post-commit");
  const postRewrites = trace.filter((e) => e.hook === "post-rewrite");
  return {
    scenario: name,
    hook_counts: byHook,
    order: trace.filter((e) => e.hook).map((e) => e.hook),
    // The critical question: when post-commit runs, is HEAD already in the reflog?
    // The recorder emits JSON booleans. Comparing against 1 silently produced
    // an all-false array and would have led to a wrong ADR conclusion.
    reflog_has_head_on_post_commit: postCommits.map((e) => e.reflog_has_head === true),
    reflog_all_true: postCommits.length > 0 && postCommits.every((e) => e.reflog_has_head === true),
    // What reflog message does this Git version actually write?
    reflog_messages: [...new Set(postCommits.map((e) => e.reflog_gs).filter(Boolean))],
    // post-rewrite detail
    rewrite_kinds: [...new Set(postRewrites.map((e) => e.argv).filter(Boolean))],
    rewrite_mappings: postRewrites.map((e) => e.stdin),
    branches: [...new Set(postCommits.map((e) => e.branch).filter(Boolean))],
    parents_shape: [...new Set(postCommits.map((e) => (e.parents ?? "").trim().split(/\s+/).filter(Boolean).length))],
    git_dir_kinds: [...new Set(postCommits.map((e) => (e.git_dir || "").includes("/worktrees/") ? "worktree" : "main"))],
    total_events: trace.length,
    // How long after the last post-commit does post-rewrite arrive? This is the
    // number the classification wait window has to cover (spec 9.6).
    rewrite_lag_ms: postRewrites.map((rw) => {
      // The relevant gap is: how long after the post-commit that Git ran just
      // before this post-rewrite does the rewrite notification arrive?
      const priorPc = postCommits.filter((pc) => pc.ts_ns <= rw.ts_ns).pop();
      if (!priorPc) return null;
      return Number(((rw.ts_ns - priorPc.ts_ns) / 1e6).toFixed(3));
    }),
    // Time each hook process spent inside git plumbing, for the timing budget.
    hook_self_micros: {
      post_commit: postCommits.map((e) => e.self_micros ?? null),
      post_rewrite: postRewrites.map((e) => e.self_micros ?? null),
    },
  };
}

async function main() {
  fs.rmSync(outRoot, { recursive: true, force: true });
  fs.mkdirSync(outRoot, { recursive: true });

  const all = {
    generated_at: new Date().toISOString(),
    host: { platform: process.platform, arch: process.arch, release: os.release() },
    git_version: sh(__dirname, ["--version"]).trim(),
    scenarios: [],
  };

  for (const s of scenarios) {
    if (!s.name.startsWith(process.env.DEVCH_ONLY ?? "")) continue;
    process.stdout.write(`[run] ${s.name} ... `);
    const ctx = newRepo(s.name);
    process.env.DEVCHIEVEMENTS_TRACE_FILE = ctx.traceFile;
    resetTrace(ctx);
    let result;
    try {
      result = s.fn(ctx);
    } catch (e) {
      result = { error: String(e.message ?? e) };
    }
    const trace = readTrace(ctx);
    const summary = summarize(s.name, trace);
    summary.description = s.description;
    summary.result = result;
    write(path.join(ctx.dir, "summary.json"), JSON.stringify(summary, null, 2) + "\n");
    all.scenarios.push(summary);
    console.log(`${trace.length} events`);
  }

  write(path.join(outRoot, "all-summary.json"), JSON.stringify(all, null, 2) + "\n");
  console.log("\nwrote " + path.join(outRoot, "all-summary.json"));

  // Compact console table
  console.log("\nscenario                        hooks                          reflog@post-commit   reflog msgs");
  for (const s of all.scenarios) {
    const hooks = Object.entries(s.hook_counts).map(([k, v]) => `${k}=${v}`).join(" ") || "-";
    console.log(
      s.scenario.padEnd(32).slice(0, 32) +
        hooks.padEnd(31).slice(0, 31) +
        String(JSON.stringify(s.reflog_has_head_on_post_commit)).padEnd(21).slice(0, 21) +
        JSON.stringify(s.reflog_messages)
    );
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

#!/usr/bin/env node
/**
 * scripts/e2e/hook-to-spool.mjs
 *
 * End-to-end proof of the first half of the Phase 2a loop (spec 32.3):
 *
 *   git commit -> git hook -> devch-helper -> (IPC unavailable) -> Spool
 *
 * This is a REAL repository with a REAL `git commit` and the REAL helper binary.
 * Nothing here is simulated: if the hook script is wrong, or the helper cannot read
 * the repository, or the spool format is broken, this script fails.
 *
 * WHY NOT A RUST TEST: the point is to prove the OS-level plumbing works (Git
 * invoking a shell script which invokes our binary). A Rust unit test bypasses
 * exactly the part most likely to be wrong.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");

const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".out");

const env = { ...process.env };
// Point the helper's spool at the scratch dir instead of the real user data dir.
env.LOCALAPPDATA = scratch;
env.XDG_DATA_HOME = scratch;

function run(exe, args, cwd) {
  const sink = path.join(scratch, "run.log");
  const fd = fs.openSync(sink, "w");
  const r = spawnSync(exe, args, { cwd, env, stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(sink, "utf8") };
}

function fail(msg) {
  console.error("FAIL: " + msg);
  process.exitCode = 1;
}

function main() {
  if (!fs.existsSync(helperExe)) {
    fail("helper binary missing: " + helperExe + " (run: node scripts/cargo-run.mjs build -p devchievements-helper)");
    return;
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });

  const step = (label, result) => {
    console.log((result.status === 0 ? "  ok   " : "  FAIL ") + label);
    if (result.status !== 0) console.log("        " + result.out.trim().split("\n").slice(-3).join("\n        "));
  };

  step("git init", run("git", ["init", "-q", "-b", "main"], repo));
  step("git config user.name", run("git", ["config", "user.name", "E2E Test"], repo));
  step("git config user.email", run("git", ["config", "user.email", "e2e@devchievements.local"], repo));

  // HOST CONSTRAINT (documented in Phase 0): MSYS sh.exe cannot start in this
  // sandbox ("couldn't create signal pipe, Win32 error 5"), so a POSIX shell hook
  // never runs. Git for Windows executes a NATIVE hook named <hook>.exe directly.
  //
  // That is not a substitute for testing the shell path, but it is the only way to
  // exercise the real chain here, and it exercises everything that matters: Git
  // invoking a process, that process reading the repository, and the spool write.
  const hooksDir = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  // The data directory exists after install; the helper refuses to create it
  // (spec 34.1), so the fixture mirrors an installed machine.
  fs.mkdirSync(path.join(env.LOCALAPPDATA ?? process.env.LOCALAPPDATA ?? "", "devchievements"), { recursive: true });

  for (const name of ["post-commit", "post-rewrite", "post-merge"]) {
    fs.copyFileSync(helperExe, path.join(hooksDir, name + ".exe"));
  }

  // Prove the shell path genuinely cannot work here, so the report is honest.
  const shProbe = run("cmd.exe", ["/c", "sh -c \"exit 0\""], repo);
  const shWorks = shProbe.status === 0;
  console.log("  info  POSIX sh available: " + shWorks + (shWorks ? "" : " (shell hooks cannot run on this host)"));

  // A plain commit must now trigger the hook.
  fs.writeFileSync(path.join(repo, "a.txt"), "hello\n");
  step("git add", run("git", ["add", "-A"], repo));
  const commit = run("git", ["commit", "-q", "-m", "first"], repo);
  step("git commit (must not be broken by the hook)", commit);

  // Did the helper actually produce a spool record?
  const spoolDir = path.join(scratch, "devchievements", "spool");
  let records = [];
  let damaged = [];
  if (fs.existsSync(spoolDir)) {
    for (const file of fs.readdirSync(spoolDir)) {
      if (!file.endsWith(".spool")) continue;
      const text = fs.readFileSync(path.join(spoolDir, file), "utf8");
      for (const line of text.split(/\r?\n/)) {
        if (!line.trim()) continue;
        const space = line.indexOf(" ");
        if (space < 0) { damaged.push(line.slice(0, 80)); continue; }
        try {
          const payload = JSON.parse(line.slice(space + 1));
          records.push(payload);
        } catch {
          damaged.push(line.slice(0, 80));
        }
      }
    }
  }

  console.log("");
  console.log("spool dir      : " + (fs.existsSync(spoolDir) ? spoolDir : "(not created)"));
  console.log("records        : " + records.length);
  console.log("damaged lines  : " + damaged.length);

  if (records.length === 0) {
    fail("the hook produced no spool record");
  } else {
    const rec = records.find((r) => r.hook === "post-commit") ?? records[0];
    console.log("record hook    : " + rec.hook);
    console.log("record sha     : " + String(rec.commit_sha).slice(0, 12));
    console.log("record author  : " + rec.author_email);
    if (rec.hook !== "post-commit") fail("expected a post-commit record");
    if (!rec.commit_sha || rec.commit_sha.length !== 40) fail("record has no valid commit sha");
    if (rec.author_email !== "e2e@devchievements.local") fail("record author email is wrong");
  }
  if (damaged.length) fail("spool contains damaged lines");

  // The commit itself must exist: a hook must never break Git.
  const log = run("git", ["log", "--oneline"], repo);
  if (log.out.trim().split("\n").length !== 1) fail("the commit was not created");

  // The hook we installed must still be in place (nothing deleted it behind us).
  if (!fs.existsSync(path.join(repo, ".git", "hooks", "post-commit.exe"))) {
    fail("the hook is missing after the run");
  }

  console.log("");
  if (process.exitCode) {
    console.log("E2E FAILED");
  } else {
    console.log("E2E PASSED: git commit -> hook -> helper -> spool");
  }
}

main();

#!/usr/bin/env node
/**
 * scripts/perf/hook-latency.mjs
 *
 * Spec 44.1 target: the git hook costs a commit less than 50 ms (p95).
 * Spec 44.4 method: from the moment git invokes the hook to the hook process
 * exiting, including process start and the helper's work, clocked from outside.
 *
 * Two numbers, because they answer different questions:
 *   hook   -- spawning the hook exactly as git does (this is the measurement the
 *             spec describes); it includes Node's own spawn cost, so it is an upper
 *             bound, and a control run with a do-nothing process shows how much.
 *   commit -- a full `git commit` with the hook installed, which is what the user
 *             feels when they run git.
 *
 * Usage: node scripts/perf/hook-latency.mjs [--samples 200]
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const outDir = path.join(__dirname, ".out");
const scratch = path.join(outDir, "hook");

function arg(name, fallback) {
  const i = process.argv.indexOf("--" + name);
  return i >= 0 && process.argv[i + 1] ? Number(process.argv[i + 1]) : fallback;
}
const SAMPLES = arg("samples", 200);

function percentile(sorted, p) {
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[index];
}
function stats(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    n: sorted.length,
    min: +sorted[0].toFixed(2),
    p50: +percentile(sorted, 50).toFixed(2),
    p95: +percentile(sorted, 95).toFixed(2),
    max: +sorted[sorted.length - 1].toFixed(2),
  };
}

function now() {
  return Number(process.hrtime.bigint()) / 1e6;
}

function main() {
  if (!fs.existsSync(helperExe)) {
    console.error("missing " + helperExe + "; build it first");
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  const dataDir = path.join(dataRoot, "devchievements");
  // The helper refuses to create the data directory (spec 34.1); the installer does.
  fs.mkdirSync(path.join(dataDir, "spool"), { recursive: true });
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const nullLog = path.join(scratch, "quiet.log");
  const run = (exe, args) => {
    const fd = fs.openSync(nullLog, "w");
    const started = now();
    const r = spawnSync(exe, args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
    const elapsed = now() - started;
    fs.closeSync(fd);
    return { status: r.status, elapsed };
  };

  run("git", ["init", "-q", "-b", "main"]);
  run("git", ["config", "user.name", "Hook Perf"]);
  run("git", ["config", "user.email", "hookperf@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  const hookExe = path.join(hooks, "post-commit.exe");
  fs.copyFileSync(helperExe, hookExe);

  // Control: the cost of Node starting any process at all, so the hook number can
  // be read as "helper work" instead of "process creation on Windows".
  const control = [];
  for (let i = 0; i < SAMPLES; i++) {
    control.push(run("cmd.exe", ["/c", "exit", "0"]).elapsed);
  }

  const hook = [];
  for (let i = 0; i < SAMPLES; i++) {
    const r = run(hookExe, []);
    if (r.status === 0) hook.push(r.elapsed);
  }

  const commit = [];
  for (let i = 0; i < SAMPLES; i++) {
    fs.writeFileSync(path.join(repo, "f.txt"), String(i));
    run("git", ["add", "-A"]);
    const r = run("git", ["commit", "-q", "-m", "hook perf " + i]);
    if (r.status === 0) commit.push(r.elapsed);
  }

  const hookStats = stats(hook);
  const commitStats = stats(commit);
  const controlStats = stats(control);
  const report = {
    measured_at: new Date().toISOString(),
    method:
      "hook: spawn the installed post-commit hook as git does, clocked from the outer script; " +
      "commit: a full `git commit` with the hook installed; control: `cmd /c exit` for the cost of creating a process",
    spec_target_ms: 50,
    device: os.hostname(),
    os: os.version() + " (" + os.release() + ")",
    cpus: os.cpus().length + " x " + (os.cpus()[0] || {}).model,
    git: (spawnSync("git", ["--version"], { encoding: "utf8" }).stdout || "").trim(),
    profile: "debug (dev build, no optimisation)",
    samples: hookStats.n,
    hook_ms: hookStats,
    commit_ms: commitStats,
    control_ms: controlStats,
    within_budget: hookStats.p95 < 50,
  };
  fs.writeFileSync(path.join(outDir, "hook-latency.json"), JSON.stringify(report, null, 2));

  console.log("");
  console.log("control (cmd /c exit)  min=" + controlStats.min + "  p50=" + controlStats.p50 + "  p95=" + controlStats.p95 + " ms");
  console.log("hook                   min=" + hookStats.min + "  p50=" + hookStats.p50 + "  p95=" + hookStats.p95 + "  max=" + hookStats.max + " ms  (n=" + hookStats.n + ")");
  console.log("full git commit        min=" + commitStats.min + "  p50=" + commitStats.p50 + "  p95=" + commitStats.p95 + " ms");
  console.log("target: hook p95 < 50 ms -> " + (report.within_budget ? "WITHIN" : "OVER"));
  console.log("report: " + path.join(outDir, "hook-latency.json"));
  process.exit(report.within_budget ? 0 : 1);
}

main();

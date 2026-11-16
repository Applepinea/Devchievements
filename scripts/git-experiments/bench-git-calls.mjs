#!/usr/bin/env node
/**
 * scripts/git-experiments/bench-git-calls.mjs
 *
 * Phase 0 measurement for ADR-0002 (Helper implementation approach).
 *
 * The hook recorder needs ~20 pieces of information. There are three candidate
 * implementations:
 *   A. spawn 'git' plumbing commands  (what the probe does today)
 *   B. link gix / git2 and read the repo in-process
 *   C. read .git files directly
 *
 * This benchmark measures (A) precisely: the cost of ONE git.exe invocation, and
 * how that multiplies when the Helper needs N of them. That number is compared
 * against the product budgets:
 *   - Hook p95 execution time target : < 50 ms   (spec 44.1)
 *   - Hook hard timeout              : 200 ms   (spec 9.2)
 *
 * Method: for each command, run it REPS times, record wall time per invocation,
 * report min / p50 / p95 / max. Every call is a fresh process, exactly like the
 * real Helper would do it.
 *
 * NOTE ON MEASUREMENT TRANSPORT: this sandbox denies pipe-based stdio capture to
 * some process trees, so child output is redirected to files and timing is taken
 * around spawnSync itself (which is what we care about: process turnaround).
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const scratch = path.join(__dirname, ".out", "bench");
fs.rmSync(scratch, { recursive: true, force: true });
fs.mkdirSync(scratch, { recursive: true });

const REPS = Number(process.env.BENCH_REPS ?? 30);

function pct(sorted, p) {
  if (sorted.length === 0) return null;
  const i = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[i];
}

function timeOne(cwd, args) {
  const outFile = path.join(scratch, "o.txt");
  const fd = fs.openSync(outFile, "w");
  const t0 = process.hrtime.bigint();
  try {
    spawnSync("git", args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
  } finally {
    fs.closeSync(fd);
  }
  return Number(process.hrtime.bigint() - t0) / 1e6;
}

function bench(label, cwd, args, reps = REPS) {
  timeOne(cwd, args); // warm-up, discarded
  const samples = [];
  for (let i = 0; i < reps; i++) samples.push(timeOne(cwd, args));
  samples.sort((a, b) => a - b);
  return {
    label,
    n: samples.length,
    min_ms: +samples[0].toFixed(3),
    p50_ms: +pct(samples, 50).toFixed(3),
    p95_ms: +pct(samples, 95).toFixed(3),
    max_ms: +samples[samples.length - 1].toFixed(3),
  };
}

function main() {
  // Build a small repo shaped like a real one.
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const g = (args) => spawnSync("git", args, { cwd: repo, stdio: "ignore", windowsHide: true });
  g(["init", "-q", "-b", "main"]);
  g(["config", "user.name", "bench"]);
  g(["config", "user.email", "bench@local"]);
  for (let i = 0; i < 20; i++) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "x" + i + "\n");
    g(["add", "-A"]);
    g(["commit", "-q", "-m", "c" + i]);
  }

  // The exact plumbing the recorder performs today.
  const calls = [
    ["rev-parse HEAD"],
    ["rev-list --parents -n 1 HEAD"],
    ["rev-parse HEAD^{tree}"],
    ["symbolic-ref --quiet --short HEAD"],
    ["log -1 --pretty=%an HEAD"],
    ["log -1 --pretty=%ae HEAD"],
    ["log -1 --pretty=%at HEAD"],
    ["log -1 --pretty=%cn HEAD"],
    ["log -1 --pretty=%ce HEAD"],
    ["log -1 --pretty=%ct HEAD"],
    ["log -1 --pretty=%s HEAD"],
    ["reflog -1 --format=%gs"],
    ["reflog --format=%H"],
    ["rev-list --count HEAD"],
    ["rev-parse --absolute-git-dir"],
    ["rev-parse --git-common-dir"],
    ["--version"],
  ];

  const results = calls.map(([a]) => bench(a, repo, a.split(" ")));
  const p50s = results.map((r) => r.p50_ms);
  const totalP50 = p50s.reduce((a, b) => a + b, 0);
  const totalP95 = results.reduce((a, b) => a + r_p95(b), 0);

  // A single combined call as a lower bound for "fewer process spawns".
  const combined = bench('rev-parse HEAD + log (2 calls)', repo, ["log", "-1", "--pretty=%H%n%an%n%ae%n%at%n%cn%n%ce%n%ct%n%s", "HEAD"]);

  const report = {
    generated_at: new Date().toISOString(),
    host: { platform: process.platform, arch: process.arch, release: os.release(), cpus: os.cpus().length },
    git_version: (() => {
      const f = path.join(scratch, "v.txt");
      const fd = fs.openSync(f, "w");
      spawnSync("git", ["--version"], { stdio: ["ignore", fd, fd] });
      fs.closeSync(fd);
      return fs.readFileSync(f, "utf8").trim();
    })(),
    reps_per_case: REPS,
    per_call: results,
    combined_log_call: combined,
    // If the Helper shells out to git for every field, this is its floor:
    total_p50_ms: +totalP50.toFixed(3),
    total_p95_ms: +totalP95.toFixed(3),
    call_count: calls.length,
    budgets: { hook_p95_target_ms: 50, hook_hard_timeout_ms: 200 },
    verdict: {
      shelling_out_p50_exceeds_p95_target: totalP50 > 50,
      shelling_out_p50_exceeds_hard_timeout: totalP50 > 200,
    },
  };

  const outFile = path.join(__dirname, ".out", "bench-git-calls.json");
  fs.writeFileSync(outFile, JSON.stringify(report, null, 2) + "\n", "utf8");

  console.log("per-call timings (ms):");
  for (const r of results) {
    console.log("  " + r.label.padEnd(34) + " p50=" + String(r.p50_ms).padStart(7) + "  p95=" + String(r.p95_ms).padStart(7));
  }
  console.log("");
  console.log("TOTAL for " + calls.length + " git calls: p50=" + totalP50.toFixed(1) + " ms, p95=" + totalP95.toFixed(1) + " ms");
  console.log("budget: hook p95 < 50 ms, hard timeout 200 ms");
  console.log("verdict: " + JSON.stringify(report.verdict));
  console.log("wrote " + outFile);
}

function r_p95(r) {
  return r.p95_ms;
}

main();

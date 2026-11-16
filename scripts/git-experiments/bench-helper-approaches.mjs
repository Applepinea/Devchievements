#!/usr/bin/env node
/**
 * scripts/git-experiments/bench-helper-approaches.mjs
 *
 * ADR-0002 evidence: three candidate Helper implementations, measured on the
 * same repository with the same facts requested.
 *
 *   A. shell out to 'git' plumbing        (bench-git-calls.mjs, 17 calls)
 *   B. link gix / git2                    (could not be built here -- see ADR-0002)
 *   C. read .git files directly           (helper-spike-direct)
 *
 * This script times A and C end to end, from a cold process each time, and
 * reports min / p50 / p95 / max, because the product budgets are p95-based
 * (hook p95 < 50 ms, hard timeout 200 ms).
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, ".out");
fs.mkdirSync(outDir, { recursive: true });

const REPS = Number(process.env.BENCH_REPS ?? 40);
const DIRECT_EXE = path.join(__dirname, "helper-spike-direct", "target", "release", "devch-helper-spike-direct.exe");

function pct(s, p) {
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
}

function stats(samples) {
  const s = [...samples].sort((a, b) => a - b);
  return {
    n: s.length,
    min: +s[0].toFixed(2),
    p50: +pct(s, 50).toFixed(2),
    p95: +pct(s, 95).toFixed(2),
    max: +s[s.length - 1].toFixed(2),
  };
}

function timedSpawn(exe, args, cwd) {
  const sink = path.join(outDir, "sink.txt");
  const fd = fs.openSync(sink, "w");
  const t0 = process.hrtime.bigint();
  spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  fs.closeSync(fd);
  return ms;
}

function main() {
  const repo = path.join(outDir, "adr0002-repo");
  fs.rmSync(repo, { recursive: true, force: true });
  fs.mkdirSync(repo, { recursive: true });
  const g = (a) => spawnSync("git", a, { cwd: repo, stdio: "ignore", windowsHide: true });
  g(["init", "-q", "-b", "main"]);
  g(["config", "user.name", "ADR Test"]);
  g(["config", "user.email", "adr@devchievements.local"]);
  for (let i = 0; i < 30; i++) {
    fs.writeFileSync(path.join(repo, "file" + i + ".txt"), "content " + i + "\n");
    g(["add", "-A"]);
    g(["commit", "-q", "-m", "commit " + i]);
  }

  // ---- Approach A: one round-trip that collects everything in ONE git call,
  // which is the cheapest possible shelling-out design. If even this loses,
  // shelling out is definitively out.
  const oneShotArgs = ["log", "-1", "--pretty=%H|%T|%P|%an|%ae|%at|%cn|%ce|%ct|%s", "HEAD"];
  const singleCall = [];
  for (let i = 0; i < REPS; i++) singleCall.push(timedSpawn("git", oneShotArgs, repo));

  // ---- Approach A': the 17 separate calls the naive Helper would make.
  const manyArgs = [
    ["rev-parse", "HEAD"],
    ["rev-parse", "HEAD^{tree}"],
    ["rev-list", "--parents", "-n", "1", "HEAD"],
    ["log", "-1", "--pretty=%an"],
    ["log", "-1", "--pretty=%ae"],
    ["log", "-1", "--pretty=%at"],
    ["log", "-1", "--pretty=%cn"],
    ["log", "-1", "--pretty=%ce"],
    ["log", "-1", "--pretty=%ct"],
    ["log", "-1", "--pretty=%s"],
    ["reflog", "-1", "--format=%gs"],
  ];
  const multiCall = [];
  for (let i = 0; i < REPS; i++) {
    let total = 0;
    for (const a of manyArgs) total += timedSpawn("git", a, repo);
    multiCall.push(total);
  }

  // ---- Approach C: our own binary, one process, zero git spawns.
  const direct = [];
  if (fs.existsSync(DIRECT_EXE)) {
    for (let i = 0; i < REPS; i++) direct.push(timedSpawn(DIRECT_EXE, [], repo));
  }

  // ---- Sanity: does C actually produce the right facts?
  let directOutput = null;
  let directOk = false;
  if (fs.existsSync(DIRECT_EXE)) {
    const outFile = path.join(outDir, "direct-out.json");
    const fd = fs.openSync(outFile, "w");
    spawnSync(DIRECT_EXE, [], { cwd: repo, stdio: ["ignore", fd, fd] });
    fs.closeSync(fd);
    directOutput = fs.readFileSync(outFile, "utf8").trim();
    try {
      const parsed = JSON.parse(directOutput);
      const headSha = (() => {
        const f = path.join(outDir, "h.txt");
        const hfd = fs.openSync(f, "w");
        spawnSync("git", ["rev-parse", "HEAD"], { cwd: repo, stdio: ["ignore", hfd, hfd] });
        fs.closeSync(hfd);
        return fs.readFileSync(f, "utf8").trim();
      })();
      directOk = parsed.ok === true && parsed.sha === headSha && parsed.parents === 1;
    } catch {
      directOk = false;
    }
  }

  const report = {
    generated_at: new Date().toISOString(),
    reps: REPS,
    approach_a_single_git_call: stats(singleCall),
    approach_a_multi_git_calls: stats(multiCall),
    approach_c_direct_file_read: direct.length ? stats(direct) : null,
    approach_c_correctness_check: { ok: directOk, sample: directOutput },
    budget: { hook_p95_target_ms: 50, hook_hard_timeout_ms: 200 },
    verdict: {},
  };

  const c = report.approach_c_direct_file_read;
  const a1 = report.approach_a_single_git_call;
  report.verdict = {
    single_git_call_within_p95_target: a1.p95 < 50,
    single_git_call_within_hard_timeout: a1.p95 < 200,
    direct_read_within_p95_target: c ? c.p95 < 50 : null,
    direct_read_faster_than_single_git_call: c ? c.p50 < a1.p50 : null,
  };

  fs.writeFileSync(path.join(outDir, "bench-helper-approaches.json"), JSON.stringify(report, null, 2) + "\n");

  const rows = [
    ["A  one git call (cheapest shell-out)", a1],
    ["A' 11 git calls (naive shell-out)", report.approach_a_multi_git_calls],
    ["C  direct .git read (1 native process)", c],
  ];
  console.log("approach".padEnd(40) + "min".padStart(9) + "p50".padStart(9) + "p95".padStart(9) + "max".padStart(9));
  for (const [label, s] of rows) {
    if (!s) continue;
    console.log(label.padEnd(40) + String(s.min).padStart(9) + String(s.p50).padStart(9) + String(s.p95).padStart(9) + String(s.max).padStart(9));
  }
  console.log("");
  console.log("correctness of approach C:", JSON.stringify(report.approach_c_correctness_check.ok));
  if (!report.approach_c_correctness_check.ok) console.log("  sample:", directOutput);
  console.log("verdict:", JSON.stringify(report.verdict));
  console.log("wrote", path.join(outDir, "bench-helper-approaches.json"));
}

main();

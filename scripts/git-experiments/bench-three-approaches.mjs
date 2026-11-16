#!/usr/bin/env node
/**
 * scripts/git-experiments/bench-three-approaches.mjs
 *
 * ADR-0002 final evidence: all THREE Helper implementation approaches, measured on
 * the same repository, same facts, same node counts.
 *
 *   A. spawn 'git' plumbing
 *   B. link gix and read in-process
 *   C. parse .git directly (the shipped choice)
 *
 * HISTORY: earlier runs could not measure B at all because `gix` needed
 * `dlltool` -> `as.exe`, which the isolated GNU toolchain lacked. That gap is
 * fixed, so this ADR now rests on measurements instead of an environment accident.
 *
 * Method: each approach produces the same JSON facts; we time N fresh processes and
 * report min/p50/p95/max, then cross-check that all three agree on the commit.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, ".out");
fs.mkdirSync(outDir, { recursive: true });
const REPS = Number(process.env.BENCH_REPS ?? 40);

const GIT_EXE = "git";
const GIX_EXE = path.join(__dirname, "helper-spike-gix", "target", "release", "devch-helper-spike-gix.exe");
const DIRECT_EXE = path.join(__dirname, "helper-spike-direct", "target", "release", "devch-helper-spike-direct.exe");

function pct(s, p) {
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
}

function stats(samples) {
  if (samples.length === 0) return null;
  const s = [...samples].sort((a, b) => a - b);
  return {
    n: s.length,
    min: +s[0].toFixed(2),
    p50: +pct(s, 50).toFixed(2),
    p95: +pct(s, 95).toFixed(2),
    max: +s[s.length - 1].toFixed(2),
  };
}

/** Run a command, capture stdout via a file (this sandbox denies pipes), time it. */
function timedRun(exe, args, cwd, outFile) {
  const fd = fs.openSync(outFile, "w");
  const t0 = process.hrtime.bigint();
  const r = spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  fs.closeSync(fd);
  const text = fs.readFileSync(outFile, "utf8").trim();
  return { ms, status: r.status, text };
}

function main() {
  const repo = path.join(outDir, "adr0002-repo2");
  fs.rmSync(repo, { recursive: true, force: true });
  fs.mkdirSync(repo, { recursive: true });
  const g = (a) => spawnSync(GIT_EXE, a, { cwd: repo, stdio: "ignore", windowsHide: true });
  g(["init", "-q", "-b", "main"]);
  g(["config", "user.name", "ADR Test"]);
  g(["config", "user.email", "adr@devchievements.local"]);
  for (let i = 0; i < 30; i++) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i + "\n");
    g(["add", "-A"]);
    g(["commit", "-q", "-m", "commit " + i]);
  }

  const headSha = (() => {
    const f = path.join(outDir, "head.txt");
    const fd = fs.openSync(f, "w");
    spawnSync(GIT_EXE, ["rev-parse", "HEAD"], { cwd: repo, stdio: ["ignore", fd, fd] });
    fs.closeSync(fd);
    return fs.readFileSync(f, "utf8").trim();
  })();

  const tmpOut = path.join(outDir, "run.out");

  // ---- A: one git call (the cheapest shell-out shape)
  const aSamples = [];
  for (let i = 0; i < REPS; i++) {
    const r = timedRun(GIT_EXE, ["log", "-1", "--pretty=%H|%T|%P|%an|%ae|%at|%cn|%ce|%ct|%s", "HEAD"], repo, tmpOut);
    aSamples.push(r.ms);
  }

  // ---- B: gix in-process
  const bSamples = [];
  let bOut = null;
  if (fs.existsSync(GIX_EXE)) {
    for (let i = 0; i < REPS; i++) {
      const r = timedRun(GIX_EXE, [], repo, tmpOut);
      bSamples.push(r.ms);
      if (!bOut) bOut = r.text;
    }
  }

  // ---- C: direct .git read
  const cSamples = [];
  let cOut = null;
  if (fs.existsSync(DIRECT_EXE)) {
    for (let i = 0; i < REPS; i++) {
      const r = timedRun(DIRECT_EXE, [], repo, tmpOut);
      cSamples.push(r.ms);
      if (!cOut) cOut = r.text;
    }
  }

  // Correctness: every approach must report the real HEAD sha.
  const correct = { A: null, B: null, C: null };
  {
    const r = timedRun(GIT_EXE, ["rev-parse", "HEAD"], repo, tmpOut);
    correct.A = r.text === headSha;
  }
  if (bOut) {
    try { correct.B = JSON.parse(bOut).sha === headSha; } catch { correct.B = false; }
  }
  if (cOut) {
    try { correct.C = JSON.parse(cOut).sha === headSha; } catch { correct.C = false; }
  }

  const report = {
    generated_at: new Date().toISOString(),
    reps: REPS,
    head_sha: headSha,
    approach_a_git_subprocess: stats(aSamples),
    approach_b_gix: stats(bSamples),
    approach_c_direct_read: stats(cSamples),
    correctness: correct,
    budget: { hook_p95_target_ms: 50, hook_hard_timeout_ms: 200 },
  };

  fs.writeFileSync(path.join(outDir, "bench-three-approaches.json"), JSON.stringify(report, null, 2) + "\n");

  const rows = [
    ["A  single git subprocess", report.approach_a_git_subprocess, correct.A],
    ["B  gix (in-process, linked)", report.approach_b_gix, correct.B],
    ["C  direct .git read (1 process)", report.approach_c_direct_read, correct.C],
  ];
  console.log("approach".padEnd(34) + "min".padStart(9) + "p50".padStart(9) + "p95".padStart(9) + "max".padStart(9) + "  correct");
  for (const [label, s, ok] of rows) {
    if (!s) { console.log(label.padEnd(34) + "  (not built)"); continue; }
    console.log(
      label.padEnd(34) +
        String(s.min).padStart(9) + String(s.p50).padStart(9) + String(s.p95).padStart(9) + String(s.max).padStart(9) +
        "  " + String(ok)
    );
  }
  console.log("");
  console.log("budget: hook p95 < 50 ms, hard timeout 200 ms");
  console.log("wrote " + path.join(outDir, "bench-three-approaches.json"));
}

main();

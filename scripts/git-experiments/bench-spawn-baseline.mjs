#!/usr/bin/env node
/**
 * scripts/git-experiments/bench-spawn-baseline.mjs
 *
 * Companion to bench-git-calls.mjs.
 *
 * That benchmark showed one 'git' invocation costs ~35 ms on this host, which
 * sounds implausible for a thin plumbing command. Before blaming git, we must
 * separate two effects:
 *
 *   1. cost of creating ANY child process in this environment, and
 *   2. cost that is specific to starting git (config parsing, pager detection,
 *      locale, fsmonitor, etc).
 *
 * We therefore time:
 *   - a trivial native command   (cmd.exe /c exit)
 *   - a trivial PE we build here (a Rust binary that does nothing and exits)
 *   - 'git --version'            (git startup, no repo)
 *   - 'git rev-parse HEAD'       (git startup + repo discovery)
 *
 * If the trivial PE is also ~30 ms, the number is process-creation overhead and
 * the ADR must say so rather than blaming git.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const scratch = path.join(__dirname, ".out", "bench-spawn");
fs.rmSync(scratch, { recursive: true, force: true });
fs.mkdirSync(scratch, { recursive: true });

const REPS = Number(process.env.BENCH_REPS ?? 40);

function pct(s, p) {
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
}

function timeCmd(exe, args, cwd, reps) {
  const sink = path.join(scratch, "sink.txt");
  const fd = fs.openSync(sink, "w");
  // warm-up
  spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
  const samples = [];
  for (let i = 0; i < reps; i++) {
    const t0 = process.hrtime.bigint();
    spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
    samples.push(Number(process.hrtime.bigint() - t0) / 1e6);
  }
  fs.closeSync(fd);
  samples.sort((a, b) => a - b);
  return {
    n: samples.length,
    min: +samples[0].toFixed(2),
    p50: +pct(samples, 50).toFixed(2),
    p95: +pct(samples, 95).toFixed(2),
  };
}

function main() {
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repo, stdio: "ignore" });
  spawnSync("git", ["config", "user.name", "b"], { cwd: repo, stdio: "ignore" });
  spawnSync("git", ["config", "user.email", "b@l"], { cwd: repo, stdio: "ignore" });
  fs.writeFileSync(path.join(repo, "a.txt"), "a\n");
  spawnSync("git", ["add", "-A"], { cwd: repo, stdio: "ignore" });
  spawnSync("git", ["commit", "-q", "-m", "c"], { cwd: repo, stdio: "ignore" });

  const trivialExe = process.env.DEVCH_TRIVIAL_EXE;
  const out = {
    generated_at: new Date().toISOString(),
    reps: REPS,
    cases: {},
  };

  out.cases["cmd /c exit"] = timeCmd("cmd.exe", ["/c", "exit"], repo, REPS);
  if (trivialExe && fs.existsSync(trivialExe)) {
    out.cases["rust no-op binary"] = timeCmd(trivialExe, [], repo, REPS);
  }
  out.cases["git --version"] = timeCmd("git", ["--version"], repo, REPS);
  out.cases["git rev-parse HEAD"] = timeCmd("git", ["rev-parse", "HEAD"], repo, REPS);

  fs.writeFileSync(path.join(__dirname, ".out", "bench-spawn-baseline.json"), JSON.stringify(out, null, 2) + "\n");

  console.log("case".padEnd(24) + "min".padStart(8) + "p50".padStart(8) + "p95".padStart(8));
  for (const [k, v] of Object.entries(out.cases)) {
    console.log(k.padEnd(24) + String(v.min).padStart(8) + String(v.p50).padStart(8) + String(v.p95).padStart(8));
  }
  const base = out.cases["cmd /c exit"].p50;
  const gitv = out.cases["git --version"].p50;
  console.log("");
  console.log("process-creation floor (cmd.exe): " + base + " ms");
  console.log("git startup above floor:          " + (gitv - base).toFixed(2) + " ms");
  console.log("=> 'per-call git cost' is dominated by " + (base / gitv > 0.6 ? "PROCESS CREATION, not git itself" : "git startup"));
}

main();

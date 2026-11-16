#!/usr/bin/env node
/**
 * scripts/verify.mjs
 *
 * The single command that answers "is this repo actually healthy?".
 *
 * WHY A SCRIPT: verification here needs a specific environment (isolated Rust
 * toolchain, mingw C toolchain, local crates proxy) and several of the host's
 * quirks make naive invocations lie (PowerShell swallows cargo stderr; child
 * processes cannot use pipes). Encoding the correct invocations once means nobody
 * has to rediscover them, and STATUS.md can cite one reproducible command.
 *
 * It runs, in order: fmt check, clippy, workspace tests, workspace build, wasm
 * build, and the extension build. Every step's tail is printed; a machine-readable
 * summary is written to .verify/summary.json.
 *
 * Exit code is non-zero if any step fails.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");
const outDir = path.join(repoRoot, ".verify");
fs.mkdirSync(outDir, { recursive: true });

const cargo = path.join(toolchain, "cargo", "bin", "cargo.exe");
const mingw = path.join(toolchain, "_mingw-gcc", "mingw64", "bin");
const selfContained = path.join(
  toolchain, "rustup", "toolchains", "stable-x86_64-pc-windows-gnu",
  "lib", "rustlib", "x86_64-pc-windows-gnu", "bin", "self-contained"
);

const env = { ...process.env };
env.RUSTUP_HOME = path.join(toolchain, "rustup");
env.CARGO_HOME = path.join(toolchain, "cargo");
env.PATH = [mingw, selfContained, path.join(toolchain, "cargo", "bin"), env.PATH ?? ""].join(";");
if (fs.existsSync(path.join(mingw, "gcc.exe"))) env.CC = path.join(mingw, "gcc.exe");

const results = [];

/** Run a step, capturing output to a file (pipes are unavailable in this sandbox). */
function step(name, cmd, args, cwd = repoRoot) {
  const logFile = path.join(outDir, name.replace(/[^a-z0-9]+/gi, "-") + ".log");
  const fd = fs.openSync(logFile, "w");
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cmd, args, { cwd, env, stdio: ["ignore", fd, fd], windowsHide: true });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  fs.closeSync(fd);
  const log = fs.readFileSync(logFile, "utf8");
  const ok = r.status === 0;

  // Extract the lines a human actually needs.
  let summary = "";
  if (/test$/.test(args.join(" "))) {
    const totals = [...log.matchAll(/test result: (\w+)\. (\d+) passed/g)];
    const passed = totals.reduce((a, m) => a + Number(m[2]), 0);
    summary = passed + " tests passed";
  } else if (log.includes("Finished")) {
    summary = "finished";
  }

  results.push({ name, ok, exit: r.status, ms: +ms.toFixed(0), logFile: path.relative(repoRoot, logFile), summary });
  console.log(
    (ok ? "PASS  " : "FAIL  ") + name.padEnd(22) + String(Math.round(ms)).padStart(7) + " ms  " + summary
  );
  if (!ok) {
    const lines = log.split(/\r?\n/).filter((l) => /^error|FAILED|panicked/.test(l));
    for (const l of lines.slice(0, 6)) console.log("        " + l);
  }
  return ok;
}

function main() {
  if (!fs.existsSync(cargo)) {
    console.error("cargo not found: " + cargo);
    console.error("run scripts/install-toolchain.ps1 first");
    process.exit(2);
  }

  // 1. Rust
  step("fmt", cargo, ["fmt", "--all", "--", "--check"]);
  step("clippy", cargo, ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
  step("test", cargo, ["test", "--workspace"]);
  step("build", cargo, ["build", "--workspace"]);
  step("build-wasm", cargo, ["build", "-p", "devchievements-core", "--target", "wasm32-unknown-unknown", "--release"]);

  // 2. Extension (optional: needs node_modules)
  const extDir = path.join(repoRoot, "apps", "extension");
  const wxt = path.join(repoRoot, "node_modules", "wxt", "bin", "wxt.mjs");
  if (fs.existsSync(wxt)) {
    step("extension-build", process.execPath, [wxt, "build"], extDir);
  } else {
    results.push({ name: "extension-build", ok: null, skipped: "wxt not installed (run: pnpm install --ignore-scripts)" });
    console.log("SKIP  extension-build      (wxt not installed)");
  }

  const failed = results.filter((r) => r.ok === false);
  fs.writeFileSync(
    path.join(outDir, "summary.json"),
    JSON.stringify({ generated_at: new Date().toISOString(), results, failed: failed.length }, null, 2) + "\n"
  );

  console.log("");
  if (failed.length === 0) {
    console.log("ALL CHECKS PASSED");
    process.exit(0);
  }
  console.log(failed.length + " CHECK(S) FAILED: " + failed.map((f) => f.name).join(", "));
  process.exit(1);
}

main();

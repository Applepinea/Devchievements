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

// Child processes get a TMP/TEMP inside the repository. Measured on this host:
// writing to %LOCALAPPDATA%\Temp is denied to grandchildren (cargo -> test binary),
// which makes every test that calls std::env::temp_dir() fail with os error 5, and
// makes link.exe fail with LNK1104. Keeping temp inside the workspace fixes both and
// also keeps the tests hermetic.
const verifyTmp = path.join(outDir, "tmp");
fs.mkdirSync(verifyTmp, { recursive: true });
env.TMP = verifyTmp;
env.TEMP = verifyTmp;

const results = [];

/** Run a step, capturing output to a file (pipes are unavailable in this sandbox). */
function step(name, cmd, args, cwd = repoRoot, stepEnv = env) {
  const logFile = path.join(outDir, name.replace(/[^a-z0-9]+/gi, "-") + ".log");
  const fd = fs.openSync(logFile, "w");
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cmd, args, { cwd, env: stepEnv, stdio: ["ignore", fd, fd], windowsHide: true });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  fs.closeSync(fd);
  const log = fs.readFileSync(logFile, "utf8");
  if (r.error) {
    fs.appendFileSync(logFile, "\n[spawn error] " + r.error.message + "\n");
  }
  if (r.signal) {
    fs.appendFileSync(logFile, "\n[terminated by signal] " + r.signal + "\n");
  }
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

  // 2. Desktop app (MSVC only; the rest of the workspace builds with GNU here).
  //    It is a separate cargo project because one target directory cannot hold both
  //    toolchains, so it needs its own build invocation.
  //    The wrapper establishes the MSVC environment itself, so this step does not
  //    depend on the caller having sourced env-msvc.ps1.
  // The Tauri build embeds the UI at compile time, so the UI has to be built
  // first. pnpm is avoided here on purpose: it stalls resolving the workspace on
  // this host, and every dependency the UI needs is already hoisted.
  const desktopUi = path.join(repoRoot, "apps", "desktop");
  const viteBin = path.join(repoRoot, "node_modules", "vite", "bin", "vite.js");
  if (fs.existsSync(viteBin) && fs.existsSync(desktopUi)) {
    step("desktop-ui-build", process.execPath, [viteBin, "build"], desktopUi);
  } else {
    results.push({
      name: "desktop-ui-build",
      ok: null,
      skipped: "vite not installed (run: pnpm install --ignore-scripts)",
    });
    console.log("SKIP  desktop-ui-build    (vite not installed)");
  }

  const desktopWrapper = path.join(repoRoot, "scripts", "dev", "build-desktop.bat");
  const vcvarsall = "C:\\Program Files (x86)\\Microsoft Visual Studio\\18\\BuildTools\\VC\\Auxiliary\\Build\\vcvarsall.bat";
  if (fs.existsSync(desktopWrapper) && fs.existsSync(vcvarsall)) {
    // Absolute path: this script rebuilds PATH for the Rust steps, and a bare
    // "cmd.exe" lookup fails once that PATH no longer contains System32 first.
    const comspec = process.env.ComSpec || "C:\\Windows\\System32\\cmd.exe";
    // process.env, not the curated Rust env: the wrapper brings its own MSVC setup
    // and the GNU-oriented PATH only interferes with it.
    step("desktop-build", comspec, ["/c", desktopWrapper, "build"], repoRoot, { ...process.env });
  } else {
    results.push({
      name: "desktop-build",
      ok: null,
      skipped: "MSVC Build Tools not found; the desktop app cannot be built on this machine",
    });
    console.log("SKIP  desktop-build        (MSVC Build Tools not found)");
  }

  // 3. Extension (optional: needs node_modules)
  const extDir = path.join(repoRoot, "apps", "extension");
  const wxt = path.join(repoRoot, "node_modules", "wxt", "bin", "wxt.mjs");
  if (fs.existsSync(wxt)) {
    step("extension-build", process.execPath, [wxt, "build"], extDir);

  // Fast unit tests for the browser-side rules. They run before the build because
  // a broken rule should fail in seconds, not after a bundle is produced.
  const extTests = path.join(repoRoot, "apps", "extension", "src");
  if (fs.existsSync(extTests)) {
    step("extension-test", process.execPath, ["--test", "src/**/*.test.ts"], path.join(repoRoot, "apps", "extension"));
  }  } else {
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

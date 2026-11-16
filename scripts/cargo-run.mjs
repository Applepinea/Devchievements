#!/usr/bin/env node
/**
 * scripts/cargo-run.mjs
 *
 * Runs cargo with the isolated toolchain environment, capturing all output to a
 * file and printing it.
 *
 * WHY THIS EXISTS
 * ---------------
 * Two host quirks make "just run cargo" unreliable here:
 *   1. cargo lives in an isolated CARGO_HOME that is not on any persistent PATH;
 *   2. PowerShell 5.1 on this host mangles cargo's stderr, so build errors and
 *      test results can come back empty even when the command failed.
 * Spawning through Node with explicit stdio wiring avoids both, and mirrors what
 * scripts/env.ps1 sets up for interactive use.
 *
 * USAGE
 * -----
 *   node scripts/cargo-run.mjs build -p devchievements-storage
 *   node scripts/cargo-run.mjs test --workspace
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");

const cargoHome = path.join(toolchain, "cargo");
const cargo = path.join(cargoHome, "bin", "cargo.exe");
const mingwBin = path.join(toolchain, "_mingw-gcc", "mingw64", "bin");
const selfContained = path.join(
  toolchain,
  "rustup",
  "toolchains",
  "stable-x86_64-pc-windows-gnu",
  "lib",
  "rustlib",
  "x86_64-pc-windows-gnu",
  "bin",
  "self-contained"
);

const env = { ...process.env };
env.RUSTUP_HOME = path.join(toolchain, "rustup");
env.CARGO_HOME = cargoHome;
const pathParts = [mingwBin, selfContained, path.join(cargoHome, "bin"), env.PATH ?? ""];
env.PATH = pathParts.join(";");
if (fs.existsSync(path.join(mingwBin, "gcc.exe"))) {
  env.CC = path.join(mingwBin, "gcc.exe");
}

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("usage: node scripts/cargo-run.mjs <cargo args...>");
  process.exit(2);
}
if (!fs.existsSync(cargo)) {
  console.error("cargo not found at " + cargo + " -- run scripts/install-rust.ps1");
  process.exit(2);
}

const outFile = path.join(workspace, "_toolprobe", "cargo-last.log");
fs.mkdirSync(path.dirname(outFile), { recursive: true });
const fd = fs.openSync(outFile, "w");

const res = spawnSync(cargo, args, {
  cwd: repoRoot,
  env,
  stdio: ["ignore", fd, fd],
  windowsHide: true,
});
fs.closeSync(fd);

const log = fs.readFileSync(outFile, "utf8");
// Echo everything except cargo's "Updating index" chatter.
const lines = log.split(/\r?\n/).filter((l) => !/^\s*(Updating|Blocking|Locking|Downloaded|Downloading)/.test(l));
process.stdout.write(lines.join("\n"));

console.log("");
console.log("-----");
console.log("cargo " + args.join(" ") + " -> exit " + res.status);
console.log("full log: " + outFile);
process.exit(res.status ?? 1);

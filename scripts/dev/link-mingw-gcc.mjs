#!/usr/bin/env node
/**
 * scripts/dev/link-mingw-gcc.mjs
 *
 * Adds the \`gcc.exe\` name that C build scripts (cc-rs, used by libsqlite3-sys)
 * look for, pointing at the mingw-w64 GCC that already ships with the isolated
 * GNU Rust toolchain.
 *
 * WHY: libsqlite3-sys bundles SQLite's C sources and compiles them with cc-rs,
 * which searches PATH for literally "gcc.exe". The toolchain only provides
 * "x86_64-w64-mingw32-gcc.exe", so the build failed with
 *   failed to find tool "gcc.exe"
 * We do NOT install a compiler system-wide; we add one name inside the isolated
 * toolchain directory (ADR-0012) by copying the existing binary next to itself.
 *
 * SCOPE: writes only inside <workspace>/_toolchain.
 */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");

const selfContained = path.join(
  toolchain, "rustup", "toolchains", "stable-x86_64-pc-windows-gnu",
  "lib", "rustlib", "x86_64-pc-windows-gnu", "bin", "self-contained"
);

const source = path.join(selfContained, "x86_64-w64-mingw32-gcc.exe");
const alias = path.join(selfContained, "gcc.exe");

if (!fs.existsSync(source)) {
  console.error("mingw gcc not found: " + source);
  process.exitCode = 1;
} else {
  if (!fs.existsSync(alias)) {
    fs.copyFileSync(source, alias);
    console.log("created " + alias);
  } else {
    console.log("already present: " + alias);
  }
  const v = spawnSync(alias, ["--version"], { stdio: "ignore", windowsHide: true });
  console.log("gcc.exe runs: " + (v.status === 0));
  if (v.status !== 0) process.exitCode = 1;
}

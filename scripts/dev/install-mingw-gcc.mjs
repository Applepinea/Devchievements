#!/usr/bin/env node
/**
 * scripts/dev/install-mingw-gcc.mjs
 *
 * Installs a WORKING mingw-w64 C compiler into the isolated toolchain
 * (<workspace>/_toolchain), so C-dependency build scripts can run.
 *
 * WHY THIS IS NEEDED
 * ------------------
 * libsqlite3-sys (pulled in by SQLx) compiles SQLite's C sources through cc-rs.
 * The isolated GNU Rust toolchain ships only a gcc *driver* with no back end:
 *
 *     gcc.exe: fatal error: cannot execute 'cc1': CreateProcess: No such file
 *
 * Without a real C compiler the entire persistence layer of Phase 1 cannot be
 * built or tested, which would force every "database round-trip" claim to stay
 * unverified.
 *
 * WHAT IT DOES
 * ------------
 * 1. GCClings: the mingw-w64 gcc 14.2.0-3 package (compiler + cc1),
 * 2. the matching gcc-libs 14.2.0-3 package (libgcc_s_seh-1, libwinpthread-1,
 *    libssp-0, ...), which gcc.exe needs at load time,
 * 3. extracts both into <workspace>/_toolchain/_mingw-gcc.
 *
 * Windows reports a missing DLL only as exit code 0xC0000135 with no message,
 * so after extraction this walks the import closure and reports anything missing
 * instead of failing silently.
 *
 * SCOPE / SAFETY
 * --------------
 * Everything stays under <workspace>/_toolchain (ADR-0012). No system paths, no
 * registry, no change to the user's persistent PATH.
 */

import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { nonSystemImports } from "./pe-lib.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");
const dest = path.join(toolchain, "_mingw-gcc");
const baseBin = path.join(dest, "mingw64", "bin");
fs.mkdirSync(dest, { recursive: true });

const BASE = "https://repo.msys2.org/mingw/mingw64/";
const PACKAGES = [
  "mingw-w64-x86_64-gcc-14.2.0-3-any.pkg.tar.zst",
  "mingw-w64-x86_64-gcc-libs-14.2.0-3-any.pkg.tar.zst",
];

function head(url) {
  return new Promise((resolve) => {
    const req = https.request(url, { method: "HEAD", timeout: 20000 }, (res) => {
      resolve(res.statusCode);
      res.resume();
    });
    req.on("timeout", () => { req.destroy(); resolve("timeout"); });
    req.on("error", () => resolve("err"));
    req.end();
  });
}

function download(url, file) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { timeout: 300000, headers: { "user-agent": "devchievements-dev-env" } }, (res) => {
      if (res.statusCode !== 200) { res.resume(); return reject(new Error("HTTP " + res.statusCode)); }
      const f = fs.createWriteStream(file);
      res.pipe(f);
      f.on("finish", () => f.close(() => resolve(fs.statSync(file).size)));
      res.on("error", reject);
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
  });
}

async function ensurePackage(pkg) {
  const archive = path.join(dest, pkg);
  if (!fs.existsSync(archive)) {
    const st = await head(BASE + pkg);
    if (st !== 200) throw new Error("package unavailable (" + st + "): " + pkg);
    const size = await download(BASE + pkg, archive);
    console.log("downloaded " + pkg + " (" + size + " bytes)");
  }
  const r = spawnSync("tar.exe", ["-xf", archive, "-C", dest], { stdio: "ignore", windowsHide: true });
  if (r.status !== 0) throw new Error("extract failed: " + pkg);
  console.log("extracted " + pkg);
}

/** Report DLLs the compiler needs that are not resolvable next to it. */
function reportMissingDlls(startFiles) {
  const missing = [];
  const seen = new Set();
  const queue = [...startFiles];
  while (queue.length) {
    const f = queue.shift();
    if (seen.has(f)) continue;
    seen.add(f);
    for (const dll of nonSystemImports(f)) {
      const local = path.join(baseBin, dll);
      if (fs.existsSync(local)) {
        if (!seen.has(local)) queue.push(local);
      } else {
        missing.push(dll + " (needed by " + path.basename(f) + ")");
      }
    }
  }
  return missing;
}

async function main() {
  for (const pkg of PACKAGES) await ensurePackage(pkg);

  const gcc = path.join(baseBin, "gcc.exe");
  const cc1 = path.join(dest, "mingw64", "lib", "gcc", "x86_64-w64-mingw32", "14.2.0", "cc1.exe");
  console.log("gcc.exe present: " + fs.existsSync(gcc));
  console.log("cc1.exe present: " + fs.existsSync(cc1));

  const missing = reportMissingDlls([gcc]);
  if (missing.length) {
    console.error("STILL MISSING DLLs:");
    for (const m of missing) console.error("  " + m);
    process.exitCode = 1;
  }

  const env = { ...process.env, PATH: baseBin + ";" + process.env.PATH };
  const v = spawnSync(gcc, ["--version"], { stdio: "ignore", windowsHide: true, env });
  console.log("gcc runs: " + (v.status === 0) + " (exit " + v.status + ")");
  if (v.status !== 0) process.exitCode = 1;

  console.log("PATH entry to use: " + baseBin);
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

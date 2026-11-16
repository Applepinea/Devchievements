#!/usr/bin/env node
/**
 * scripts/dev/install-mingw-binutils.mjs
 *
 * WHY THIS EXISTS (host workaround, NOT part of the product)
 * ---------------------------------------------------------
 * The isolated GNU Rust toolchain on this machine ships rust-mingw's
 * self-contained ||dlltool.exe|| but NOT ||as.exe||. Any crate needing a Windows
 * import library (windows-sys, parking_lot_core, therefore sqlx and gix) fails:
 *
 *     dlltool.exe: CreateProcess
 *     x86_64-w64-mingw32-gcc.exe: fatal error: cannot execute 'as': No such file
 *
 * We fetch the matching MSYS2 packages and extract ONLY the binaries (plus the
 * DLLs they require) into the toolchain's self-contained directory, completing the
 * GNU linker pipeline.
 *
 * SCOPE / SAFETY
 * --------------
 * - writes only inside <workspace>/_toolchain, the isolated disposable toolchain
 *   directory (ADR-0012); nothing touches system paths or user PATH;
 * - downloads over Node's https, because this host's schannel TLS is broken;
 * - package versions pinned so the assembler matches the bundled dlltool (2.44);
 * - only the specific members listed below are extracted, not whole packages.
 */

import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const toolchain = path.join(workspace, "_toolchain");

// Packages whose binaries as.exe needs at runtime, determined by reading its
// PE import table (scripts/dev/pe-imports.mjs): libintl-8, zlib1, libzstd.
const PACKAGES = [
  { name: "mingw-w64-x86_64-binutils-2.44-1-any.pkg.tar.zst", members: ["mingw64/bin/as.exe"] },
  // Version confirmed present on the mirror via scripts/dev/search-msys2.mjs.
  { name: "mingw-w64-x86_64-gettext-runtime-1.0-1-any.pkg.tar.zst", members: ["mingw64/bin/libintl-8.dll"] },
  { name: "mingw-w64-x86_64-zlib-1.3.1-1-any.pkg.tar.zst", members: ["mingw64/bin/zlib1.dll"] },
  { name: "mingw-w64-x86_64-zstd-1.5.7-1-any.pkg.tar.zst", members: ["mingw64/bin/libzstd.dll"] },
];

const BASE = "https://repo.msys2.org/mingw/mingw64/";
const work = path.join(toolchain, "_binutils");
fs.mkdirSync(work, { recursive: true });

function head(url) {
  return new Promise((resolve, reject) => {
    const req = https.request(url, { method: "HEAD", timeout: 20000 }, (res) => {
      resolve({ status: res.statusCode, len: res.headers["content-length"] });
      res.resume();
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
    req.end();
  });
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { timeout: 120000, headers: { "user-agent": "devchievements-dev-env" } }, (res) => {
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error("HTTP " + res.statusCode + " for " + url));
      }
      const f = fs.createWriteStream(dest);
      res.pipe(f);
      f.on("finish", () => f.close(() => resolve(fs.statSync(dest).size)));
      res.on("error", reject);
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
  });
}

function extract(archive, members) {
  const r = spawnSync("tar.exe", ["-x", "-f", archive, "-C", work, ...members], {
    stdio: "ignore",
    windowsHide: true,
  });
  return r.status === 0;
}

async function main() {
  const selfContained = path.join(
    toolchain, "rustup", "toolchains", "stable-x86_64-pc-windows-gnu",
    "lib", "rustlib", "x86_64-pc-windows-gnu", "bin", "self-contained"
  );
  if (!fs.existsSync(selfContained)) {
    console.error("toolchain self-contained dir not found: " + selfContained);
    process.exitCode = 1;
    return;
  }

  for (const pkg of PACKAGES) {
    const archive = path.join(work, pkg.name);
    if (!fs.existsSync(archive)) {
      const h = await head(BASE + pkg.name).catch((e) => ({ status: "err:" + e.message }));
      if (h.status !== 200) {
        console.error("MISSING " + pkg.name + " (status " + h.status + ")");
        process.exitCode = 1;
        continue;
      }
      const size = await download(BASE + pkg.name, archive);
      console.log("downloaded " + pkg.name + " (" + size + " bytes)");
    }
    if (!extract(archive, pkg.members)) {
      console.error("extract failed: " + pkg.name);
      process.exitCode = 1;
      continue;
    }
    for (const m of pkg.members) {
      const from = path.join(work, m);
      const to = path.join(selfContained, path.basename(m));
      if (!fs.existsSync(from)) {
        console.error("member missing after extract: " + m);
        process.exitCode = 1;
        continue;
      }
      fs.copyFileSync(from, to);
      console.log("installed " + path.basename(m) + " -> " + selfContained);
    }
  }

  const asExe = path.join(selfContained, "as.exe");
  const v = spawnSync(asExe, ["--version"], { stdio: "ignore", windowsHide: true });
  console.log("as.exe runs: " + (v.status === 0) + " (exit " + v.status + ")");
  if (v.status !== 0) process.exitCode = 1;
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

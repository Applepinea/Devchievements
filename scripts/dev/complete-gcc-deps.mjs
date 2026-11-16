#!/usr/bin/env node
/**
 * scripts/dev/complete-gcc-deps.mjs
 *
 * Fetches the runtime DLL closure a freshly extracted mingw-w64 GCC needs.
 *
 * WHY: extracting the gcc package alone is not enough. cc1.exe links against
 * libgmp / libmpfr / libmpc / libisl, and Windows reports each missing DLL only
 * as exit code 0xC0000135 with no message. Adding them one guess at a time cost
 * several round trips, so this walks the import closure and installs providers.
 *
 * SCOPE: writes only inside <workspace>/_toolchain (ADR-0012); never touches the
 * system, the registry, or the user's persistent PATH.
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
const dest = path.join(workspace, "_toolchain", "_mingw-gcc");
const bin = path.join(dest, "mingw64", "bin");
const BASE = "https://repo.msys2.org/mingw/mingw64/";

/** Package that provides each non-system DLL in GCC's closure. */
const PROVIDER = {
  "libgmp-10.dll": "mingw-w64-x86_64-gmp-6.3.0-2-any.pkg.tar.zst",
  "libmpfr-6.dll": "mingw-w64-x86_64-mpfr-4.2.1-2-any.pkg.tar.zst",
  "libmpc-3.dll": "mingw-w64-x86_64-mpc-1.3.1-2-any.pkg.tar.zst",
  "libisl-23.dll": "mingw-w64-x86_64-isl-0.27-1-any.pkg.tar.zst",
  "zlib1.dll": "mingw-w64-x86_64-zlib-1.3.1-1-any.pkg.tar.zst",
  "libzstd.dll": "mingw-w64-x86_64-zstd-1.5.7-1-any.pkg.tar.zst",
  "libwinpthread-1.dll": "mingw-w64-x86_64-libwinpthread-git-13.0.0.r309.g2b4bfbe9e-1-any.pkg.tar.zst",
};

// Fallbacks, because package versions on a rolling mirror change.
const FALLBACK = {
  "libgmp-10.dll": ["mingw-w64-x86_64-gmp-6.3.0-1-any.pkg.tar.zst"],
  "libmpfr-6.dll": ["mingw-w64-x86_64-mpfr-4.2.1-1-any.pkg.tar.zst"],
  "libmpc-3.dll": ["mingw-w64-x86_64-mpc-1.3.1-1-any.pkg.tar.zst"],
  "libisl-23.dll": ["mingw-w64-x86_64-isl-0.28-1-any.pkg.tar.zst"],
};

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

function listMembers(archive) {
  const listing = path.join(dest, "_listing.txt");
  fs.writeFileSync(listing, "");
  const fd = fs.openSync(listing, "w");
  const r = spawnSync("tar.exe", ["-tf", archive], { stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  if (r.status !== 0) return [];
  return fs.readFileSync(listing, "utf8").split(/\r?\n/).filter(Boolean);
}

async function fetchDll(dll) {
  const local = path.join(bin, dll);
  if (fs.existsSync(local)) return true;

  const candidates = [PROVIDER[dll], ...(FALLBACK[dll] ?? [])].filter(Boolean);
  if (candidates.length === 0) {
    console.error("no known provider for " + dll);
    return false;
  }

  for (const pkg of candidates) {
    const archive = path.join(dest, pkg);
    if (!fs.existsSync(archive)) {
      const st = await head(BASE + pkg);
      if (st !== 200) continue;
      const size = await download(BASE + pkg, archive);
      console.log("downloaded " + pkg + " (" + size + " bytes)");
    }
    const member = listMembers(archive).find((m) => m.split(/[\\/]/).pop() === dll);
    if (!member) continue;

    const r = spawnSync("tar.exe", ["-xf", archive, "-C", dest, member], { stdio: "ignore", windowsHide: true });
    if (r.status !== 0) continue;
    const from = path.join(dest, member);
    if (!fs.existsSync(from)) continue;
    fs.copyFileSync(from, local);
    console.log("installed " + dll);
    return true;
  }
  console.error("could not install " + dll);
  return false;
}

async function main() {
  const cc1 = path.join(dest, "mingw64", "lib", "gcc", "x86_64-w64-mingw32", "14.2.0", "cc1.exe");
  const roots = [path.join(bin, "gcc.exe"), cc1].filter((f) => fs.existsSync(f));

  let allOk = true;
  const seen = new Set();
  const queue = [...roots];
  while (queue.length) {
    const f = queue.shift();
    if (seen.has(f)) continue;
    seen.add(f);
    for (const dll of nonSystemImports(f)) {
      const local = path.join(bin, dll);
      if (fs.existsSync(local)) {
        if (!seen.has(local)) queue.push(local);
        continue;
      }
      const ok = await fetchDll(dll);
      if (!ok) { allOk = false; continue; }
      if (!seen.has(local)) queue.push(local);
    }
  }

  const env = { ...process.env, PATH: bin + ";" + process.env.PATH };
  for (const f of roots) {
    const v = spawnSync(f, f === cc1 ? ["--version"] : ["--version"], { stdio: "ignore", windowsHide: true, env });
    console.log(path.basename(f) + " runs: " + (v.status === 0) + " (exit " + v.status + ")");
    if (v.status !== 0) allOk = false;
  }
  if (!allOk) process.exitCode = 1;
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

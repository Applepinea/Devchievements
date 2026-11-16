#!/usr/bin/env node
/**
 * scripts/dev/complete-mingw-sysroot.mjs
 *
 * Adds the mingw-w64 C runtime that GCC needs to actually compile and link:
 * headers (stdarg.h, stdio.h, ...), CRT objects (dllcrt2.o, crt2.o) and import
 * libraries (libkernel32.a, ...).
 *
 * WHY: a GCC package on its own cannot build even a trivial program. Without the
 * sysroot the errors are:
 *   fatal error: stdarg.h: No such file or directory
 *   ld.exe: cannot find dllcrt2.o
 *   ld.exe: cannot find -lkernel32
 * This blocks libsqlite3-sys (SQLite's C sources), which blocks SQLx, which
 * blocks every "real database" verification in Phase 1.
 *
 * SCOPE: extracts only into <workspace>/_toolchain (ADR-0012).
 */

import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const dest = path.join(workspace, "_toolchain", "_mingw-gcc");
const BASE = "https://repo.msys2.org/mingw/mingw64/";

// crt provides crt2.o/dllcrt2.o/scrt1.o; headers provides the C/C++ system
// headers; winpthreads provides the posix thread headers GCC's thread model needs.
// Exact names verified present on the mirror with scripts/dev/search-msys2.mjs.
const PACKAGES = [
  "mingw-w64-x86_64-crt-git-13.0.0.r380.gb83511db8-1-any.pkg.tar.zst",
  "mingw-w64-x86_64-headers-git-13.0.0.r380.gb83511db8-1-any.pkg.tar.zst",
  // pthread_time.h / libpthread.a: GCC here is configured --enable-threads=posix.
  "mingw-w64-x86_64-winpthreads-git-12.0.0.r747.g1a99f8514-1-any.pkg.tar.zst",
  // default-manifest.o: GCC adds -l:default-manifest.o when linking a program on
  // Windows; without it the link stops with "cannot find default-manifest.o".
  "mingw-w64-x86_64-windows-default-manifest-6.4-4-any.pkg.tar.zst",
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

async function ensure(pkg) {
  const archive = path.join(dest, pkg);
  if (!fs.existsSync(archive)) {
    const st = await head(BASE + pkg);
    if (st !== 200) {
      console.error("unavailable (" + st + "): " + pkg);
      return false;
    }
    const size = await download(BASE + pkg, archive);
    console.log("downloaded " + pkg + " (" + size + " bytes)");
  } else {
    console.log("cached " + pkg);
  }
  const r = spawnSync("tar.exe", ["-xf", archive, "-C", dest], { stdio: "ignore", windowsHide: true });
  console.log("extracted " + pkg + ": " + (r.status === 0));
  return r.status === 0;
}

async function main() {
  let ok = true;
  for (const pkg of PACKAGES) {
    if (!(await ensure(pkg))) ok = false;
  }
  const sysroot = path.join(dest, "mingw64", "x86_64-w64-mingw32");
  console.log("sysroot exists: " + fs.existsSync(sysroot));
  const crt = path.join(sysroot, "lib", "dllcrt2.o");
  const k32 = path.join(sysroot, "lib", "libkernel32.a");
  const stdarg = path.join(dest, "mingw64", "lib", "gcc", "x86_64-w64-mingw32", "14.2.0", "include", "stdarg.h");
  console.log("dllcrt2.o: " + fs.existsSync(crt));
  console.log("libkernel32.a: " + fs.existsSync(k32));
  console.log("gcc stdarg.h: " + fs.existsSync(stdarg));
  if (!ok || !fs.existsSync(crt) || !fs.existsSync(k32)) process.exitCode = 1;
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

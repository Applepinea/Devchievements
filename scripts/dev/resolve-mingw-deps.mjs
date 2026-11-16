#!/usr/bin/env node
/**
 * scripts/dev/resolve-mingw-deps.mjs
 *
 * Completes the GNU toolchain's assembler by fetching the DLL closure of as.exe.
 *
 * WHY: windows-sys / parking_lot_core / sqlx / gix all need a Windows import
 * library, which needs dlltool, which needs as.exe. The isolated toolchain
 * (ADR-0012) ships dlltool but not the assembler. Windows reports a missing
 * dependency only as exit code 0xC0000135 with no message, so this walks each
 * PE's import table and fetches providers until as.exe starts.
 *
 * SAFETY: writes only inside <workspace>/_toolchain (see ADR-0012).
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
const work = path.join(toolchain, "_binutils");
fs.mkdirSync(work, { recursive: true });

const BASE = "https://repo.msys2.org/mingw/mingw64/";

/** Which MSYS2 package provides each DLL the closure needs. */
const PROVIDER = {
  "libintl-8.dll": "mingw-w64-x86_64-gettext-runtime-1.0-1-any.pkg.tar.zst",
  "libiconv-2.dll": "mingw-w64-x86_64-libiconv-1.18-1-any.pkg.tar.zst",
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

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { timeout: 120000, headers: { "user-agent": "devchievements-dev-env" } }, (res) => {
      if (res.statusCode !== 200) { res.resume(); return reject(new Error("HTTP " + res.statusCode)); }
      const f = fs.createWriteStream(dest);
      res.pipe(f);
      f.on("finish", () => f.close(() => resolve(fs.statSync(dest).size)));
      res.on("error", reject);
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
  });
}

// ARCHIVE LISTING WITHOUT PIPES.
//
// spawnSync({encoding:"utf8"}) captures the child's stdout through a pipe, and
// this sandbox denies pipe creation to child processes, so the capture silently
// came back empty. Redirecting to a file works everywhere and is what the rest of
// this repo does for the same reason.
function listMembers(archive) {
  const listing = path.join(work, "_listing.txt");
  fs.writeFileSync(listing, "");
  const fd = fs.openSync(listing, "w");
  const r = spawnSync("tar.exe", ["-tf", archive], { stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  if (r.status !== 0) return [];
  return fs.readFileSync(listing, "utf8").split(/\r?\n/).filter(Boolean);
}

async function main() {
  const selfContained = path.join(
    toolchain, "rustup", "toolchains", "stable-x86_64-pc-windows-gnu",
    "lib", "rustlib", "x86_64-pc-windows-gnu", "bin", "self-contained"
  );

  const asExe = path.join(selfContained, "as.exe");
  const queue = [asExe];
  const seen = new Set([asExe]);

  while (queue.length) {
    const file = queue.shift();
    for (const dll of nonSystemImports(file)) {
      const local = path.join(selfContained, dll);
      if (fs.existsSync(local)) {
        if (!seen.has(local)) { seen.add(local); queue.push(local); }
        continue;
      }
      const pkg = PROVIDER[dll];
      if (!pkg) { console.error("NO PROVIDER for " + dll); continue; }

      const archive = path.join(work, pkg);
      if (!fs.existsSync(archive)) {
        const st = await head(BASE + pkg);
        if (st !== 200) { console.error("cannot fetch " + pkg + " (" + st + ")"); continue; }
        await download(BASE + pkg, archive);
        console.log("downloaded " + pkg);
      }
      // Match on the basename so it works regardless of the separator the
      // platform tar happens to print.
      const member = listMembers(archive).find(
        (m) => m.split(/[\\/]/).pop() === dll
      );
      if (!member) { console.error(pkg + " contains no " + dll); continue; }

      const r = spawnSync("tar.exe", ["-x", "-f", archive, "-C", work, member], { stdio: "ignore", windowsHide: true });
      if (r.status !== 0) { console.error("extract failed: " + member); continue; }
      fs.copyFileSync(path.join(work, member), local);
      console.log("installed " + dll);
      seen.add(local);
      queue.push(local);
    }
  }

  const v = spawnSync(asExe, ["--version"], { stdio: "ignore", windowsHide: true });
  console.log("as.exe runs: " + (v.status === 0) + " (exit " + v.status + ")");
  if (v.status !== 0) process.exitCode = 1;
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

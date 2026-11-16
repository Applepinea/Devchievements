#!/usr/bin/env node
/**
 * scripts/dev/complete-mingw-binutils.mjs
 *
 * Downloads and extracts the mingw binutils tools (ar, ranlib, nm, objcopy, ...)
 * into the isolated mingw toolchain directory.
 *
 * WHY: cc-rs archives C objects with \`ar\`. The isolated Rust toolchain ships only
 * the linker-side pieces (ld, dlltool, as), so C builds (libsqlite3-sys -> SQLx)
 * stopped at:
 *   error occurred in cc-rs: failed to find tool "ar"
 *
 * SCOPE: writes only inside <workspace>/_toolchain (ADR-0012).
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
const bin = path.join(dest, "mingw64", "bin");
const BASE = "https://repo.msys2.org/mingw/mingw64/";
const PKG = "mingw-w64-x86_64-binutils-2.44-1-any.pkg.tar.zst";
const archive = path.join(dest, PKG);

const TOOLS = ["ar.exe", "ranlib.exe", "nm.exe", "objcopy.exe", "objdump.exe", "strip.exe", "as.exe", "ld.exe"];

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

function list(archivePath, outFile) {
  fs.writeFileSync(outFile, "");
  const fd = fs.openSync(outFile, "w");
  const r = spawnSync("tar.exe", ["-tf", archivePath], { stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  if (r.status !== 0) return [];
  return fs.readFileSync(outFile, "utf8").split(/\r?\n/).filter(Boolean);
}

async function main() {
  if (!fs.existsSync(archive)) {
    const size = await download(BASE + PKG, archive);
    console.log("downloaded " + PKG + " (" + size + " bytes)");
  }

  const members = list(archive, path.join(dest, "_binutils-listing.txt")).filter(
    (m) => m.startsWith("mingw64/bin/") && m.endsWith(".exe")
  );
  console.log("members to extract: " + members.length);
  if (members.length === 0) {
    console.error("archive listing empty");
    process.exitCode = 1;
    return;
  }

  const r = spawnSync("tar.exe", ["-xf", archive, "-C", dest, ...members], { stdio: "ignore", windowsHide: true });
  console.log("extract ok: " + (r.status === 0));

  const env = { ...process.env, PATH: bin + ";" + process.env.PATH };
  let allOk = true;
  for (const t of TOOLS) {
    const p = path.join(bin, t);
    if (!fs.existsSync(p)) { console.log(t + ": MISSING"); allOk = false; continue; }
    const v = spawnSync(p, ["--version"], { stdio: "ignore", windowsHide: true, env });
    console.log(t + " runs: " + (v.status === 0));
    if (v.status !== 0) allOk = false;
  }
  if (!allOk) process.exitCode = 1;
}

main().catch((e) => { console.error(e); process.exitCode = 1; });

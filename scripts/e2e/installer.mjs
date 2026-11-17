#!/usr/bin/env node
/**
 * scripts/e2e/installer.mjs
 *
 * Spec 40.1: the installer, the upgrade path and the uninstaller are part of the
 * product, so they get verified on a real build instead of being assumed.
 *
 * What this proves:
 *   1. the NSIS installer runs unattended and lays down both binaries
 *      (the desktop app and the helper it needs for git hooks),
 *   2. the installed binary actually starts and reports itself (--diagnostics),
 *   3. the uninstaller removes the program, and
 *   4. it leaves user data alone -- the database created by step 2 survives.
 *
 * It installs into a scratch directory via the NSIS /D flag (which must be the
 * last, unquoted argument) and passes /NS so no desktop or start menu shortcut
 * is created on the machine running the test.
 *
 * Exit codes: 0 pass, 1 fail, 2 environment prevented verification.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const bundleDir = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "release", "bundle", "nsis");
const scratch = path.join(__dirname, ".installer");
const installDir = path.join(scratch, "app");
const dataRoot = path.join(scratch, "appdata");

let failures = 0;
let checks = 0;
function check(label, ok, detail) {
  checks++;
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function findInstaller() {
  if (!fs.existsSync(bundleDir)) return null;
  const hit = fs.readdirSync(bundleDir).filter((n) => n.endsWith("-setup.exe"));
  if (hit.length === 0) return null;
  return path.join(bundleDir, hit[0]);
}

// One log per invocation: the same executable is called with different flags, and
// appending both outputs to one file makes the JSON unreadable.
function run(exe, args, options) {
  const { logName, ...spawnOptions } = options || {};
  const log = path.join(scratch, logName || path.basename(exe) + ".log");
  const fd = fs.openSync(log, "w");
  const result = spawnSync(exe, args, {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    timeout: 240000,
    ...spawnOptions,
  });
  fs.closeSync(fd);
  return result;
}

function waitFor(predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    // The installer and uninstaller both finish on their own; this only waits
    // for the file system to catch up with a process that already exited.
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200);
  }
  return predicate();
}

// The pubkey in tauri.conf.json is base64 of the key file: an untrusted-comment
// line, then base64(signature algorithm + 8-byte key id + 32-byte key).
function readPublicKeyId() {
  const configPath = path.join(repoRoot, "apps", "desktop", "src-tauri", "tauri.conf.json");
  const raw = fs.readFileSync(configPath, "utf8");
  const match = /"pubkey"\s*:\s*"([^"]+)"/.exec(raw);
  if (!match) return null;
  const file = Buffer.from(match[1], "base64").toString("utf8").split(/\r?\n/).filter(Boolean);
  if (file.length < 2) return null;
  const blob = Buffer.from(file[1], "base64");
  return blob.subarray(2, 10).toString("hex");
}

// The .sig Tauri writes is the whole minisign signature file, base64-encoded as one
// line. Inside: untrusted comment, base64 blob, trusted comment, base64 blob. The
// first blob starts with the algorithm and the 8-byte key id.
function readSignatureKeyId(sigPath) {
  const raw = fs.readFileSync(sigPath, "utf8").trim();
  const decoded = Buffer.from(raw, "base64").toString("utf8");
  const inner = decoded.startsWith("untrusted comment") ? decoded : raw;
  const lines = inner.split(/\r?\n/).filter(Boolean);
  if (lines.length < 2) return null;
  const blob = Buffer.from(lines[1], "base64");
  if (blob.length < 10) return null;
  return blob.subarray(2, 10).toString("hex");
}

function main() {
  const installer = findInstaller();
  if (!installer) {
    console.log("ENVIRONMENT: no NSIS installer under " + bundleDir);
    console.log("Run scripts/dev/build-installer.bat first.");
    process.exit(2);
  }
  console.log("installer: " + path.basename(installer));

  // Spec 40.3: an installer nobody can update from is a dead end. The bundler
  // signs the artifact with the updater key, and that signature is what an
  // update feed serves next to it.
  //
  // A .sig file existing is not the same as it belonging to the key this build
  // trusts: a signature from a different keypair looks identical until the user
  // tries to update. Both files carry the same 8-byte key id, so that is what is
  // compared here.
  const sigPath = installer + ".sig";
  check("the installer is signed for the updater", fs.existsSync(sigPath), sigPath);
  const keyIdFromConfig = readPublicKeyId();
  const keyIdFromSignature = fs.existsSync(sigPath) ? readSignatureKeyId(sigPath) : null;
  check(
    "the signature belongs to the configured update key",
    !!keyIdFromConfig && !!keyIdFromSignature && keyIdFromConfig === keyIdFromSignature,
    "config=" + keyIdFromConfig + " signature=" + keyIdFromSignature,
  );

  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });
  fs.mkdirSync(dataRoot, { recursive: true });

  // Silent, no shortcuts, into the scratch directory.
  const install = run(installer, ["/S", "/NS", "/D=" + installDir]);
  const mainExe = path.join(installDir, "devchievements-desktop.exe");
  const helperExe = path.join(installDir, "devch-helper.exe");
  const uninstaller = path.join(installDir, "uninstall.exe");
  const installed = waitFor(
    () => fs.existsSync(mainExe) && fs.existsSync(helperExe),
    120000,
  );
  check("the installer succeeds", install.status === 0, "exit=" + install.status);
  check("it installs the desktop app", fs.existsSync(mainExe));
  check("it installs the helper the hooks call", fs.existsSync(helperExe));
  check("it installs an uninstaller", fs.existsSync(uninstaller));
  if (!installed) {
    report();
    return;
  }

  // The installed binary is a different file than the one in target/release: it
  // has been patched for the bundle type, so it is worth starting on its own.
  const diagnosticsLog = path.join(scratch, "diagnostics.json");
  const diagnostics = run(mainExe, ["--diagnostics"], {
    logName: "diagnostics.json",
    env: { ...process.env, LOCALAPPDATA: dataRoot },
  });
  const raw = fs.existsSync(diagnosticsLog) ? fs.readFileSync(diagnosticsLog, "utf8") : "";
  check("the installed app starts", diagnostics.status === 0, "exit=" + diagnostics.status);

  let report_ = null;
  const brace = raw.indexOf("{");
  if (brace >= 0) {
    try {
      report_ = JSON.parse(raw.slice(brace));
    } catch {
      report_ = null;
    }
  }
  check("it reports diagnostics", !!report_ && typeof report_.dataDir === "string", raw.slice(0, 200));
  check(
    "the report names the data directory it uses",
    !!report_ && report_.dataDir.includes("devchievements"),
    report_ ? report_.dataDir : "(no report)",
  );
  check("the report carries a storage snapshot", !!report_ && !!report_.snapshot, raw.slice(0, 200));

  const db = path.join(dataRoot, "devchievements", "companion.db");
  check("it creates its database outside the install directory", fs.existsSync(db), db);

  // Spec 40.3: the release build must be able to say where it stands on updates
  // rather than claiming it checked a feed that does not exist.
  const updateLog = path.join(scratch, "update-status.json");
  const updateRun = run(mainExe, ["--update-status"], {
    logName: "update-status.json",
    env: { ...process.env, LOCALAPPDATA: dataRoot },
  });
  const updateRaw = fs.existsSync(updateLog) ? fs.readFileSync(updateLog, "utf8") : "";
  let updateStatus = null;
  const updateBrace = updateRaw.indexOf("{");
  if (updateBrace >= 0) {
    try {
      updateStatus = JSON.parse(updateRaw.slice(updateBrace));
    } catch {
      updateStatus = null;
    }
  }
  check("the installed app reports its update state", updateRun.status === 0 && !!updateStatus, updateRaw.slice(0, 200));
  check(
    "it does not claim a feed it does not have",
    !!updateStatus && updateStatus.configured === false,
    updateStatus ? JSON.stringify(updateStatus) : "(no status)",
  );

  // Spec 25: prerequisites are checked during installation, before anything is
  // copied. A machine that cannot run the app must end up with no installation at
  // all, rather than with a program that opens a blank window and dies.
  const blockedDir = path.join(scratch, "blocked");
  const blocked = spawnSync(installer, ["/S", "/NS", "/D=" + blockedDir], {
    stdio: "ignore",
    windowsHide: true,
    timeout: 240000,
    env: { ...process.env, DEVCH_FORCE_NO_WEBVIEW2: "1" },
  });
  check("a machine without WebView2 fails the install", blocked.status === 2, "exit=" + blocked.status);
  check(
    "and nothing was installed",
    !fs.existsSync(path.join(blockedDir, "devchievements-desktop.exe")),
    blockedDir,
  );

  // Uninstall in place so the uninstaller can remove its own directory.
  const removal = run(uninstaller, ["/S", "_?=" + installDir]);
  const gone = waitFor(() => !fs.existsSync(mainExe), 120000);
  check("the uninstaller succeeds", removal.status === 0, "exit=" + removal.status);
  check("it removes the desktop app", gone);
  check("it removes the helper", !fs.existsSync(helperExe));

  // Spec 26.4: user data is the user's. Uninstalling is not a data wipe.
  check("it keeps the user database", fs.existsSync(db), db);

  report();
}

function report() {
  console.log("");
  if (failures) {
    console.log("INSTALLER FAILED (" + failures + " of " + checks + " checks)");
    process.exitCode = 1;
  } else {
    console.log("INSTALLER PASSED: install, run and uninstall work and data survives");
  }
}

main();

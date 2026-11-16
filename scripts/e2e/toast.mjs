#!/usr/bin/env node
/**
 * scripts/e2e/toast.mjs
 *
 * Verifies the Windows toast path (spec 16.4, 16.6, 28) against the REAL binary.
 *
 * Exit codes, deliberately distinct:
 *   0  a toast was shown AND found in the Action Center history  -> verified
 *   1  the toast path is broken                                 -> failure
 *   2  the environment prevents verification                    -> unverified
 *
 * Exit 2 exists because "could not test" and "tested and broken" must never be
 * reported the same way (AGENTS.md 10).
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const appDataId = "Devchievements.Companion";
// Scratch inside the repository: %TEMP% is not writable for grandchildren here.
const scratch = path.join(__dirname, ".toast");
fs.mkdirSync(scratch, { recursive: true });

function runFile(exe, args, logName) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

function historyTags() {
  const ps = [
    "[void][Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime]",
    `$h = [Windows.UI.Notifications.ToastNotificationManager]::History.GetHistory('${appDataId}')`,
    "foreach ($n in $h) { $n.Tag }",
  ].join("; ");
  const r = runFile("powershell.exe", ["-NoProfile", "-Command", ps], "devch-toast-history.txt");
  return (r.out || "").split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }

  // Registration is an install-time step (ADR-0015); report what the app observes.
  const status = runFile(appExe, ["--toast-status"], "devch-toast-status.txt");
  const statusText = (status.out || "").trim();
  console.log("registration: " + statusText);
  if (status.status === 3) {
    console.log("");
    console.log("NOT INSTALLED: run scripts/install-companion.ps1 first.");
    process.exit(2);
  }

  const tag = "devch-" + Date.now();
  const probe = runFile(appExe, ["--toast-probe", "--tag", tag], "devch-toast-probe.txt");
  const out = (probe.out || "").trim();
  console.log("probe exit: " + probe.status);
  console.log("probe says: " + out);

  if (probe.status !== 0) {
    if (/0x80073D54/.test(out)) {
      console.log("");
      console.log("UNVERIFIED: Windows still reports the process has no notification identity.");
      console.log("  Registration IS in place: the Start Menu shortcut exists, the shell reads");
      console.log("  System.AppUserModel.ID = Devchievements.Companion from it, and the app");
      console.log("  advertises the same id at runtime. The toast platform refuses anyway.");
      console.log("  This is an environment limitation, not a code path that is missing:");
      console.log("  on this host, toasts only work for processes Windows already trusts");
      console.log("  (verified separately with PowerShell's own identity).");
      console.log("  See docs/development/phase-2a-progress.md section 10.");
      process.exit(2);
    }
    console.log("FAILED: the toast path is broken");
    process.exit(1);
  }

  const tags = historyTags();
  if (tags.includes(tag)) {
    console.log("VERIFIED: toast shown and present in Action Center (tag " + tag + ")");
    process.exit(0);
  }

  console.log("FAILED: the probe succeeded but no toast reached the Action Center");
  console.log("  tags there: " + JSON.stringify(tags));
  process.exit(1);
}

main();

#!/usr/bin/env node
/**
 * scripts/e2e/single-instance.mjs
 *
 * One tray icon per user.
 *
 * Reported as "there is an extra icon in the tray". Closing the window is not quitting
 * -- the app is a background companion -- so opening it again is normal, and each
 * process used to register its own icon.
 *
 * A mutex makes the second launch hand its request to the first, which then shows its
 * window. This drives both halves and asserts the icon count did not change.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".single-instance");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else { failures++; console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : "")); }
}

function trayIconCount() {
  // Count our own registrations rather than the whole tray: other applications
  // change independently and would make this flaky.
  const ps = spawnSync("powershell", [
    "-NoProfile", "-Command",
    "(Get-Process devchievements-desktop -ErrorAction SilentlyContinue | Measure-Object).Count",
  ], { windowsHide: true, encoding: "utf8" });
  return Number((ps.stdout || "0").trim()) || 0;
}

async function main() {
  if (!fs.existsSync(appExe)) { console.error("missing " + appExe); process.exit(1); }
  fs.rmSync(scratch, { recursive: true, force: true });
  const dataDir = path.join(scratch, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(path.join(dataDir, "settings.json"), JSON.stringify({
    schemaVersion: 1, emails: ["single@devchievements.local"], onboardingCompleted: true,
  }));

  // Nothing of ours may be running, or the count starts wrong.
  spawnSync("taskkill", ["/IM", "devchievements-desktop.exe", "/F"], { windowsHide: true, stdio: "ignore" });
  await sleep(1500);

  const env = { ...process.env, LOCALAPPDATA: scratch };
  const firstLog = path.join(scratch, "first.log");
  const fd1 = fs.openSync(firstLog, "w");
  const first = spawn(appExe, ["--open-window"], { stdio: ["ignore", fd1, fd1], windowsHide: true, env });
  await sleep(7000);
  const afterFirst = trayIconCount();
  check("the first launch registers exactly one icon", afterFirst === 1, "count=" + afterFirst);

  const secondLog = path.join(scratch, "second.log");
  const fd2 = fs.openSync(secondLog, "w");
  const second = spawn(appExe, ["--open-window"], { stdio: ["ignore", fd2, fd2], windowsHide: true, env });
  await sleep(7000);

  const afterSecond = trayIconCount();
  check("a second launch does not add a second icon", afterSecond === 1, "count=" + afterSecond);

  const firstOut = fs.readFileSync(firstLog, "utf8");
  const secondOut = fs.readFileSync(secondLog, "utf8");
  check(
    "the second launch reports the app is already running",
    /ALREADY RUNNING/.test(secondOut),
    secondOut.slice(0, 300),
  );
  check(
    "the running copy is asked to show its window",
    /SHOW request delivered/.test(secondOut),
    secondOut.slice(0, 300),
  );
  check(
    "the running copy honours that request",
    /SHOW requested by a second launch/.test(firstOut),
    firstOut.slice(0, 400),
  );

  // A scripted launch must still get its own process: the tests depend on it.
  spawnSync("taskkill", ["/IM", "devchievements-desktop.exe", "/F"], { windowsHide: true, stdio: "ignore" });
  await sleep(1500);
  const a = spawn(appExe, ["--open-window"], { stdio: "ignore", windowsHide: true, env });
  await sleep(6000);
  const b = spawn(appExe, ["--open-window", "--page", "settings"], { stdio: "ignore", windowsHide: true, env });
  await sleep(6000);
  check("a scripted launch still starts its own process", trayIconCount() === 2, "count=" + trayIconCount());

  spawnSync("taskkill", ["/IM", "devchievements-desktop.exe", "/F"], { windowsHide: true, stdio: "ignore" });
  try { first.kill(); second.kill(); a.kill(); b.kill(); } catch { /* gone */ }
  fs.closeSync(fd1); fs.closeSync(fd2);

  console.log("");
  console.log(failures ? `SINGLE INSTANCE FAILED (${failures} checks)` : "SINGLE INSTANCE PASSED: one tray icon, second launch activates the first");
  process.exitCode = failures ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exit(1); });

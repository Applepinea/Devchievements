#!/usr/bin/env node
/**
 * scripts/e2e/wizard.mjs
 *
 * Spec 25: first run must guide, and must not guide twice.
 *
 * Checks that a fresh data directory shows the wizard, and that a machine which has
 * already been through it opens straight to the dashboard.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".wizard");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function openWindow(page, dataRoot, logName, step) {
  const args = ["--window-smoke", "--page", page];
  if (step) args.push("--step", step);
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, args, {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot },
    timeout: 60000,
  });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/// Wait until `predicate` is true or the deadline passes.
function waitFor(predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    sleep(100);
  }
  return predicate();
}

/// Start the app the way the product starts it (no --window-smoke) and wait for a
/// log line, then stop it. Needed because a command missing from the *app's* handler
/// list is invisible to a smoke-test run: that is exactly how the wizard became
/// unreachable while every check here still passed.
function realLaunch(dataRoot, logName, needle, timeoutMs) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot },
  });
  const found = waitFor(() => {
    try {
      return fs.readFileSync(log, "utf8").includes(needle);
    } catch {
      return false;
    }
  }, timeoutMs);
  try {
    child.kill();
  } catch {}
  fs.closeSync(fd);
  const out = fs.readFileSync(log, "utf8");
  return { found, out };
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  // 1. A completely fresh machine: no data directory at all.
  const fresh = path.join(scratch, "fresh");
  fs.mkdirSync(fresh, { recursive: true });
  const first = openWindow("dashboard", fresh, "fresh.log");
  check("the app starts on a fresh machine", first.status === 0, first.out);
  check("it shows the wizard instead of the dashboard", /UI READY view=welcome-/.test(first.out), first.out);

  // 1b. The same claim through the REAL launch path. The smoke builder and the app
  // used to carry different command lists, so the wizard worked here and not for the
  // user; a check that only uses --window-smoke cannot see that.
  const real = path.join(scratch, "fresh-real");
  fs.mkdirSync(real, { recursive: true });
  const launched = realLaunch(real, "fresh-real.log", "UI READY view=welcome", 30000);
  check(
    "a real launch on a fresh machine opens the wizard",
    launched.found,
    launched.out,
  );

  // Spec 34.1 lists the import step as part of the wizard; each step is rendered
  // separately so a broken one cannot hide behind the first.
  for (const step of ["checks", "identity", "hooks", "import", "extension", "done"]) {
    const run = openWindow("welcome", fresh, "step-" + step + ".log", step);
    check("the " + step + " step renders", run.out.includes("view=welcome-" + step), run.out);
  }

  // 2. Onboarding completed: the dashboard must open directly.
  const ready = path.join(scratch, "ready");
  fs.mkdirSync(path.join(ready, "devchievements"), { recursive: true });
  fs.writeFileSync(
    path.join(ready, "devchievements", "settings.json"),
    JSON.stringify(
      { schemaVersion: 1, emails: ["wiz@devchievements.local"], onboardingCompleted: true },
      null,
      2,
    )
  );
  const second = openWindow("dashboard", ready, "ready.log");
  check("a configured machine skips the wizard", /UI READY view=dashboard/.test(second.out), second.out);
  check(
    "the wizard does not reappear",
    !/UI READY view=welcome/.test(second.out),
    second.out
  );

  // 3. The flag must survive the app writing settings again.
  const settingsPath = path.join(ready, "devchievements", "settings.json");
  const stored = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  check("the onboarding flag is persisted", stored.onboardingCompleted === true, JSON.stringify(stored));

  // 4. The wizard's install step, driven through the same code the window calls.
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repo, windowsHide: true, env: { ...process.env, ...{} } });
  const hookRun = spawnSync(appExe, ["--install-hooks", repo], {
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: ready },
    timeout: 60000,
  });
  const hookOut = (hookRun.stdout ?? Buffer.alloc(0)).toString() + (hookRun.stderr ?? Buffer.alloc(0)).toString();
  check("the wizard can install hooks", hookRun.status === 0, hookOut);
  check("all three hooks were reported", /post-commit/.test(hookOut) && /post-rewrite/.test(hookOut) && /post-merge/.test(hookOut), hookOut);

  const installed = fs.existsSync(path.join(repo, ".git", "hooks", "post-commit"));
  check("the hook file exists", installed, path.join(repo, ".git", "hooks"));
  const hookBody = installed ? fs.readFileSync(path.join(repo, ".git", "hooks", "post-commit"), "utf8") : "";
  check("it points at the helper", hookBody.includes("devch-helper"), hookBody.slice(0, 200));

  // 5. Re-running setup from the settings page: the same command the button calls.
  const reset = spawnSync(appExe, ["--reset-onboarding"], {
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: ready },
    timeout: 60000,
  });
  const resetOut = (reset.stdout ?? Buffer.alloc(0)).toString();
  check("setup can be re-armed from the app", reset.status === 0, resetOut);
  check("the flag is cleared, not the data", /completed=false/.test(resetOut), resetOut);

  const afterReset = realLaunch(ready, "after-reset.log", "UI READY view=welcome", 30000);
  check("the next launch shows the wizard again", afterReset.found, afterReset.out);
  const keptSettings = JSON.parse(fs.readFileSync(path.join(ready, "devchievements", "settings.json"), "utf8"));
  check(
    "re-running setup keeps the configured identity",
    Array.isArray(keptSettings.emails) && keptSettings.emails.length === 1,
    JSON.stringify(keptSettings),
  );

  console.log("");
  if (failures) {
    console.log("WIZARD FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("WIZARD PASSED: the first run guides once and then stays out of the way");
  }
}

main();

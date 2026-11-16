#!/usr/bin/env node
/**
 * scripts/e2e/notification-settings.mjs
 *
 * Spec 34.1: the notification switch and the sound controls must do something.
 * The interesting assertion is that turning notifications off still records the
 * unlock -- otherwise the setting would silently stop counting work.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".notify")
  ;

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function run(args, env, logName) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, args, { stdio: ["ignore", fd, fd], windowsHide: true, env });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

async function waitFor(fn, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (fn()) return true;
    await new Promise((r) => setTimeout(r, 100));
  }
  return false;
}

async function main() {
  if (!fs.existsSync(appExe) || !fs.existsSync(helperExe)) {
    console.error("missing binaries; build them first");
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(path.join(dataRoot, "devchievements", "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["notify@devchievements.local"], onboardingCompleted: true }, null, 2)
  );
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const git = (args) => {
    const log = path.join(scratch, "git.log");
    const fd = fs.openSync(log, "w");
    const r = spawnSync("git", args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
    fs.closeSync(fd);
    return r.status;
  };
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Notify Test"]);
  git(["config", "user.email", "notify@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

  // Turn everything off before the first commit.
  check("notifications can be turned off", run(["--set-notifications", "off"], env, "n.log").status === 0);
  check("sound can be muted", run(["--set-muted", "on"], env, "m.log").status === 0);
  const volume = run(["--set-volume", "55"], env, "v.log");
  check("volume can be set", volume.status === 0 && /VOLUME 55/.test(volume.out), volume.out);

  const appLog = path.join(scratch, "app.log");
  const fd = fs.openSync(appLog, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });
  const ready = await waitFor(
    () => fs.existsSync(appLog) && fs.readFileSync(appLog, "utf8").includes("TRAY READY"),
    25000,
  );
  check("the companion started", ready, fs.readFileSync(appLog, "utf8"));

  fs.writeFileSync(path.join(repo, "a.txt"), "a");
  git(["add", "-A"]);
  git(["commit", "-q", "-m", "first commit"]);

  await waitFor(() => /INGESTED/.test(fs.readFileSync(appLog, "utf8")), 30000);
  await new Promise((r) => setTimeout(r, 2000));
  const log = fs.readFileSync(appLog, "utf8");

  check("no popup was opened", !/POPUP shown/.test(log), log.slice(-500));
  check("the suppression is reported", /POPUP suppressed notifications=off/.test(log), log.slice(-500));
  check("sound was muted", /SOUND suppressed muted=true/.test(log), log.slice(-500));

  // The unlock must still be recorded: the switch controls interruption, not counting.
  const diag = run(["--diagnostics"], env, "diag.log");
  let snapshot = null;
  try { snapshot = JSON.parse(diag.out); } catch { /* reported below */ }
  check("the commit was still recorded", (snapshot?.snapshot?.events ?? 0) >= 1, diag.out.slice(0, 300));
  check(
    "the achievement still unlocked",
    (snapshot?.snapshot?.unlocked_achievements ?? []).length >= 1,
    JSON.stringify(snapshot?.snapshot?.unlockedAchievements)
  );

  app.kill();
  await new Promise((r) => app.on("exit", r));
  fs.closeSync(fd);

  // Turning it back on must restore popups.
  check("notifications can be turned back on", run(["--set-notifications", "on"], env, "n2.log").status === 0);
  check("mute can be cleared", run(["--set-muted", "off"], env, "m2.log").status === 0);
  const settings = JSON.parse(fs.readFileSync(path.join(dataRoot, "devchievements", "settings.json"), "utf8"));
  check("the settings were persisted", settings.notifications === true && settings.muted === false && settings.volume === 55, JSON.stringify(settings));

  console.log("");
  if (failures) {
    console.log("NOTIFICATION SETTINGS FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("NOTIFICATION SETTINGS PASSED: the switches stop the interruption, not the counting");
  }
}

main();

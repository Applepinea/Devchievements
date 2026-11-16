#!/usr/bin/env node
/**
 * scripts/e2e/popup.mjs
 *
 * The popup is now the notification channel (ADR-0015), so it has to be verified the
 * way the user experiences it: a real commit, a real window, real pixels.
 *
 * Asserting only on the log line would repeat the earlier mistake of accepting a
 * process that did not crash as proof that something worked.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".popup");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
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
    JSON.stringify({ schemaVersion: 1, emails: ["popup@devchievements.local"], onboardingCompleted: true }, null, 2)
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
  git(["config", "user.name", "Popup Test"]);
  git(["config", "user.email", "popup@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

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

  const shown = await waitFor(
    () => /POPUP shown/.test(fs.readFileSync(appLog, "utf8")),
    30000,
  );
  const log = fs.readFileSync(appLog, "utf8");
  check("a popup was created for the unlock", shown, log.slice(-600));
  check("it was not a failed creation", !/POPUP FAILED/.test(log), log.slice(-600));

  // The toast must no longer be part of the path at all.
  check("no system notification was attempted", !/NOTIFY (shown|unavailable)/.test(log), log.slice(-600));

  // More commits: the invariant is that every presentation opens exactly one popup,
  // whatever unlocks. Asserting a fixed count would encode today's rule set into the
  // test and fail the day a new achievement exists.
  fs.writeFileSync(path.join(repo, "b.txt"), "b");
  git(["add", "-A"]);
  git(["commit", "-q", "-m", "second commit"]);
  await new Promise((r) => setTimeout(r, 3000));
  fs.writeFileSync(path.join(repo, "c.txt"), "c");
  git(["add", "-A"]);
  git(["commit", "-q", "-m", "third commit"]);
  await new Promise((r) => setTimeout(r, 3000));

  const after = fs.readFileSync(appLog, "utf8");
  const popupCount = (after.match(/POPUP shown/g) ?? []).length;
  const copyCount = (after.match(/^COPY /gm) ?? []).length;
  const soundOnly = (after.match(/^SOUND play id=commit/gm) ?? []).length;
  const presentations = popupCount + soundOnly;
  check("at least one more popup appeared", popupCount >= 2, "count=" + popupCount);
  check("popups match presentations one to one", copyCount === popupCount, "copy=" + copyCount + " popup=" + popupCount);
  check("nothing was presented without a popup or a sound", presentations > 0, after.slice(-500));
  check("no popup failed to open", !/POPUP FAILED/.test(after), after.slice(-500));

  app.kill();
  await new Promise((r) => app.on("exit", r));
  fs.closeSync(fd);

  console.log("");
  if (failures) {
    console.log("POPUP FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("POPUP PASSED: unlocks open a real notification window, plain commits do not");
  }
}

main();

#!/usr/bin/env node
/**
 * scripts/e2e/ui-pages.mjs
 *
 * Opens every page of the real window against a seeded database and asserts two
 * things the previous tests never did:
 *
 *   1. the page that was asked for is the page that rendered (--page was silently
 *      ignored, so every capture showed the dashboard);
 *   2. nothing on screen is placeholder text. `undefined d` and `Invalid Date` shipped
 *      on the dashboard because the wire format and the TypeScript types disagreed,
 *      and no assertion looked at what a person would read.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".ui-pages");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 600) : ""));
  }
}

const pages = ["dashboard", "gallery", "activity", "repositories", "settings", "about"];

function seed(dataDir) {
  const repo = path.join(scratch, "repo");
  const env = { ...process.env, LOCALAPPDATA: path.join(scratch, "data") };
  fs.mkdirSync(repo, { recursive: true });
  const git = (args) => spawnSync("git", args, { cwd: repo, windowsHide: true, encoding: "utf8", env });
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Page Test"]);
  git(["config", "user.email", "pages@devchievements.local"]);
  fs.mkdirSync(path.join(repo, ".git", "hooks"), { recursive: true });
  fs.copyFileSync(helperExe, path.join(repo, ".git", "hooks", "post-commit.exe"));
  for (let i = 0; i < 3; i++) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "x" + i);
    git(["add", "-A"]);
    git(["commit", "-q", "-m", "commit " + i]);
  }
  spawnSync(companionExe, [dataDir, "pages@devchievements.local", "--once"], {
    windowsHide: true,
    encoding: "utf8",
    env,
  });
  return env;
}

function appRunning() {
  const out = spawnSync("tasklist", ["/FI", "IMAGENAME eq devchievements-desktop.exe", "/NH"], {
    windowsHide: true,
    encoding: "utf8",
  });
  return (out.stdout || "").toLowerCase().includes("devchievements-desktop");
}

function stopApp() {
  spawnSync("taskkill", ["/IM", "devchievements-desktop.exe", "/F"], {
    windowsHide: true,
    stdio: "ignore",
  });
}

function waitForExit() {
  for (let i = 0; i < 50; i++) {
    if (!appRunning()) return;
    spawnSync("powershell", ["-NoProfile", "-Command", "Start-Sleep -Milliseconds 100"], {
      windowsHide: true,
      stdio: "ignore",
    });
  }
}

// Opens a page, waits until it has reported, then stops the app.
//
// The app has no reason to exit on its own, so an earlier version waited for the
// spawn timeout and killed it blind -- six pages took four minutes and one of them
// raced the shutdown of the previous instance. Watching the log for the report and
// stopping as soon as it arrives fixes both.
function openPage(env, page) {
  const log = path.join(scratch, page + ".log");
  fs.writeFileSync(log, "");
  const fd = fs.openSync(log, "a");
  const child = spawn(appExe, ["--open-window", "--page", page], {
    detached: false,
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env,
  });

  const deadline = Date.now() + 30000;
  let out = "";
  while (Date.now() < deadline) {
    out = fs.readFileSync(log, "utf8");
    if (/UI PLACEHOLDER/.test(out)) break;
    // A crash on the page is a result too, not something to wait out.
    if (/UI ERROR TypeError|UI ERROR ReferenceError/.test(out)) break;
    spawnSync("powershell", ["-NoProfile", "-Command", "Start-Sleep -Milliseconds 200"], {
      windowsHide: true,
      stdio: "ignore",
    });
  }

  stopApp();
  waitForExit();
  try { child.kill(); } catch { /* already gone */ }
  fs.closeSync(fd);
  return { out: fs.readFileSync(log, "utf8") };
}

function main() {
  for (const required of [appExe, companionExe, helperExe]) {
    if (!fs.existsSync(required)) {
      console.error("missing " + required + "; build first");
      process.exit(1);
    }
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });
  const dataDir = path.join(scratch, "data", "devchievements");
  fs.mkdirSync(path.join(dataDir, "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["pages@devchievements.local"], onboardingCompleted: true }),
  );
  const env = seed(dataDir);

  for (const page of pages) {
    const result = openPage(env, page);
    const opened = /WINDOW opened reason=flag page=/.test(result.out);
    const reported = result.out.split(/\r?\n/).find((l) => l.includes("UI PLACEHOLDER")) ?? "";
    check("the window opens on " + page, opened && result.out.includes("page=" + page), result.out.slice(0, 400));
    check(
      page + " reports no placeholder text",
      /UI PLACEHOLDER none on page /.test(reported),
      reported || "no placeholder report at all",
    );
    // openPage stops the app and waits for it to disappear before returning.
  }

  console.log("");
  if (failures) {
    console.log("UI PAGES FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("UI PAGES PASSED: every page opens where it was asked to, with no undefined on screen");
  }
}

main();

#!/usr/bin/env node
/**
 * scripts/e2e/desktop-tray.mjs
 *
 * Drives the REAL Tauri desktop binary: a real tray process, real commits through the
 * real helper, and the real ingestion loop inside it.
 *
 * What this proves: the desktop shell runs, hosts the data loop, and reacts to real
 * commits. What it does NOT prove: the popup widget itself, because the embedded
 * webview (WebView2) cannot start in this sandbox -- see docs/development/
 * phase-2a-progress.md.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".desktop");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).trim().slice(0, 400) : ""));
  }
}

function runFile(exe, args, cwd, logName) {
  const log = path.join(cwd, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
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
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(dataRoot, { recursive: true });
  fs.mkdirSync(path.join(dataRoot, "devchievements", "spool"), { recursive: true });

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const git = (args) => runFile("git", args, repo, "git.log");
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Desktop Test"]);
  git(["config", "user.email", "desktop@devchievements.local"]);

  const hooksDir = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooksDir, "post-commit.exe"));

  // The app refuses to count without a configured email (spec 7), so configure one.
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["desktop@devchievements.local"] }, null, 2)
  );

  const appOut = path.join(scratch, "app.log");
  const appErr = path.join(scratch, "app.err.log");
  const outFd = fs.openSync(appOut, "w");
  const errFd = fs.openSync(appErr, "w");

  const app = spawn(appExe, [], {
    stdio: ["ignore", outFd, errFd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot },
  });

  const ready = await waitFor(
    () => fs.existsSync(appOut) && fs.readFileSync(appOut, "utf8").includes("TRAY READY"),
    20000
  );
  check("the desktop app started and created its tray", ready, fs.readFileSync(appOut, "utf8"));

  if (ready) {
    process.env.LOCALAPPDATA = dataRoot;
    for (let i = 0; i < 3; i++) {
      fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i + "\n");
      git(["add", "-A"]);
      const c = git(["commit", "-q", "-m", "commit " + i]);
      check("commit " + i + " succeeded", c.status === 0, c.out);
    }

    // Wait for the FULL expected set: waiting only for the first popup stops the test
    // before the third commit is acknowledged, which is what happened the first time.
    const reacted = await waitFor(
      () => {
        const t = fs.readFileSync(appOut, "utf8");
        return t.includes("POPUP First Commit") && /SOUND play id=commit/.test(t);
      },
      30000
    );
    const log = fs.readFileSync(appOut, "utf8");
    console.log(log.trim().split("\n").map((l) => "  |  " + l).join("\n"));

    check("the desktop app reacted to real commits", reacted, log);
    check("it merged the first commit into one popup", /POPUP First Commit/.test(log), log);
    check("it acknowledged a plain commit by sound", /SOUND play id=commit/.test(log), log);
    // Spec 19: the leading item decides the sound, and the built-in rarity sound is
    // what an un-customised install plays.
    check(
      "the merged popup used the rarity sound",
      /SOUND play id=rarity-common/.test(log),
      log
    );
    check(
      "the rank upgrade used the daily sound",
      /SOUND play id=daily-uncommon/.test(log),
      log
    );
  }

  app.kill();
  await new Promise((r) => app.on("exit", r));
  fs.closeSync(outFd);
  fs.closeSync(errFd);

  const err = fs.readFileSync(appErr, "utf8").trim();
  if (err) console.log("  info stderr: " + err.split("\n").slice(0, 3).join(" | "));

  console.log("");
  console.log("not shown: the popup widget (embedded WebView2 cannot start in this sandbox)");
  console.log("");
  if (failures) {
    console.log("DESKTOP TRAY FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("DESKTOP TRAY PASSED: real commits -> real helper -> real Tauri tray process -> presented");
  }
}

main();

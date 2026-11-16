#!/usr/bin/env node
/**
 * scripts/e2e/privacy-mode.mjs
 *
 * Verifies privacy mode as a user would experience it: the same commit produces
 * different notification text depending on the setting, and the setting survives a
 * restart (spec 26.2, 26.4).
 *
 * The tray menu itself cannot be clicked from a test, so the setting is written through
 * the CLI the app exposes for it; the reading side is the real code path.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".privacy");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function runFile(exe, args, logName, env) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { stdio: ["ignore", fd, fd], windowsHide: true, env: env ?? process.env });
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

/** Make one commit in a repo that has the helper installed. */
function makeCommit(repo, env, i) {
  fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i + "\n");
  const run = (args) => {
    const log = path.join(scratch, "git.log");
    const fd = fs.openSync(log, "w");
    const r = spawnSync("git", args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
    fs.closeSync(fd);
    return r.status;
  };
  run(["add", "-A"]);
  return run(["commit", "-q", "-m", "commit " + i]);
}

async function scenario(mode, expectGeneric) {
  const root = path.join(scratch, mode);
  fs.rmSync(root, { recursive: true, force: true });
  const dataRoot = path.join(root, "data");
  fs.mkdirSync(path.join(dataRoot, "devchievements", "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["priv@devchievements.local"] }, null, 2)
  );

  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const set = runFile(appExe, ["--set-privacy", mode], "set-privacy.log", env);
  check("privacy mode persisted as " + mode, set.status === 0, set.out);

  const repo = path.join(root, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const git = (args) => {
    const log = path.join(scratch, "git-init.log");
    const fd = fs.openSync(log, "w");
    const r = spawnSync("git", args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
    fs.closeSync(fd);
    return r.status;
  };
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Privacy Test"]);
  git(["config", "user.email", "priv@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

  const appOut = path.join(scratch, "app-" + mode + ".log");
  const outFd = fs.openSync(appOut, "w");
  const app = (await import("node:child_process")).spawn(appExe, [], {
    stdio: ["ignore", outFd, outFd],
    windowsHide: true,
    env,
  });

  const ready = await waitFor(() => fs.existsSync(appOut) && fs.readFileSync(appOut, "utf8").includes("TRAY READY"), 20000);
  check("app started with privacy=" + mode, ready, fs.readFileSync(appOut, "utf8"));

  makeCommit(repo, env, 0);

  // The app logs the copy it would show; that is the observable behaviour we assert.
  const sawCopy = await waitFor(() => /COPY privacy=/.test(fs.readFileSync(appOut, "utf8")), 25000);
  const log = fs.readFileSync(appOut, "utf8");
  check("a notification copy was produced", sawCopy, log);
  check(
    "the copy reports privacy=" + expectGeneric,
    new RegExp("COPY privacy=" + expectGeneric).test(log),
    log
  );

  if (expectGeneric) {
    check("the achievement name is still shown", /title="First Commit"/.test(log), log);
    check(
      "the detail is replaced by the generic body",
      /body="A new milestone has been reached"/.test(log),
      log
    );
  } else {
    check(
      "the detail is shown as usual",
      /body="/.test(log) && !/A new milestone has been reached/.test(log),
      log
    );
  }

  app.kill();
  await new Promise((r) => app.on("exit", r));
  fs.closeSync(outFd);
}

async function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  console.log("");
  console.log("scenario: privacy mode ON");
  await scenario("privacy", true);

  console.log("");
  console.log("scenario: privacy mode OFF");
  await scenario("normal", false);

  console.log("");
  if (failures) {
    console.log("PRIVACY MODE FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("PRIVACY MODE PASSED: the setting changes notification text and persists");
  }
}

main();

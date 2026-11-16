#!/usr/bin/env node
/**
 * scripts/e2e/uninstall.mjs
 *
 * Spec 34.1: after uninstall, hooks left behind must exit silently.
 *
 * A hook that fails or prints would either block the user's commits or spam their
 * terminal long after the app is gone, so this checks both.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".uninstall");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function git(repo, args, env) {
  const log = path.join(scratch, "git.log");
  const fd = fs.openSync(log, "w");
  const r = spawnSync("git", args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
  fs.closeSync(fd);
  return r.status;
}

function main() {
  if (!fs.existsSync(helperExe)) {
    console.error("missing helper: " + helperExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  // A data directory with no devchievements folder: exactly what remains after
  // the user uninstalls and the folder is removed.
  const dataRoot = path.join(scratch, "appdata");
  fs.mkdirSync(dataRoot, { recursive: true });
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  git(repo, ["init", "-q", "-b", "main"], env);
  git(repo, ["config", "user.name", "Uninstall Test"], env);
  git(repo, ["config", "user.email", "gone@devchievements.local"], env);

  // The hook is still installed, but the companion is gone.
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

  // Run the hook the way git would, and look at what the user would see.
  const out = path.join(scratch, "hook.log");
  const fd = fs.openSync(out, "w");
  const direct = spawnSync(path.join(hooks, "post-commit.exe"), [], {
    cwd: repo,
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env,
  });
  fs.closeSync(fd);
  const text = fs.readFileSync(out, "utf8");

  check("the orphaned hook exits successfully", direct.status === 0, "status=" + direct.status);
  check("it prints nothing to the user", text.trim() === "", JSON.stringify(text.slice(0, 200)));

  // And a real commit must still succeed with the orphaned hook in place.
  fs.writeFileSync(path.join(repo, "a.txt"), "a");
  git(repo, ["add", "-A"], env);
  const commit = git(repo, ["commit", "-q", "-m", "after uninstall"], env);
  check("a commit still succeeds", commit === 0, "git exit=" + commit);

  // Nothing may be created in a data directory the app no longer owns.
  const created = fs.existsSync(path.join(dataRoot, "devchievements"))
    ? fs.readdirSync(path.join(dataRoot, "devchievements"))
    : [];
  check("it does not recreate the data directory", created.length === 0, JSON.stringify(created));

  console.log("");
  if (failures) {
    console.log("UNINSTALL FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("UNINSTALL PASSED: orphaned hooks exit silently and do not block commits");
  }
}

main();

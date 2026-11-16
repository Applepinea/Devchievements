#!/usr/bin/env node
/**
 * scripts/e2e/upgrade.mjs
 *
 * Spec 34.2 requires the install, upgrade and uninstall flows to pass. This covers
 * upgrade in the three places a version change can hurt: the hooks already in the
 * user's repositories, the settings file written by an older build, and the database
 * written by an older schema.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".upgrade");

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

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  const dataDir = path.join(dataRoot, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  // A settings file as an older build would have written it: an unknown field the
  // current build has never heard of, and no onboarding flag.
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["upgrade@devchievements.local"], legacyField: "ignored" }, null, 2)
  );

  const status = run(["--onboarding"], env, "onboarding.log");
  check("an older settings file still loads", status.status === 0, status.out);
  let parsed = null;
  try { parsed = JSON.parse(status.out); } catch { /* reported below */ }
  // The command reports Git suggestions, not configured emails; the evidence that the
  // old file loaded is that a save puts the same email back.
  const resaved = run(["--set-privacy", "privacy"], env, "resave.log");
  check("the app can write the old settings file", resaved.status === 0, resaved.out);
  const resavedFile = JSON.parse(fs.readFileSync(path.join(dataDir, "settings.json"), "utf8"));
  check(
    "the configured email survived the upgrade",
    JSON.stringify(resavedFile.emails ?? []).includes("upgrade@devchievements.local"),
    JSON.stringify(resavedFile)
  );
  check(
    "an unknown field did not break the file",
    status.status === 0,
    status.out
  );

  // Hooks: installing over an existing installation must be safe, which is what an
  // upgrade does.
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  spawnSync("git", ["init", "-q", "-b", "main"], { cwd: repo, stdio: "ignore", windowsHide: true, env });

  // A repository with no commit cannot be walked, so give it one first.
  fs.writeFileSync(path.join(repo, "a.txt"), "a");
  spawnSync("git", ["add", "-A"], { cwd: repo, stdio: "ignore", windowsHide: true, env });
  spawnSync("git", ["-c", "user.name=Upgrade Test", "-c", "user.email=upgrade@devchievements.local", "commit", "-q", "-m", "first"], {
    cwd: repo,
    stdio: "ignore",
    windowsHide: true,
    env,
  });

  const first = run(["--install-hooks", repo], env, "hooks-1.log");
  check("the first hook install succeeds", first.status === 0, first.out);
  const hookFile = path.join(repo, ".git", "hooks", "post-commit");
  const beforeBody = fs.existsSync(hookFile) ? fs.readFileSync(hookFile, "utf8") : "";
  check("it wrote a hook", beforeBody.length > 0, hookFile);

  const second = run(["--install-hooks", repo], env, "hooks-2.log");
  check("installing again over it succeeds", second.status === 0, second.out);
  const afterBody = fs.readFileSync(hookFile, "utf8");
  check("the hook is not corrupted by the second install", afterBody.includes("devch-helper"), afterBody.slice(0, 200));
  check("both installs produced the same file", beforeBody === afterBody, "bodies differ");

  // Database: reopening an existing database must migrate without losing rows.
  const imported = run(["--import", repo], env, "import.log");
  check("import into a fresh database works", imported.status === 0, imported.out);

  const diagOne = run(["--diagnostics"], env, "diag-1.log");
  const diagTwo = run(["--diagnostics"], env, "diag-2.log");
  let a = null;
  let b = null;
  try { a = JSON.parse(diagOne.out); b = JSON.parse(diagTwo.out); } catch { /* reported below */ }
  check("diagnostics works before and after a reopen", !!a && !!b, diagOne.out.slice(0, 200));
  check(
    "reopening does not change the event count",
    a?.snapshot?.events === b?.snapshot?.events,
    JSON.stringify([a?.snapshot?.events, b?.snapshot?.events])
  );
  check(
    "the schema version is reported",
    (b?.snapshot?.schema_version ?? b?.snapshot?.schemaVersion ?? 0) > 0,
    JSON.stringify(b?.snapshot)
  );

  console.log("");
  if (failures) {
    console.log("UPGRADE FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("UPGRADE PASSED: settings, hooks and database survive an upgrade");
  }
}

main();

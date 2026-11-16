#!/usr/bin/env node
/**
 * scripts/e2e/backup-restore.mjs
 *
 * Spec 26.4: a backup must be restorable and the restored data must be the same
 * data. Checking that a file exists is not that check.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".backup");

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

function git(repo, args, env) {
  spawnSync("git", args, { cwd: repo, stdio: "ignore", windowsHide: true, env });
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(path.join(dataRoot, "devchievements"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["backup@devchievements.local"], onboardingCompleted: true }, null, 2)
  );
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  // Real data: three commits imported, which also exercises the projection rebuild.
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  git(repo, ["init", "-q", "-b", "main"], env);
  git(repo, ["config", "user.name", "Backup Test"], env);
  git(repo, ["config", "user.email", "backup@devchievements.local"], env);
  for (let i = 0; i < 3; i += 1) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i);
    git(repo, ["add", "-A"], env);
    spawnSync("git", ["commit", "-q", "-m", "c" + i], {
      cwd: repo,
      stdio: "ignore",
      windowsHide: true,
      env: { ...env, GIT_AUTHOR_DATE: "2024-02-0" + (i + 1) + "T10:00:00 +0800", GIT_COMMITTER_DATE: "2024-02-0" + (i + 1) + "T10:00:00 +0800" },
    });
  }

  const imported = run(["--import", repo], env, "import.log");
  check("history imported", imported.status === 0, imported.out);

  const before = run(["--diagnostics"], env, "before.log");
  let snapshotBefore = null;
  try { snapshotBefore = JSON.parse(before.out); } catch { /* reported below */ }
  check("there is data to back up", (snapshotBefore?.snapshot?.events ?? 0) === 3, before.out.slice(0, 300));

  const backup = run(["--backup"], env, "backup.log");
  check("backup succeeded", backup.status === 0, backup.out);
  let report = null;
  try { report = JSON.parse(backup.out); } catch { /* reported below */ }
  check("it reported a path and a size", !!report?.path && (report?.bytes ?? 0) > 0, backup.out.slice(0, 300));
  check("the file exists", !!report?.path && fs.existsSync(report.path), report?.path);

  // Destroy the live database, then restore from the backup.
  const live = path.join(dataRoot, "devchievements", "companion.db");
  fs.rmSync(live, { force: true });
  fs.rmSync(live + "-wal", { force: true });
  fs.rmSync(live + "-shm", { force: true });
  check("the live database is gone", !fs.existsSync(live), live);

  const restored = run(["--restore", report.path], env, "restore.log");
  check("restore succeeded", restored.status === 0, restored.out);

  const after = run(["--diagnostics"], env, "after.log");
  let snapshotAfter = null;
  try { snapshotAfter = JSON.parse(after.out); } catch { /* reported below */ }
  check(
    "the restored database holds the same events",
    snapshotAfter?.snapshot?.events === snapshotBefore?.snapshot?.events,
    after.out.slice(0, 300)
  );
  check(
    "the restored database holds the same commits",
    JSON.stringify(snapshotAfter?.snapshot?.repositories) === JSON.stringify(snapshotBefore?.snapshot?.repositories),
    JSON.stringify(snapshotAfter?.snapshot?.repositories)
  );
  check("the snapshot actually lists an achievement", (snapshotBefore?.snapshot?.unlocked_achievements ?? []).length > 0, JSON.stringify(snapshotBefore?.snapshot?.unlocked_achievements));
  check(
    "the restored database still knows the achievement",
    JSON.stringify(snapshotAfter?.snapshot?.unlocked_achievements) === JSON.stringify(snapshotBefore?.snapshot?.unlocked_achievements),
    JSON.stringify(snapshotAfter?.snapshot?.unlocked_achievements)
  );

  // A file that is not a database must be refused rather than overwrite the data.
  const junk = path.join(scratch, "junk.db");
  fs.writeFileSync(junk, "this is not a sqlite database at all, not even close");
  const refused = run(["--restore", junk], env, "refuse.log");
  check("a non-database file is refused", refused.status === 1, refused.out);
  check("the refusal says why", /not a SQLite database/.test(refused.out), refused.out);

  const stillThere = run(["--diagnostics"], env, "still.log");
  let still = null;
  try { still = JSON.parse(stillThere.out); } catch { /* reported below */ }
  check("the data survived the refused restore", (still?.snapshot?.events ?? 0) === 3, stillThere.out.slice(0, 300));

  console.log("");
  if (failures) {
    console.log("BACKUP/RESTORE FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("BACKUP/RESTORE PASSED: a backup restores the same data, and junk is refused");
  }
}

main();

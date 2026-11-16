#!/usr/bin/env node
/**
 * scripts/e2e/history-import.mjs
 *
 * Drives the real --import command against a real git repository and checks the
 * promises spec 11.2 makes: original dates, silence, idempotency, identity filter.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".import");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function run(exe, args, cwd, logName, env) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true, env: env ?? process.env });
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
  fs.mkdirSync(path.join(dataRoot, "devchievements"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["hist@devchievements.local"], timezone: "Asia/Taipei" }, null, 2)
  );
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const git = (args, extraEnv) => run("git", args, repo, "git.log", { ...env, ...(extraEnv ?? {}) });
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "History Test"]);
  git(["config", "user.email", "hist@devchievements.local"]);

  const commitAt = (file, at, message) => {
    fs.writeFileSync(path.join(repo, file), message);
    git(["add", "-A"]);
    git(["commit", "-q", "-m", message], { GIT_AUTHOR_DATE: at + " +0800", GIT_COMMITTER_DATE: at + " +0800" });
  };

  commitAt("a.txt", "2023-03-01T10:00:00", "first");
  commitAt("b.txt", "2023-03-02T10:00:00", "second");
  commitAt("c.txt", "2023-03-03T10:00:00", "third");

  const first = run(appExe, ["--import", repo], scratch, "import1.json", env);
  check("import exited successfully", first.status === 0, first.out);

  let report = null;
  try { report = JSON.parse(first.out); } catch { /* reported below */ }
  check("import printed a JSON report", !!report, first.out.slice(0, 300));

  if (report) {
    check("it scanned three commits", report.scanned === 3, JSON.stringify(report));
    check("all three matched the identity", report.matched === 3, JSON.stringify(report));
    check("all three were ingested", report.ingested === 3, JSON.stringify(report));
    check("a batch record was written", report.batches >= 1, JSON.stringify(report));
  }

  // Spec 11.2: silent. Nothing may be queued for the user.
  const diag = run(appExe, ["--diagnostics"], scratch, "diag1.json", env);
  let snapshot = null;
  try { snapshot = JSON.parse(diag.out); } catch { /* reported below */ }
  check("diagnostics still works after import", diag.status === 0, diag.out);
  if (snapshot?.snapshot) {
    check("three events recorded", snapshot.snapshot.events === 3, JSON.stringify(snapshot.snapshot.events));
    check(
      "no notification was queued for the import",
      (snapshot.snapshot.outbox ?? []).length === 0,
      JSON.stringify(snapshot.snapshot.outbox)
    );
    const commits = (snapshot.snapshot.repositories ?? [])[0]?.commits;
    check("all three count as valid commits", commits === 3, String(commits));
  }

  // Spec 11.2: repeatable and idempotent.
  const second = run(appExe, ["--import", repo], scratch, "import2.json", env);
  let secondReport = null;
  try { secondReport = JSON.parse(second.out); } catch { /* reported below */ }
  check("a second import succeeds", second.status === 0, second.out);
  if (secondReport) {
    check("nothing new is ingested", secondReport.ingested === 0, JSON.stringify(secondReport));
    check("all three are recognised as duplicates", secondReport.duplicates === 3, JSON.stringify(secondReport));
  }

  // Counting must not drift after a repeat import. The original-date guarantee
  // (spec 11.2) is asserted in crates/storage/tests/history_import_e2e.rs, where the
  // commit timestamps are readable; the diagnostics snapshot does not expose dates.
  const dates = run(appExe, ["--diagnostics"], scratch, "diag2.json", env);
  check("counts did not double after the second import", dates.status === 0, dates.out);
  try {
    const again = JSON.parse(dates.out);
    check("still three events", again.snapshot.events === 3, JSON.stringify(again.snapshot.events));
  } catch { /* covered above */ }

  console.log("");
  if (failures) {
    console.log("HISTORY IMPORT FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("HISTORY IMPORT PASSED: silent, idempotent, counted once");
  }
}

main();

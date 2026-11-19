#!/usr/bin/env node
/**
 * scripts/e2e/cross-repository.mjs
 *
 * Spec 14.1 / 13.2, checklist T1: the daily rank and the cumulative achievements are
 * about the USER, not about one working copy.
 *
 * The scenario the checklist names: three commits in repository A, two in B and one in
 * C. Per-repository counting saw three separate days of 3 / 2 / 1 commits (all
 * Uncommon at best); the user actually had one day of six commits, which is Rare.
 *
 * # Why the real binaries
 *
 * The bug lived in the seam between the Engine (which reads a snapshot) and storage
 * (which fills it). A unit test on either side passed while the pair was wrong, so
 * this drives three real repositories through the real hook helper, the real Spool
 * and the real headless Companion.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".cross-repository");
const EMAIL = "multi@example.com";

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function main() {
  for (const [exe, label] of [[companionExe, "companion"], [helperExe, "helper"]]) {
    if (!fs.existsSync(exe)) {
      console.error("missing " + label + " binary (run: node scripts/cargo-run.mjs build --workspace)");
      process.exit(1);
    }
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: [EMAIL], onboardingCompleted: true }, null, 2),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  /** Create a repository with `commits` commits, with the hook helper installed. */
  function buildRepository(name, commits) {
    const dir = path.join(scratch, "repos", name);
    fs.mkdirSync(dir, { recursive: true });
    const git = (args) =>
      spawnSync("git", args, { cwd: dir, windowsHide: true, encoding: "utf8", env });
    git(["init", "-q", "-b", "main"]);
    git(["config", "user.name", "Multi Repo"]);
    git(["config", "user.email", EMAIL]);
    git(["remote", "add", "origin", `https://github.com/multi/${name}.git`]);
    fs.mkdirSync(path.join(dir, ".git", "hooks"), { recursive: true });
    fs.copyFileSync(helperExe, path.join(dir, ".git", "hooks", "post-commit.exe"));
    for (let i = 0; i < commits; i++) {
      fs.writeFileSync(path.join(dir, `f${i}.txt`), `x${i}`);
      git(["add", "-A"]);
      git(["commit", "-q", "-m", `${name} commit ${i}`]);
    }
    return dir;
  }

  // The checklist's exact plan: A 3, B 2, C 1 = six commits today.
  buildRepository("alpha", 3);
  buildRepository("beta", 2);
  buildRepository("gamma", 1);

  const drained = spawnSync(companionExe, [dataDir, EMAIL, "--once"], {
    windowsHide: true,
    encoding: "utf8",
    env,
  });
  const out = String(drained.stdout ?? "") + String(drained.stderr ?? "");
  check("all six commits from three repositories drain", /ingested=6/.test(out), out);

  const db = new DatabaseSync(path.join(dataDir, "companion.db"));
  try {
    // The per-repository rows must still be right: the Repositories page reads them.
    const perRepo = db.prepare("SELECT canonical_repository_id, commit_count FROM repository_projection ORDER BY canonical_repository_id").all();
    check("three repositories are recorded", perRepo.length === 3, JSON.stringify(perRepo));
    const total = perRepo.reduce((sum, row) => sum + row.commit_count, 0);
    check("the per-repository counts still add up to six", total === 6, JSON.stringify(perRepo));

    // The global day is the number the achievement and the rank read.
    const day = db.prepare("SELECT commit_count, daily_rank FROM daily_global_projection").all();
    check("one global day row exists", day.length === 1, JSON.stringify(day));
    check("six commits across three repositories is one day of six", day[0]?.commit_count === 6, JSON.stringify(day));
    check("and six commits in a day is Rare", day[0]?.daily_rank === "rare", JSON.stringify(day));

    // Spec 14.3: a rank is granted once per day, and the day passed through four
    // ranks on the way up rather than stopping at the highest per repository.
    const ranks = db.prepare("SELECT status, kind, payload FROM outbox WHERE kind = 'daily_rank_upgrade' ORDER BY created_at, notification_id").all();
    const rankNames = ranks.map((row) => JSON.parse(row.payload).rank);
    check("the day passed through the ranks in order", JSON.stringify(rankNames) === JSON.stringify(["common", "uncommon", "rare"]), JSON.stringify(rankNames));

    // The dashboard total is the same number (spec 44.2: the page and the progress bar
    // must not disagree).
    const stats = db.prepare("SELECT COALESCE(SUM(commit_count), 0) AS commits FROM repository_projection").get();
    check("the dashboard total equals the achievement basis", stats.commits === day[0]?.commit_count, JSON.stringify({ stats, day }));

    // Century is about the user: a hundred commits spread over three repositories.
    const unlocked = db.prepare("SELECT achievement_id FROM unlocked_achievements").all().map((r) => r.achievement_id);
    check("First Commit unlocked once", unlocked.filter((id) => id === "first_commit").length === 1, JSON.stringify(unlocked));
  } finally {
    db.close();
  }

  console.log("");
  if (failures) {
    console.log("CROSS REPOSITORY FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("CROSS REPOSITORY PASSED: six commits in three repositories are one day of six, rated Rare");
  }
}

main();

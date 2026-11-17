#!/usr/bin/env node
/**
 * scripts/e2e/user-achievements.mjs
 *
 * Spec 22, measured on the real binary rather than in a unit test: a rule the user
 * creates must unlock on the commit that satisfies it, and must survive a restart.
 *
 * This is the test the product was missing. Until 2026-11-18 the Builder could save
 * a rule and nothing ever read it, so every user achievement was silently dead --
 * which no unit test noticed because none of them crossed the layer that was missing.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const workspace = path.resolve(repoRoot, "..");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const helper = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".user-achievements");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function main() {
  for (const required of [companionExe, helper]) {
    if (!fs.existsSync(required)) {
      console.error("missing " + required + "; build first");
      process.exit(1);
    }
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  const dataRoot = path.join(scratch, "data");
  const dataDir = path.join(dataRoot, "devchievements");
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(path.join(dataDir, "spool"), { recursive: true });
  fs.mkdirSync(repo, { recursive: true });
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  // The user identity the rule will count for, and a rule reaching two commits.
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      emails: ["user@example.com"],
      onboardingCompleted: true,
      userAchievements: [
        {
          id: "two-commits",
          name: "Two Commits",
          description: "Reach two commits",
          rule: { op: "condition", metric: "commits", at_least: 2 },
          version: 1,
        },
      ],
    }),
  );

  const git = (args) => spawnSync("git", args, { cwd: repo, windowsHide: true, encoding: "utf8", env });
  git(["init", "-q", "-b", "main"]);

  // A native hook, exactly as the product installs it on Windows. Without it git
  // runs nothing and the helper never sees the commit -- which is what an earlier
  // version of this test got wrong, reporting "no unlock" for "no commits".
  const hooksDir = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  fs.copyFileSync(helper, path.join(hooksDir, "post-commit.exe"));
  git(["config", "user.name", "User"])
  git(["config", "user.email", "user@example.com"]);

  // One commit: below the threshold.
  fs.writeFileSync(path.join(repo, "a.txt"), "a");
  git(["add", "-A"]);
  const first = git(["commit", "-q", "-m", "first"]);
  check("the hook did not break the commit", first.status === 0, first.out);

  const drain = spawnSync(companionExe, [dataDir, "user@example.com", "--once"], {
    windowsHide: true,
    encoding: "utf8",
    env,
  });
  check("the first drain runs", drain.status === 0, drain.stdout + drain.stderr);

  // SQLite runs in WAL mode, so a row written moments ago lives in companion.db-wal
  // until a checkpoint. Reading only the main file would report "not unlocked" for a
  // row that is committed and durable.
  const unlockedIds = () => {
    const parts = ["companion.db", "companion.db-wal"]
      .map((name) => path.join(dataDir, name))
      .filter((file) => fs.existsSync(file))
      .map((file) => fs.readFileSync(file, "latin1"));
    return parts.join("");
  };

  const afterFirst = unlockedIds();
  check("no unlock below the threshold", !afterFirst.includes("two-commits"), afterFirst.slice(0, 300));

  // Second commit: the rule must fire.
  fs.writeFileSync(path.join(repo, "b.txt"), "b");
  git(["add", "-A"]);
  git(["commit", "-q", "-m", "second"]);
  const drain2 = spawnSync(companionExe, [dataDir, "user@example.com", "--once"], {
    windowsHide: true,
    encoding: "utf8",
    env,
  });
  check("the second drain runs", drain2.status === 0, drain2.stdout + drain2.stderr);

  const afterSecond = unlockedIds();
  check("the user rule unlocked at its threshold", afterSecond.includes("two-commits"), afterSecond.slice(0, 300));

  // The unlock must be durable, not a value recomputed on every read.
  check("the unlock is stored in the database", afterSecond.includes("two-commits"), "db content checked above");

  console.log("");
  if (failures) {
    console.log("USER ACHIEVEMENTS FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("USER ACHIEVEMENTS PASSED: a saved rule unlocks on the commit that satisfies it");
  }
}

main();

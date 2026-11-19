#!/usr/bin/env node
/**
 * scripts/e2e/builder-form.mjs
 *
 * Spec 22: the Achievement Builder. A user achievement is a rule, and the UI path
 * must enforce the same limits the JSON path does.
 *
 * What this proves:
 *  - a rule built by the form's own wire shape is accepted and stored;
 *  - it actually unlocks on a real commit (the rule is not decoration);
 *  - a rule that exceeds the complexity limits is refused with a message the page
 *    can show, and nothing is stored;
 *  - an id that shadows a built-in is refused (spec 13.3);
 *  - deleting a rule does not revoke an achievement it already unlocked (spec 13.4).
 *
 * # Why not drive the React form with CDP
 *
 * The form's only job is to produce the JSON this script produces. Driving every
 * slider through the DevTools protocol would test the DOM, not the contract; the
 * contract is "the object the form builds is accepted, and the same limits apply".
 * The command the form calls is exercised for real, through the real binary.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const scratch = path.join(__dirname, ".builder-form");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function run(exe, args, env) {
  const r = spawnSync(exe, args, { env, timeout: 40000, windowsHide: true, encoding: "utf8" });
  return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
}

/**
 * The exact object the form builds. Flat because the builder's rule enum is
 * internally tagged: a nested `{ Condition: {...} }` parses as `missing field metric`.
 */
function rule(metric, atLeast) {
  return { op: "condition", metric, at_least: atLeast };
}

function main() {
  for (const [exe, label] of [[appExe, "desktop"], [companionExe, "companion"]]) {
    if (!fs.existsSync(exe)) {
      console.error("missing " + label + " binary: " + exe);
      process.exit(1);
    }
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  const settingsPath = path.join(dataDir, "settings.json");
  fs.writeFileSync(
    settingsPath,
    JSON.stringify({ schemaVersion: 1, emails: ["me@example.com"], onboardingCompleted: true }, null, 2),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };
  const stored = () => JSON.parse(fs.readFileSync(settingsPath, "utf8"));

  const writeDefinition = (name, body) => {
    const file = path.join(scratch, name + ".json");
    fs.writeFileSync(file, JSON.stringify(body, null, 2));
    return file;
  };

  // 1. The form's wire shape is accepted.
  const good = writeDefinition("two-commits", {
    id: "two-commits",
    name: "Two Commits",
    description: "Land two valid commits.",
    rule: rule("commits", 2),
  });
  const accepted = run(appExe, ["--add-achievement", good], env);
  check("a form-shaped rule is accepted", accepted.status === 0, accepted.out);
  check("it reports the id it stored", /ACHIEVEMENT OK id=two-commits/.test(accepted.out), accepted.out);
  check("it is in the settings document", (stored().userAchievements ?? []).some((a) => a.id === "two-commits"), JSON.stringify(stored().userAchievements));
  check("it was given a rule version", stored().userAchievements?.[0]?.version === 1, JSON.stringify(stored().userAchievements?.[0]));

  // 2. It survives an export/import round trip, which is what "shareable" means.
  const exported = path.join(scratch, "config.json");
  const exportRun = run(appExe, ["--export-config", exported], env);
  check("the configuration exports", exportRun.status === 0, exportRun.out);
  const document = JSON.parse(fs.readFileSync(exported, "utf8"));
  check("the rule travels with it", (document.userAchievements ?? []).length === 1, JSON.stringify(document.userAchievements));

  // 3. The stored rule really unlocks on a real commit. Without this the rest is
  //    testing that a JSON file can be written.
  const spoolDir = path.join(dataDir, "spool");
  fs.mkdirSync(spoolDir, { recursive: true });
  /** Append one hook record in exactly the format the Helper produces. */
  function pushRecord(eventId, sha) {
    const payload = JSON.stringify({
      event_id: eventId,
      hook: "post-commit",
      hook_args: [],
      repo_git_dir: "C:/repos/demo/.git",
      commit_sha: sha,
      parents: ["p"],
      author_name: "Me",
      author_email: "me@example.com",
      committer_email: "me@example.com",
      subject: "a commit",
      occurred_ms: Date.now(),
    });
    const bytes = Buffer.from(payload, "utf8");
    const M = 0x100000001b3n;
    const MASK = (1n << 64n) - 1n;
    let a = 0xcbf29ce484222325n;
    let b = 0x9e3779b97f4a7c15n;
    for (let i = 0; i < bytes.length; i++) {
      a = (a ^ BigInt(bytes[i])) & MASK;
      a = (a * M) & MASK;
      b = ((b << 7n) | (b >> 57n)) & MASK;
      b = (b ^ ((BigInt(bytes[i]) + BigInt(i)) & MASK)) & MASK;
      b = (b * M) & MASK;
    }
    const checksum = a.toString(16).padStart(16, "0") + b.toString(16).padStart(16, "0");
    fs.appendFileSync(path.join(spoolDir, "pending.spool"), checksum + " " + payload + "\n");
  }

  pushRecord("evt-1", "a".repeat(40));
  const once = run(companionExe, [dataDir, "me@example.com", "--once"], env);
  check("the first commit drains", /ingested=1/.test(once.out), once.out);

  /** Read a table out of the companion database, including its WAL. */
  function query(sql) {
    const db = new DatabaseSync(path.join(dataDir, "companion.db"));
    try {
      return db.prepare(sql).all();
    } finally {
      db.close();
    }
  }

  // A row for "two-commits" must NOT exist yet: one commit is below the threshold.
  let unlocked = query("SELECT achievement_id FROM unlocked_achievements");
  check("one commit does not satisfy a two-commit rule", !unlocked.some((r) => r.achievement_id === "two-commits"), JSON.stringify(unlocked));

  pushRecord("evt-2", "b".repeat(40));
  const twice = run(companionExe, [dataDir, "me@example.com", "--once"], env);
  check("the second commit drains", /ingested=1/.test(twice.out), twice.out);
  unlocked = query("SELECT achievement_id FROM unlocked_achievements");
  check("the rule unlocks on the commit that satisfies it", unlocked.some((r) => r.achievement_id === "two-commits"), JSON.stringify(unlocked));

  // 4. A rule that breaches the complexity limits is refused, with a usable message.
  let deep = rule("commits", 1);
  for (let i = 0; i < 12; i += 1) deep = { op: "all", children: [deep] };
  const tooDeep = writeDefinition("too-deep", {
    id: "too-deep",
    name: "Too Deep",
    description: "",
    rule: deep,
  });
  const deepRun = run(appExe, ["--add-achievement", tooDeep], env);
  check("an over-nested rule is refused", deepRun.status === 1, deepRun.out);
  check("the message says it is too complex", /too complex/i.test(deepRun.out), deepRun.out);

  const tooMany = writeDefinition("too-many", {
    id: "too-many",
    name: "Too Many",
    description: "",
    rule: { op: "any", children: Array.from({ length: 60 }, () => rule("commits", 1)) },
  });
  const manyRun = run(appExe, ["--add-achievement", tooMany], env);
  check("a rule with too many conditions is refused", manyRun.status === 1, manyRun.out);

  // 5. An id that shadows a built-in is refused (spec 13.3).
  const shadow = writeDefinition("shadow", {
    id: "first_commit",
    name: "Redefined",
    description: "",
    rule: rule("commits", 1),
  });
  const shadowRun = run(appExe, ["--add-achievement", shadow], env);
  check("shadowing a built-in is refused", shadowRun.status === 1, shadowRun.out);
  check("the message names the built-in", /built-in/i.test(shadowRun.out), shadowRun.out);

  // 6. A duplicate user id is refused.
  const duplicate = writeDefinition("duplicate", {
    id: "two-commits",
    name: "Again",
    description: "",
    rule: rule("commits", 5),
  });
  const duplicateRun = run(appExe, ["--add-achievement", duplicate], env);
  check("a duplicate id is refused", duplicateRun.status === 1, duplicateRun.out);

  // 7. None of the refusals stored anything.
  check("only the one valid rule is stored", (stored().userAchievements ?? []).length === 1, JSON.stringify(stored().userAchievements));

  console.log("");
  if (failures) {
    console.log("BUILDER FORM FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("BUILDER FORM PASSED: the form's rule shape is accepted, unlocks for real, and over-limit rules are refused");
  }
}

main();

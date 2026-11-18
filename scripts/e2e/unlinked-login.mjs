#!/usr/bin/env node
/**
 * scripts/e2e/unlinked-login.mjs
 *
 * The identity gate refuses a browser report whose GitHub account is not linked
 * (spec 7). That is correct. What was wrong is what happened next: the report was
 * deleted, so "you have not linked this account" was indistinguishable from "the
 * extension is broken" -- and linking later could not recover anything.
 *
 * What this proves, against the real binaries:
 *   1. a refused report is parked, not dropped;
 *   2. linking the account replays it through the same Spool path;
 *   3. the replayed report is ingested, unlocks its achievement, and celebrates.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const desktopExe = path.join(
  repoRoot,
  "apps",
  "desktop",
  "src-tauri",
  "target",
  "debug",
  "devchievements-desktop.exe",
);
const scratch = path.join(__dirname, ".unlinked");

const LOGIN = "Applepinea";
const EMAIL = "dev@example.com";

let failures = 0;
function check(label, ok, detail) {
  if (ok) {
    console.log("  ok   " + label);
  } else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).trim().slice(0, 400) : ""));
  }
}

function checksum(payload) {
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
  return a.toString(16).padStart(16, "0") + b.toString(16).padStart(16, "0");
}

function setup() {
  fs.rmSync(scratch, { recursive: true, force: true });
  const appdata = path.join(scratch, "appdata");
  const data = path.join(appdata, "devchievements");
  fs.mkdirSync(path.join(data, "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(data, "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      emails: [EMAIL],
      // Deliberately no githubLogins: this is the state the user was in.
      onboardingCompleted: true,
      startHidden: true,
    }),
  );

  const payload = JSON.stringify({
    kind: "browser_github",
    event_id: "browser:github:" + LOGIN + "/widgets#pull_request_merged:pr:5",
    repository_id: "github:" + LOGIN + "/widgets",
    activity: "pull_request",
    number: 5,
    merged: true,
    author_login: LOGIN,
    occurred_ms: Date.now() - 60_000,
  });
  fs.writeFileSync(
    path.join(data, "spool", "pending-unlinked.spool"),
    checksum(payload) + " " + payload + "\n",
  );
  return { appdata, data };
}

function runCompanion(data, appdata) {
  const log = path.join(scratch, "companion.log");
  const fd = fs.openSync(log, "w");
  spawnSync(companionExe, [data, EMAIL, "--once"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: appdata },
  });
  fs.closeSync(fd);
  return fs.readFileSync(log, "utf8");
}

function linkLogin(login, appdata) {
  const log = path.join(scratch, "link.log");
  const fd = fs.openSync(log, "w");
  spawnSync(desktopExe, ["--link-login", login], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: appdata },
  });
  fs.closeSync(fd);
  return fs.readFileSync(log, "utf8");
}

async function query(data, sql, ...params) {
  const { DatabaseSync } = await import("node:sqlite");
  const db = new DatabaseSync(path.join(data, "companion.db"), { readOnly: true });
  try {
    return db.prepare(sql).all(...params);
  } finally {
    db.close();
  }
}

async function main() {
  for (const [name, binary] of [
    ["companion", companionExe],
    ["desktop", desktopExe],
  ]) {
    if (!fs.existsSync(binary)) {
      console.error("missing " + name + " binary: " + binary);
      process.exit(1);
    }
  }

  const { appdata, data } = setup();

  console.log("");
  console.log("an unlinked account's report is refused but kept");
  {
    const out = runCompanion(data, appdata);
    check("the report is refused", /refused=1/.test(out), out);
    check("nothing is ingested", /ingested=0/.test(out), out);
    check("nothing is celebrated", !/NOTIFY popup/.test(out), out);

    const parked = await query(
      data,
      "SELECT login, repository_id FROM browser_reports_rejected",
    );
    check("the report is parked", parked.length === 1, JSON.stringify(parked));
    check("with the account that sent it", parked[0]?.login === LOGIN, JSON.stringify(parked[0]));
  }

  console.log("");
  console.log("linking the account replays what it sent");
  {
    const out = linkLogin(LOGIN, appdata);
    check("linking succeeds", /LINKED/.test(out), out);
    check("and reports a replay", /replayed=1/.test(out), out);

    const settings = JSON.parse(fs.readFileSync(path.join(data, "settings.json"), "utf8"));
    check(
      "the mapping is saved",
      Array.isArray(settings.githubLogins?.[LOGIN]) &&
        settings.githubLogins[LOGIN][0] === EMAIL,
      JSON.stringify(settings.githubLogins),
    );

    const parked = await query(data, "SELECT event_id FROM browser_reports_rejected");
    check("the parked copy is cleared", parked.length === 0, JSON.stringify(parked));
  }

  console.log("");
  console.log("the replayed report now counts and celebrates");
  {
    const out = runCompanion(data, appdata);
    check("it is ingested", /ingested=1/.test(out), out);
    check("with no refusal", /refused=0/.test(out), out);
    check("it celebrates", /NOTIFY popup/.test(out) && /title="Merged"/.test(out), out);

    const unlocked = await query(data, "SELECT achievement_id FROM unlocked_achievements");
    check(
      "first_pr_merged unlocked",
      unlocked.some((row) => row.achievement_id === "first_pr_merged"),
      JSON.stringify(unlocked),
    );
    const counters = await query(
      data,
      "SELECT pull_request_merged_count, commit_count FROM repository_projection",
    );
    check("it counts as a merged PR", counters[0]?.pull_request_merged_count === 1, JSON.stringify(counters));
    check("and never as a commit", counters[0]?.commit_count === 0, JSON.stringify(counters));
  }

  console.log("");
  if (failures) {
    console.log("UNLINKED LOGIN FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("UNLINKED LOGIN PASSED");
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

#!/usr/bin/env node
/**
 * scripts/e2e/browser-achievement.mjs
 *
 * Proves the path a browser report takes once it reaches the Companion: spool
 * record -> event -> projection -> achievement -> outbox -> popup.
 *
 * # Why this exists separately from native-host.mjs
 *
 * native-host.mjs proves the extension can hand a report to the host. It cannot
 * prove what the app does with it afterwards, and for a long time nothing happened:
 * the event was stored, the counters stayed at zero, and the user saw no popup. The
 * two failures look identical from the browser, so they are tested from both ends.
 *
 * # Why the real binary
 *
 * The engine, the projections and the outbox are all real here: a unit test could
 * pass while the wiring between them stayed broken, which is exactly the bug this
 * script was written for.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const scratch = path.join(__dirname, ".browser");

let failures = 0;
function check(label, ok, detail) {
  if (ok) {
    console.log("  ok   " + label);
  } else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).trim().slice(0, 400) : ""));
  }
}

/** Checksum matching devchievements_git_hooks::spool::checksum_of exactly. */
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

const REPO = "github:acme/widgets";
const LOGIN = "octo-dev";
const EMAIL = "dev@example.com";

/** A fresh data directory with an identity that owns the login. */
function fresh(name) {
  const dir = path.join(scratch, name);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(path.join(dir, "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dir, "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      emails: [EMAIL],
      githubLogins: { [LOGIN]: [EMAIL] },
      onboardingCompleted: true,
    }),
  );
  return dir;
}

/**
 * Write one browser report exactly as the Native Messaging host forwards it.
 *
 * The payload shape is what crates/git-hooks/src/browser_event.rs parses; writing it
 * here rather than going through the host keeps this script about the Companion.
 */
function browserRecord(dir, activity) {
  const payload = JSON.stringify({
    kind: "browser_github",
    event_id:
      "browser:" + REPO + "#" + activity.event_id,
    repository_id: REPO,
    activity: activity.kind,
    number: activity.number,
    merged: activity.merged,
    closed: activity.closed,
    external_id: activity.external_id,
    author_login: LOGIN,
    occurred_ms: Date.now() - (activity.ageMs ?? 60_000),
  });
  fs.appendFileSync(
    path.join(dir, "spool", "pending-browser.spool"),
    checksum(payload) + " " + payload + "\n",
  );
}

/**
 * Run the companion once and return its output.
 *
 * Output goes to a FILE, not a pipe: this sandbox denies pipe-based stdio capture to
 * child processes, which silently yields an empty string and makes every assertion
 * fail for the wrong reason.
 */
function runOnce(dir) {
  const log = path.join(dir, "run.log");
  const fd = fs.openSync(log, "w");
  spawnSync(companionExe, [dir, EMAIL, "--once"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
  });
  fs.closeSync(fd);
  return fs.readFileSync(log, "utf8");
}

async function query(dir, sql, ...params) {
  const { DatabaseSync } = await import("node:sqlite");
  const db = new DatabaseSync(path.join(dir, "companion.db"), { readOnly: true });
  try {
    return db.prepare(sql).all(...params);
  } finally {
    db.close();
  }
}

const mergedPr = {
  kind: "pull_request",
  event_id: "pull_request_merged:pr:7",
  number: 7,
  merged: true,
};
const openedPr = {
  kind: "pull_request",
  event_id: "pull_request_opened:pr:8",
  number: 8,
  merged: false,
};

async function main() {
  if (!fs.existsSync(companionExe)) {
    console.error("missing companion binary: " + companionExe);
    process.exit(1);
  }
  fs.mkdirSync(scratch, { recursive: true });

  console.log("");
  console.log("a merged pull request reaches the achievement and the popup");
  {
    const dir = fresh("merged");
    browserRecord(dir, mergedPr);
    const out = runOnce(dir);
    check("the record is ingested", /ingested=1/.test(out), out);
    check("it is not refused", /refused=0/.test(out), out);
    check("a popup is delivered", /NOTIFY popup/.test(out), out);
    check("the popup says Merged", /title="Merged"/.test(out), out);

    const counters = await query(
      dir,
      "SELECT commit_count, pull_request_merged_count FROM repository_projection",
    );
    check("the merged counter is 1", counters[0]?.pull_request_merged_count === 1, JSON.stringify(counters));
    check(
      "the commit counter is still 0",
      counters[0]?.commit_count === 0,
      "a merged PR must never be counted as a commit",
    );

    const unlocked = await query(dir, "SELECT achievement_id FROM unlocked_achievements");
    check(
      "first_pr_merged unlocked",
      unlocked.some((row) => row.achievement_id === "first_pr_merged"),
      JSON.stringify(unlocked),
    );

    const outbox = await query(dir, "SELECT status FROM outbox");
    check(
      "the notification was delivered, not left queued",
      outbox.length === 1 && outbox[0].status !== "created",
      JSON.stringify(outbox),
    );
  }

  console.log("");
  console.log("an opened pull request is recorded and stays silent");
  {
    const dir = fresh("opened");
    browserRecord(dir, openedPr);
    const out = runOnce(dir);
    check("the record is ingested", /ingested=1/.test(out), out);
    check("nothing is celebrated", !/NOTIFY popup/.test(out), out);

    const counters = await query(
      dir,
      "SELECT commit_count, pull_request_merged_count FROM repository_projection",
    );
    check("no counter moved", counters[0] === undefined || counters[0].pull_request_merged_count === 0, JSON.stringify(counters));
    const outbox = await query(dir, "SELECT notification_id FROM outbox");
    check("the outbox is empty", outbox.length === 0, JSON.stringify(outbox));
  }

  console.log("");
  console.log("the same report twice is counted once");
  {
    const dir = fresh("duplicate");
    browserRecord(dir, mergedPr);
    runOnce(dir);
    browserRecord(dir, mergedPr);
    const out = runOnce(dir);
    check("the replay is a duplicate", /dup=1/.test(out), out);
    check("nothing is celebrated again", !/NOTIFY popup/.test(out), out);
    const unlocked = await query(dir, "SELECT achievement_id FROM unlocked_achievements");
    check("one unlock", unlocked.length === 1, JSON.stringify(unlocked));
  }

  console.log("");
  console.log("a release unlocks its own achievement");
  {
    const dir = fresh("release");
    browserRecord(dir, {
      kind: "release",
      event_id: "release_published:release:1",
      external_id: "v1.0.0",
    });
    const out = runOnce(dir);
    check("the record is ingested", /ingested=1/.test(out), out);
    check("the popup says First Release", /title="First Release"/.test(out), out);
    const counters = await query(dir, "SELECT release_count FROM repository_projection");
    check("the release counter is 1", counters[0]?.release_count === 1, JSON.stringify(counters));
  }

  console.log("");
  console.log("an old activity still counts but stays silent (spec 16.3)");
  {
    const dir = fresh("stale");
    browserRecord(dir, {
      kind: "pull_request",
      event_id: "pull_request_merged:pr:9",
      number: 9,
      merged: true,
      ageMs: 3 * 24 * 60 * 60 * 1000,
    });
    const out = runOnce(dir);
    check("the record is ingested", /ingested=1/.test(out), out);
    check("but nothing is shown", !/NOTIFY popup/.test(out), out);
    const unlocked = await query(dir, "SELECT achievement_id FROM unlocked_achievements");
    check(
      "first_pr_merged is recorded anyway",
      unlocked.some((row) => row.achievement_id === "first_pr_merged"),
      JSON.stringify(unlocked),
    );
    const outbox = await query(dir, "SELECT kind FROM outbox");
    check(
      "nothing was queued for display either",
      outbox.length === 0,
      JSON.stringify(outbox),
    );
  }

  console.log("");
  console.log("an activity from this morning is summarised, not celebrated (spec 16.3)");
  {
    const dir = fresh("digest");
    browserRecord(dir, {
      kind: "pull_request",
      event_id: "pull_request_merged:pr:11",
      number: 11,
      merged: true,
      ageMs: 11 * 60 * 1000,
    });
    const out = runOnce(dir);
    check("the record is ingested", /ingested=1/.test(out), out);
    check(
      "no window is opened",
      !/NOTIFY popup/.test(out),
      out,
    );
    check(
      "one summary is shown instead",
      /NOTIFY digest/.test(out),
      out,
    );
    const unlocked = await query(dir, "SELECT achievement_id FROM unlocked_achievements");
    check(
      "the unlock is still real",
      unlocked.some((row) => row.achievement_id === "first_pr_merged"),
      JSON.stringify(unlocked),
    );
  }

  console.log("");
  if (failures) {
    console.log("BROWSER ACHIEVEMENT FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("BROWSER ACHIEVEMENT PASSED");
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});

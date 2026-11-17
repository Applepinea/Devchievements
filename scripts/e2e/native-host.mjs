#!/usr/bin/env node
/**
 * scripts/e2e/native-host.mjs
 *
 * Drives the Native Messaging host over stdin/stdout with the browser's own framing
 * (4-byte native-endian length, then UTF-8 JSON) and checks that a browser report
 * reaches the Spool, which is where the Companion picks it up.
 *
 * Why not through a real browser: the browser side is only framing and a socket.
 * The rules that can actually be wrong are in the host, and this exercises them
 * with real bytes on a real pipe.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const hostExe = path.join(repoRoot, "target", "debug", "devchievements-native-host.exe");
const scratch = path.join(__dirname, ".native-host");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

/** Frame one JSON message the way the browser does. */
function frame(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  return Buffer.concat([header, body]);
}

/** Split a stream of framed replies. */
function unframe(buffer) {
  const messages = [];
  let offset = 0;
  while (offset + 4 <= buffer.length) {
    const length = buffer.readUInt32LE(offset);
    if (offset + 4 + length > buffer.length) break;
    const body = buffer.subarray(offset + 4, offset + 4 + length);
    messages.push(JSON.parse(body.toString("utf8")));
    offset += 4 + length;
  }
  return messages;
}

function main() {
  if (!fs.existsSync(hostExe)) {
    console.error("missing host binary: " + hostExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  // Path convention: the app resolves its data directory as
  // LOCALAPPDATA/devchievements. Keep the host, the companion and the diagnostics
  // command on that same path, or they silently read different databases.
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  // Spec 7: the browser path attributes by login, so the mapping must exist before
  // the companion will count anything the host forwards.
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify(
      {
        schemaVersion: 1,
        emails: ["browser@devchievements.local"],
        githubLogins: { me: ["browser@devchievements.local"] },
        onboardingCompleted: true,
      },
      null,
      2,
    ),
  );

  const valid = {
    type: "devch:github_event",
    payload: {
      kind: "browser_github",
      event_id: "browser:github:o/r#abcdef0123456789abcdef0123456789abcdef01",
      repository_id: "github:o/r",
      activity: "commit",
      commit_sha: "abcdef0123456789abcdef0123456789abcdef01",
      occurred_ms: 1_700_000_000_000,
      author_login: "me",
    },
  };
  const bogus = {
    type: "devch:github_event",
    payload: { kind: "browser_github", event_id: "x", repository_id: "github:o/r" },
  };

  const input = Buffer.concat([frame(valid), frame(bogus)]);
  const run = spawnSync(hostExe, [], {
    input,
    env: { ...process.env, DEVCH_DATA_DIR: dataDir },
    timeout: 30000,
    windowsHide: true,
  });

  check("the host exited cleanly on EOF", run.status === 0, String(run.stderr));
  const replies = unframe(run.stdout ?? Buffer.alloc(0));
  check("it answered each message", replies.length === 2, JSON.stringify(replies));
  check("the valid report was stored", replies[0]?.stored === true, JSON.stringify(replies[0]));
  check("the invalid report was refused with a reason", replies[1]?.stored === false && !!replies[1]?.reason, JSON.stringify(replies[1]));

  const spoolDir = path.join(dataDir, "spool");
  const spoolFiles = fs.existsSync(spoolDir) ? fs.readdirSync(spoolDir).filter((f) => f.endsWith(".spool")) : [];
  check("a spool record was written", spoolFiles.length === 1, JSON.stringify(spoolFiles));

  if (spoolFiles.length === 1) {
    const body = fs.readFileSync(path.join(spoolDir, spoolFiles[0]), "utf8");
    check("it carries the repository", body.includes("github:o/r"), body.slice(0, 200));
    check("it is marked as a browser report", body.includes("browser_github"), body.slice(0, 200));
  }

  // The point of the host is not the spool file: it is that the Companion judges
  // what the browser saw. Drain it once and read the result back.
  const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
  if (fs.existsSync(companionExe)) {
    const drain = spawnSync(
      companionExe,
      [dataDir, "browser@devchievements.local", "--once"],
      { timeout: 60000, windowsHide: true, encoding: "utf8" },
    );
    check("the companion drained the browser record", drain.status === 0, String(drain.stdout) + String(drain.stderr));

    const db = path.join(dataDir, "companion.db");
    check("a database was written", fs.existsSync(db), db);

    // Ask the real diagnostics command what landed, rather than trusting the log.
    const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
    if (fs.existsSync(appExe)) {
      const diag = spawnSync(appExe, ["--diagnostics"], {
        timeout: 60000,
        windowsHide: true,
        encoding: "utf8",
        env: { ...process.env, LOCALAPPDATA: appData },
      });
      let snapshot = null;
      try {
        const parsed = JSON.parse(String(diag.stdout));
        snapshot = parsed.snapshot;
      } catch {
        // Reported by the check below.
      }
      check("the browser event reached the database", (snapshot?.events ?? 0) === 1, String(diag.stdout).slice(0, 300));
      check("it was counted as a commit", (snapshot?.repositories ?? []).some((r) => r.commits === 1), JSON.stringify(snapshot?.repositories));
    }
  } else {
    console.log("  SKIP the companion was not built");
  }

  console.log("");
  if (failures) {
    console.log("NATIVE HOST FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("NATIVE HOST PASSED: browser reports reach the spool, junk is refused");
  }
}

main();

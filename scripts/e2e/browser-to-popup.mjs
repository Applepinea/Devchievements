#!/usr/bin/env node
/**
 * scripts/e2e/browser-to-popup.mjs
 *
 * The last link of the Full Experience chain: a report from the browser must make the
 * DESKTOP app open its popup.
 *
 * Why end to end and not three unit tests: each piece is already covered in
 * isolation, and what has never been shown is that they compose. A break between
 * "the host wrote a spool record" and "the companion presented it" is invisible to
 * every part-wise test.
 *
 * The browser itself is not involved: the host is driven over a real pipe with the
 * browser's own framing, which is the same bytes the browser sends.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const hostExe = path.join(repoRoot, "target", "debug", "devchievements-native-host.exe");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".browser-popup");
const SHA = "abcdef0123456789abcdef0123456789abcdef01";

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(fn, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (fn()) return true;
    await sleep(150);
  }
  return false;
}

function frame(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  return Buffer.concat([header, body]);
}

async function main() {
  if (!fs.existsSync(hostExe) || !fs.existsSync(appExe)) {
    console.error("missing binaries; build them first");
    process.exit(1);
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify(
      {
        schemaVersion: 1,
        emails: ["browser@devchievements.local"],
        // Spec 7: the browser path attributes by login, and only an explicit
        // mapping makes a login the user's. Without this the report is refused.
        githubLogins: { me: ["browser@devchievements.local"] },
        onboardingCompleted: true,
        notifications: true,
      },
      null,
      2,
    ),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  // 1. The browser's report arrives at the host, exactly as the extension sends it.
  const message = {
    type: "devch:github_event",
    payload: {
      kind: "browser_github",
      event_id: "browser:github:o/r#" + SHA,
      repository_id: "github:o/r",
      activity: "commit",
      commit_sha: SHA,
      author_login: "me",
      occurred_ms: Date.now() - 2_000,
    },
  };
  const host = spawnSync(hostExe, [], {
    input: frame(message),
    env: { ...env, DEVCH_DATA_DIR: dataDir },
    timeout: 30000,
    windowsHide: true,
  });
  check("the host accepted the browser report", host.status === 0, String(host.stderr));

  // 2. The desktop app must judge it and open its popup without being told about it.
  const appLog = path.join(scratch, "app.log");
  const fd = fs.openSync(appLog, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });

  try {
    const started = await waitFor(
      () => fs.existsSync(appLog) && fs.readFileSync(appLog, "utf8").includes("TRAY READY"),
      30000,
    );
    check("the desktop app started", started, fs.readFileSync(appLog, "utf8").slice(-400));

    const presented = await waitFor(
      () => /POPUP shown/.test(fs.readFileSync(appLog, "utf8")),
      40000,
    );
    const log = fs.readFileSync(appLog, "utf8");
    check("the browser event reached the database", /INGESTED/.test(log), log.slice(-600));
    check("the desktop app opened a popup for it", presented, log.slice(-600));
    check("the popup was not a failed creation", !/POPUP FAILED/.test(log), log.slice(-600));
    check("no system notification was attempted", !/NOTIFY (shown|unavailable)/.test(log), log.slice(-600));
  } finally {
    app.kill();
    await new Promise((r) => app.on("exit", r));
    fs.closeSync(fd);
  }

  console.log("");
  if (failures) {
    console.log("BROWSER TO POPUP FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("BROWSER TO POPUP PASSED: a browser report becomes a desktop popup");
  }
}

main();

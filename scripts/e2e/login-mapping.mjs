#!/usr/bin/env node
/**
 * scripts/e2e/login-mapping.mjs
 *
 * Spec 7: the browser path attributes by GitHub login, and only an explicit mapping
 * makes a login the user's.
 *
 * The interesting assertion is the PAIR: the same report is refused before the
 * mapping exists and counted after it. A test that only checks the happy path would
 * pass even if the gate accepted everything.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const hostExe = path.join(repoRoot, "target", "debug", "devchievements-native-host.exe");
const scratch = path.join(__dirname, ".login-mapping");

const SHA_A = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

function sendReport(dataDir, sha, login) {
  const message = {
    type: "devch:github_event",
    payload: {
      kind: "browser_github",
      event_id: "browser:github:o/r#" + sha,
      repository_id: "github:o/r",
      activity: "commit",
      commit_sha: sha,
      occurred_ms: Date.now() - 1_000,
      author_login: login,
    },
  };
  return spawnSync(hostExe, [], {
    input: frame(message),
    env: { ...process.env, DEVCH_DATA_DIR: dataDir },
    timeout: 30000,
    windowsHide: true,
  });
}

async function runApp(env, logPath, until) {
  const fd = fs.openSync(logPath, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });
  try {
    await waitFor(
      () => fs.existsSync(logPath) && until(fs.readFileSync(logPath, "utf8")),
      35000,
    );
  } finally {
    app.kill();
    await new Promise((r) => app.on("exit", r));
    fs.closeSync(fd);
  }
  return fs.readFileSync(logPath, "utf8");
}

async function main() {
  if (!fs.existsSync(appExe) || !fs.existsSync(hostExe)) {
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
        emails: ["me@example.com"],
        // No githubLogins yet: this is the state before the user maps anything.
        onboardingCompleted: true,
      },
      null,
      2,
    ),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  // 1. A report from an account nobody mapped must be refused.
  sendReport(dataDir, SHA_A, "stranger");
  const refusedLog = await runApp(
    env,
    path.join(scratch, "app-1.log"),
    (text) => /REFUSED/.test(text) || /INGESTED/.test(text),
  );
  check("an unmapped login is refused", /REFUSED/.test(refusedLog), refusedLog.slice(-400));
  check("nothing was ingested from it", !/INGESTED/.test(refusedLog), refusedLog.slice(-400));

  // 2. Map the login, then send the same kind of report from that account.
  const mapped = spawnSync(appExe, ["--map-login", "me", "me@example.com"], {
    env,
    timeout: 30000,
    windowsHide: true,
    encoding: "utf8",
  });
  check("the mapping can be written", mapped.status === 0, String(mapped.stdout) + String(mapped.stderr));

  const stored = JSON.parse(fs.readFileSync(path.join(dataDir, "settings.json"), "utf8"));
  check("it is persisted in the settings file", !!stored.githubLogins?.me?.length, JSON.stringify(stored));

  sendReport(dataDir, SHA_B, "me");
  const acceptedLog = await runApp(
    env,
    path.join(scratch, "app-2.log"),
    (text) => /INGESTED/.test(text) || /REFUSED/.test(text),
  );
  check("the mapped login is accepted", /INGESTED/.test(acceptedLog), acceptedLog.slice(-400));
  check("and it is not refused", !/REFUSED/.test(acceptedLog), acceptedLog.slice(-400));

  // 3. The count must be exactly one: the refused report must not have been stored.
  const diag = spawnSync(appExe, ["--diagnostics"], {
    env,
    timeout: 30000,
    windowsHide: true,
    encoding: "utf8",
  });
  let snapshot = null;
  try {
    snapshot = JSON.parse(String(diag.stdout)).snapshot;
  } catch {
    // Reported by the check below.
  }
  check("only the accepted report is in the database", (snapshot?.events ?? 0) === 1, String(diag.stdout).slice(0, 300));

  console.log("");
  if (failures) {
    console.log("LOGIN MAPPING FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("LOGIN MAPPING PASSED: unmapped refused, mapped counted, nothing else stored");
  }
}

main();

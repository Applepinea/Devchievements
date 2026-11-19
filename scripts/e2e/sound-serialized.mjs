#!/usr/bin/env node
/**
 * scripts/e2e/sound-serialized.mjs
 *
 * Spec 16.6 says one sound plays per event and never several at once. `PlaySoundW` with
 * `SND_ASYNC` does not mix: it REPLACES whatever is playing. So when one drain delivers two
 * notifications back to back, the second call cut the first sound off mid-play -- the
 * reported "sometimes only the second half plays".
 *
 * Why end to end: the unit test proves the playback gate orders callers, and it would keep
 * passing if the notification path quietly stopped going through that gate. What is checked
 * here is that a real drain, from a real spool record to a real popup, is serialised: the
 * second sound starts no earlier than the first one could have finished.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const hostExe = path.join(repoRoot, "target", "debug", "devchievements-native-host.exe");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const soundsDir = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "sounds");
// The host only serves the origin the browser passes as argv[1].
const HOST_ORIGIN = "chrome-extension://gfgbmedanlpminnlbbdefojbgphoekbj/";
const scratch = path.join(__dirname, ".sound-serialized");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(fn, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (fn()) return true;
    await sleep(100);
  }
  return false;
}

function frame(message) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  return Buffer.concat([header, body]);
}

/** How long a WAV file is, read from its own header the way the app reads it. */
function wavDurationMs(file) {
  const wav = fs.readFileSync(file);
  let offset = 12;
  let byteRate = 0;
  while (offset + 8 <= wav.length) {
    const id = wav.toString("latin1", offset, offset + 4);
    const size = wav.readUInt32LE(offset + 4);
    const body = offset + 8;
    if (id === "fmt " && body + 16 <= wav.length) byteRate = wav.readUInt32LE(body + 8);
    if (id === "data") {
      const bytes = Math.min(size, wav.length - body);
      return byteRate === 0 ? null : Math.floor((bytes * 1000) / byteRate);
    }
    offset = body + size + (size % 2);
  }
  return null;
}

async function main() {
  for (const exe of [hostExe, appExe]) {
    if (!fs.existsSync(exe)) {
      console.error("missing binary: " + exe);
      process.exit(1);
    }
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
        // Spec 7: the browser path attributes by login, and only an explicit mapping
        // makes a login the user's. Without this both reports are refused.
        githubLogins: { me: ["browser@devchievements.local"] },
        onboardingCompleted: true,
        notifications: true,
        muted: false,
        volume: 80,
      },
      null,
      2,
    ),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  // Two commits, spooled before the app starts, so they are drained together: that is the
  // arrangement in which the second sound used to cut the first one off.
  const shas = [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  ];
  let accepted = 0;
  shas.forEach((sha, i) => {
    const host = spawnSync(
      hostExe,
      [HOST_ORIGIN],
      {
        input: frame({
          type: "devch:github_event",
          payload: {
            kind: "browser_github",
            event_id: "browser:github:o/r#" + sha,
            repository_id: "github:o/r",
            activity: "commit",
            commit_sha: sha,
            author_login: "me",
            occurred_ms: Date.now() - 2_000 - i * 1_000,
          },
        }),
        env: { ...env, DEVCH_DATA_DIR: dataDir },
        timeout: 30_000,
        windowsHide: true,
      },
    );
    if (host.status === 0) accepted++;
  });
  check("the host accepted both reports", accepted === 2, String(accepted));

  const appLog = path.join(scratch, "app.log");
  const fd = fs.openSync(appLog, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });
  const read = () => (fs.existsSync(appLog) ? fs.readFileSync(appLog, "utf8") : "");

  // Both popups have to be shown before the log is judged: a single popup cannot show a
  // gap, and a missing second one would otherwise look like "no overlap".
  await waitFor(() => (read().match(/POPUP shown/g) || []).length >= 2, 40_000);

  app.kill();
  await new Promise((r) => app.on("exit", r));
  fs.closeSync(fd);

  const log = read();
  const popups = (log.match(/POPUP shown/g) || []).length;
  check("the app started", /TRAY READY/.test(log), log.slice(-400));
  check("both events reached the database", /INGESTED 2/.test(log), log.slice(-400));
  check("both notifications were presented", popups >= 2, String(popups));

  const plays = [...log.matchAll(/SOUND play id=(\S+) .*?at=(\d+)/g)].map((m) => ({
    id: m[1],
    at: Number(m[2]),
  }));
  check("both presentations asked for a sound", plays.length >= 2, JSON.stringify(plays));

  check("the audio device accepted every buffer", !/PlaySoundW refused/.test(log), log.slice(-400));

  let overlapping = [];
  for (let i = 1; i < plays.length; i++) {
    const previous = plays[i - 1];
    const file = path.join(soundsDir, previous.id + ".wav");
    if (!fs.existsSync(file)) continue;
    const duration = wavDurationMs(file);
    if (duration === null) continue;
    const gap = plays[i].at - previous.at;
    // 100 ms of slack for the 60 ms tail margin and the clock the log is stamped with.
    if (gap < duration - 100) overlapping.push(`${previous.id} lasted ${duration} ms but ${plays[i].id} started ${gap} ms later`);
  }
  check(
    "no sound starts before the one before it could have finished",
    overlapping.length === 0,
    overlapping.join("; "),
  );

  console.log("");
  if (failures) {
    console.log("SOUND SERIALIZED FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("SOUND SERIALIZED PASSED: a second notification waits for the first sound to end");
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
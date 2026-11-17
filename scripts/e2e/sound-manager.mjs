#!/usr/bin/env node
/**
 * scripts/e2e/sound-manager.mjs
 *
 * Spec 19: a user can replace the sounds. The interesting parts are:
 *  - an imported sound becomes an available id;
 *  - an override pointing at it actually plays it;
 *  - a non-audio file is refused;
 *  - removing the file degrades to the built-in rather than going silent.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".sound-manager");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures += 1;
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

function run(args, env) {
  const r = spawnSync(appExe, args, { env, timeout: 40000, windowsHide: true, encoding: "utf8" });
  return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
}

async function runApp(env, logPath, action, until) {
  const fd = fs.openSync(logPath, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });
  try {
    const started = await waitFor(
      () => fs.existsSync(logPath) && fs.readFileSync(logPath, "utf8").includes("TRAY READY"),
      30000,
    );
    if (!started) return fs.readFileSync(logPath, "utf8");
    action();
    await waitFor(() => until(fs.readFileSync(logPath, "utf8")), 30000);
  } finally {
    app.kill();
    await new Promise((r) => app.on("exit", r));
    fs.closeSync(fd);
  }
  return fs.readFileSync(logPath, "utf8");
}

// A minimal but real WAV: RIFF/WAVE header plus a little silence.
function wavBytes() {
  const header = Buffer.alloc(44);
  header.write("RIFF", 0);
  header.writeUInt32LE(36 + 1600, 4);
  header.write("WAVE", 8);
  header.write("fmt ", 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(8000, 24);
  header.writeUInt32LE(16000, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36);
  header.writeUInt32LE(1600, 40);
  return Buffer.concat([header, Buffer.alloc(1600)]);
}

async function main() {
  if (!fs.existsSync(appExe) || !fs.existsSync(helperExe)) {
    console.error("missing binaries; build them first");
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(path.join(dataDir, "spool"), { recursive: true });
  const settingsPath = path.join(dataDir, "settings.json");
  fs.writeFileSync(settingsPath, JSON.stringify({ schemaVersion: 1, emails: ["sound@devchievements.local"], onboardingCompleted: true, volume: 100 }, null, 2));
  const env = { ...process.env, LOCALAPPDATA: appData };

  // 1. Import a sound.
  const source = path.join(scratch, "custom.wav");
  fs.writeFileSync(source, wavBytes());
  const imported = run(["--import-sound", source], env);
  check("a WAV imports", imported.status === 0, imported.out);

  const assetsDir = path.join(dataDir, "assets");
  check("it was stored", fs.existsSync(path.join(assetsDir, "custom.wav")), fs.readdirSync(assetsDir).join(", "));

  // 2. It becomes an available sound id.
  const diag = run(["--diagnostics"], env);
  let snapshot = null;
  try {
    snapshot = JSON.parse(diag.out);
  } catch {
    // Reported below.
  }
  check("the imported sound is available", (snapshot?.sounds ?? []).includes("custom"), JSON.stringify(snapshot?.sounds));

  // 3. An override pointing at it plays it.
  const configured = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  // The first commit unlocks "First Commit", which is rarity common and is not a
  // daily reward, so the rarity slot is the one that decides here.
  configured.soundOverrides = { "rarity:common": "custom" };
  configured.muted = false;
  fs.writeFileSync(settingsPath, JSON.stringify(configured, null, 2));

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  const git = (args) => {
    const log = path.join(scratch, "git.log");
    const fd = fs.openSync(log, "w");
    const r = spawnSync("git", args, { cwd: repo, stdio: ["ignore", fd, fd], windowsHide: true, env });
    fs.closeSync(fd);
    return r.status;
  };
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Sound Test"]);
  git(["config", "user.email", "sound@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

  const commit = (name) => {
    fs.writeFileSync(path.join(repo, name + ".txt"), name);
    git(["add", "-A"]);
    git(["commit", "-q", "-m", name]);
  };

  const played = await runApp(env, path.join(scratch, "app-1.log"), () => commit("a"), (t) => /SOUND play/.test(t));
  check("the imported sound played", /SOUND play id=custom/.test(played), played.slice(-400));

  // 4. A non-audio file is refused.
  const fake = path.join(scratch, "fake.wav");
  fs.writeFileSync(fake, Buffer.from("<html><script>alert(1)</script></html>"));
  const fakeRun = run(["--import-sound", fake], env);
  check("a renamed document is refused", fakeRun.status === 1, fakeRun.out);

  // 5. Removing the file degrades to the built-in, not to silence.
  fs.rmSync(path.join(assetsDir, "custom.wav"));
  const degraded = await runApp(env, path.join(scratch, "app-2.log"), () => commit("b"), (t) => /SOUND play/.test(t));
  check("it falls back to a built-in sound", /SOUND play id=achievement/.test(degraded), degraded.slice(-400));
  check("it does not go silent", !/SOUND suppressed/.test(degraded), degraded.slice(-400));

  console.log("");
  if (failures) {
    console.log("SOUND MANAGER FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("SOUND MANAGER PASSED: imported sounds play, junk is refused, missing files degrade");
  }
}

main();

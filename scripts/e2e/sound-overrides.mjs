#!/usr/bin/env node
/**
 * scripts/e2e/sound-overrides.mjs
 *
 * Spec 19: the Sound Manager's choices must actually change playback.
 *
 * The check that matters is the DIFFERENCE: the same unlock plays one file with no
 * override and another once the override is set. Asserting only that a sound played
 * would pass even if overrides were ignored.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".sound-overrides");

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

/**
 * Start the app, run an action while it is alive, then wait for the expected line.
 *
 * The action has to happen AFTER startup: the companion drains the spool on a timer,
 * so a commit made before it starts is picked up but never observed by this test.
 */
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
    await waitFor(
      () => until(fs.readFileSync(logPath, "utf8")),
      30000,
    );
  } finally {
    app.kill();
    await new Promise((r) => app.on("exit", r));
    fs.closeSync(fd);
  }
  return fs.readFileSync(logPath, "utf8");
}

function firstSoundId(log) {
  const match = log.match(/SOUND play id=([^ ]+)/);
  return match ? match[1] : null;
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
  const env = { ...process.env, LOCALAPPDATA: appData };

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  // The git process must see LOCALAPPDATA: the hook helper looks for the companion's
  // data directory and stays silent when it is absent (spec 34.1). Without this the
  // commits succeed but never reach the spool.
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

  fs.writeFileSync(
    settingsPath,
    JSON.stringify(
      {
        schemaVersion: 1,
        emails: ["sound@devchievements.local"],
        onboardingCompleted: true,
        volume: 100,
        muted: false,
      },
      null,
      2,
    ),
  );

  // 1. With no override, the reward sound comes from the built-in rule.
  const commit = (name) => {
    fs.writeFileSync(path.join(repo, name + ".txt"), name);
    git(["add", "-A"]);
    git(["commit", "-q", "-m", name]);
  };

  const plain = await runApp(
    env,
    path.join(scratch, "app-1.log"),
    () => commit("a"),
    (t) => /SOUND play/.test(t),
  );
  const plainSound = firstSoundId(plain);
  check("a sound played with no override", !!plainSound, plain.slice(-400));

  // 2. With an override, the same unlock must play the chosen file.
  const configured = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  // The second commit unlocks "Uncommon Day", whose id starts with `daily:`. That
  // makes it a daily-rank reward, and spec 19 puts daily-rank above rarity, so the
  // rarity slot would be shadowed. Target the slot that actually decides here.
  configured.soundOverrides = { "daily:uncommon": "commit" };
  fs.writeFileSync(settingsPath, JSON.stringify(configured, null, 2));

  const overridden = await runApp(
    env,
    path.join(scratch, "app-2.log"),
    () => commit("b"),
    (t) => /SOUND play/.test(t),
  );
  const overriddenSound = firstSoundId(overridden);
  check("a sound played with an override", !!overriddenSound, overridden.slice(-400));
  check("the override differs from the default", overriddenSound !== plainSound, JSON.stringify({ plainSound, overriddenSound }));
  check("the override is the configured file", overriddenSound === "commit", String(overriddenSound));

  // 3. Muting still wins over any override.
  configured.muted = true;
  fs.writeFileSync(settingsPath, JSON.stringify(configured, null, 2));

  const muted = await runApp(
    env,
    path.join(scratch, "app-3.log"),
    () => commit("c"),
    (t) => /SOUND suppressed/.test(t),
  );
  check("muting suppresses playback despite the override", /SOUND suppressed muted=true/.test(muted), muted.slice(-400));

  console.log("");
  if (failures) {
    console.log("SOUND OVERRIDES FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("SOUND OVERRIDES PASSED: the chosen file plays, and muting still wins");
  }
}

main();

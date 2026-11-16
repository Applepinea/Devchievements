#!/usr/bin/env node
/**
 * scripts/e2e/window.mjs
 *
 * Verifies the WebView2 window can actually be created by the real binary.
 *
 * This exists because the product was tray-only for a while and the window creation
 * path was never exercised, which is how a WebView2 profile-directory problem stayed
 * hidden behind a bare 'os error 5'.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".window");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(dataRoot, { recursive: true });
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const log = path.join(scratch, "window.log");
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, ["--window-smoke"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env,
    timeout: 60000,
  });
  fs.closeSync(fd);
  const out = fs.readFileSync(log, "utf8");

  check("the smoke command exited cleanly", r.status === 0, out);
  check("the window was created", /WINDOW OK/.test(out), out);
  check("it carries the expected title", /title="Devchievements"/.test(out), out);

  // The profile directory must exist afterwards: its absence is what made creation
  // fail in the first place, so this is the regression guard.
  const profile = path.join(dataRoot, "devchievements", "webview");
  check("the WebView2 profile directory was created", fs.existsSync(profile), profile);

  console.log("");
  if (failures) {
    console.log("WINDOW FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("WINDOW PASSED: the WebView2 window opens");
  }
}

main();

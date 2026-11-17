#!/usr/bin/env node
/**
 * scripts/e2e/dependencies.mjs
 *
 * Spec 25: a first run must say what the machine is missing and where to get it.
 *
 * What this proves: the checks run, they are reported for both prerequisites, and
 * the window renders its preflight panel. What it cannot prove: that a machine
 * WITHOUT the prerequisite gets the right advice -- the test machine has both, and
 * uninstalling Git to find out is not something a test may do. That half is
 * carried by unit tests on the report shape and by the user's VM run.
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".dependencies");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * Run a GUI launch with the runtime pretended to be absent.
 *
 * The prompt is modal, so the process is killed after the log line appears. The
 * point is not to test the message box (that cannot be automated) but to prove the
 * guard runs before any window is created, and that `--open-window` does not sneak
 * past it -- which is exactly how a machine without the runtime failed with no
 * explanation.
 */
function missingRuntimeLaunch(dataRoot, logName) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot, DEVCH_FORCE_NO_WEBVIEW2: "1" },
  });
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const text = fs.readFileSync(log, "utf8");
    if (text.includes("WEBVIEW2 missing")) break;
    sleep(100);
  }
  try {
    child.kill();
  } catch {}
  fs.closeSync(fd);
  return fs.readFileSync(log, "utf8");
}

function openWindow(dataRoot, logName) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, ["--window-smoke", "--page", "welcome", "--step", "checks"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot },
    timeout: 60000,
  });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(path.join(dataRoot, "devchievements"), { recursive: true });

  const { status, out } = openWindow(dataRoot, "checks.log");

  check("the preflight step renders", out.includes("view=welcome-checks"), out);
  check("it did not crash", status === 0, out);
  check("WebView2 is reported", /PREFLIGHT webview2 ok=/.test(out), out);
  check("Git is reported", /PREFLIGHT git ok=/.test(out), out);
  check(
    "both prerequisites are present on this machine",
    /PREFLIGHT webview2 ok=true/.test(out) && /PREFLIGHT git ok=true/.test(out),
    out,
  );

  // A GUI launch must stop and explain itself when the runtime is absent, even with
  // the launcher's --open-window flag. Treating that flag as "command mode" skipped
  // the guard and produced a raw failure instead.
  const missing = missingRuntimeLaunch(dataRoot, "missing.log");
  check("a missing runtime is reported before any window", /WEBVIEW2 missing/.test(missing), missing);
  check("no window was created first", !/WINDOW created|WINDOW opened/.test(missing), missing);

  console.log("");
  if (failures) {
    console.log("DEPENDENCIES FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("DEPENDENCIES PASSED: the preflight step checks and reports both prerequisites");
  }
}

main();

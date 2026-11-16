#!/usr/bin/env node
/**
 * scripts/e2e/pages.mjs
 *
 * Opens each page inside the real window and waits for it to report that it rendered.
 *
 * A page that throws (bad IPC, missing field, wrong type) never reaches the report,
 * so this catches the failure mode a build alone cannot: code that compiles but
 * explodes at runtime inside the webview.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".pages");

const PAGES = ["dashboard", "gallery", "activity", "repositories", "settings", "about"];

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function openPage(page, dataRoot) {
  const log = path.join(scratch, page + ".log");
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, ["--window-smoke", "--page", page], {
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
  // The data directory exists after install; the helper now refuses to

  // create it, so fixtures mirror an installed machine (spec 34.1).

  fs.mkdirSync(path.join(dataRoot, "devchievements"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify(
      { schemaVersion: 1, emails: ["pages@devchievements.local"], onboardingCompleted: true },
      null,
      2,
    )
  );

  for (const page of PAGES) {
    const { status, out } = openPage(page, dataRoot);
    check("the " + page + " page rendered", /UI READY view=/.test(out), out);
    check("it reported itself as " + page, out.includes("view=" + page), out);
    check("it did not crash", status === 0 && /WINDOW OK/.test(out), out);
  }

  console.log("");
  if (failures) {
    console.log("PAGES FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("PAGES PASSED: every page rendered from the real database");
  }
}

main();

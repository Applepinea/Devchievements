#!/usr/bin/env node
/**
 * scripts/e2e/config-io.mjs
 *
 * Spec 26.4 / Phase 4a: the whole configuration can be exported and imported.
 *
 * The assertions that matter:
 *  - an export is a document a human can read;
 *  - an import actually changes the app's behaviour, not just the file;
 *  - a configuration that fails validation leaves the PREVIOUS one intact.
 *
 * The last one is the point. An import that applies half a broken file is worse than
 * one that refuses outright.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".config-io");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function run(args, env) {
  const r = spawnSync(appExe, args, {
    env,
    timeout: 40000,
    windowsHide: true,
    encoding: "utf8",
  });
  return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
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
        githubLogins: { me: ["me@example.com"] },
        onboardingCompleted: true,
        volume: 80,
      },
      null,
      2,
    ),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  // 1. Export.
  const exported = path.join(scratch, "export.json");
  const exportRun = run(["--export-config", exported], env);
  check("the configuration exports", exportRun.status === 0 && fs.existsSync(exported), exportRun.out);

  let document = null;
  try {
    document = JSON.parse(fs.readFileSync(exported, "utf8"));
  } catch {
    // Reported below.
  }
  check("it is valid JSON", !!document, fs.readFileSync(exported, "utf8").slice(0, 200));
  check("it carries the identity", JSON.stringify(document?.emails) === '["me@example.com"]', JSON.stringify(document?.emails));
  check("it carries the login mapping", !!document?.githubLogins?.me?.length, JSON.stringify(document?.githubLogins));
  check("it declares a schema version", document?.schemaVersion === 1, String(document?.schemaVersion));
  check("it carries the popup style", !!document?.popup, JSON.stringify(document?.popup));

  // 2. Edit and import: the change must be observable through diagnostics.
  document.volume = 33;
  document.privacy = "privacy";
  document.popup = { ...document.popup, width: 500, corner: "top_left" };
  const edited = path.join(scratch, "edited.json");
  fs.writeFileSync(edited, JSON.stringify(document, null, 2));

  const importRun = run(["--import-config", edited], env);
  check("the edited configuration imports", importRun.status === 0, importRun.out);

  const diag = run(["--diagnostics"], env);
  let snapshot = null;
  try {
    snapshot = JSON.parse(diag.out);
  } catch {
    // Reported below.
  }
  check("the privacy change took effect", snapshot?.privacy === "privacy", String(snapshot?.privacy));
  check("the popup width took effect", snapshot?.popup?.width === 500, JSON.stringify(snapshot?.popup));
  check("the popup corner took effect", snapshot?.popup?.corner === "top_left", JSON.stringify(snapshot?.popup?.corner));

  const stored = JSON.parse(fs.readFileSync(path.join(dataDir, "settings.json"), "utf8"));
  check("the volume was written", stored.volume === 33, String(stored.volume));

  // 3. An invalid document must change nothing.
  const broken = path.join(scratch, "broken.json");
  fs.writeFileSync(broken, '{ "schemaVersion": 99, "emails": [] }');
  const brokenRun = run(["--import-config", broken], env);
  check("a future schema is refused", brokenRun.status === 1, brokenRun.out);
  check("the refusal explains itself", /schema 99 is not supported/.test(brokenRun.out), brokenRun.out);

  const after = JSON.parse(fs.readFileSync(path.join(dataDir, "settings.json"), "utf8"));
  check("the previous configuration is intact", after.volume === 33, String(after.volume));
  check("the identity did not change", JSON.stringify(after.emails) === '["me@example.com"]', JSON.stringify(after.emails));

  // 4. A document with an out-of-range value is clamped, not rejected.
  const clamped = path.join(scratch, "clamped.json");
  const withHuge = JSON.parse(fs.readFileSync(edited, "utf8"));
  withHuge.volume = 300;
  withHuge.popup.width = 5000;
  fs.writeFileSync(clamped, JSON.stringify(withHuge, null, 2));
  const clampRun = run(["--import-config", clamped], env);
  check("an out-of-range document still imports", clampRun.status === 0, clampRun.out);

  const clampDiag = run(["--diagnostics"], env);
  let clampSnapshot = null;
  try {
    clampSnapshot = JSON.parse(clampDiag.out);
  } catch {
    // Reported below.
  }
  check("the width was clamped into range", clampSnapshot?.popup?.width === 600, String(clampSnapshot?.popup?.width));
  const clampStored = JSON.parse(fs.readFileSync(path.join(dataDir, "settings.json"), "utf8"));
  check("the volume was clamped", clampStored.volume === 100, String(clampStored.volume));

  console.log("");
  if (failures) {
    console.log("CONFIG IO FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("CONFIG IO PASSED: export, edit, import, refuse and clamp all behave");
  }
}

main();

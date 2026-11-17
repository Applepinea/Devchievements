#!/usr/bin/env node
/**
 * scripts/e2e/filters.mjs
 *
 * Spec 15: user filters change the user-filtered statistics profile, and spec 13.3:
 * they must NOT change a built-in achievement's outcome.
 *
 * That second half is the reason the filters crate exists, so it is what this test
 * exercises: excluding a repository from the dashboard must leave the official
 * commit count untouched.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".filters");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures += 1;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
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
  const settingsPath = path.join(dataDir, "settings.json");
  fs.writeFileSync(
    settingsPath,
    JSON.stringify({ schemaVersion: 1, emails: ["me@example.com"], onboardingCompleted: true }, null, 2),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  const exported = path.join(scratch, "base.json");
  check("the baseline exports", run(["--export-config", exported], env).status === 0);
  const document = JSON.parse(fs.readFileSync(exported, "utf8"));
  check("the filters are part of the configuration", !!document.filters, JSON.stringify(document.filters));

  // Exclude merges and imported history; the change must be visible in diagnostics.
  document.filters = {
    ...document.filters,
    include_merges: false,
    include_imported: false,
  };
  const edited = path.join(scratch, "edited.json");
  fs.writeFileSync(edited, JSON.stringify(document, null, 2));
  check("the edited configuration imports", run(["--import-config", edited], env).status === 0);

  const diag = run(["--diagnostics"], env);
  let snapshot = null;
  try {
    snapshot = JSON.parse(diag.out);
  } catch {
    // Reported by the check below.
  }
  check("the import took effect", snapshot?.filters?.include_imported === false, JSON.stringify(snapshot?.filters));

  // Excluding a repository must not change the OFFICIAL count (spec 13.3).
  const officialBefore = snapshot?.snapshot?.repositories ?? [];
  document.filters = {
    ...document.filters,
    repositories: { mode: "except", ids: ["github:acme/secret"] },
  };
  fs.writeFileSync(edited, JSON.stringify(document, null, 2));
  check("a repository exclusion imports", run(["--import-config", edited], env).status === 0);

  const after = run(["--diagnostics"], env);
  let afterSnapshot = null;
  try {
    afterSnapshot = JSON.parse(after.out);
  } catch {
    // Reported by the check below.
  }
  check(
    "the official repository list is unchanged",
    JSON.stringify(afterSnapshot?.snapshot?.repositories) === JSON.stringify(officialBefore),
    JSON.stringify({ officialBefore, after: afterSnapshot?.snapshot?.repositories }),
  );
  check(
    "the exclusion is recorded in the filters",
    JSON.stringify(afterSnapshot?.filters?.repositories ?? {}).includes("github:acme/secret"),
    JSON.stringify(afterSnapshot?.filters?.repositories),
  );

  // A malformed filter must be refused, not half-applied.
  const broken = path.join(scratch, "broken.json");
  const brokenDoc = JSON.parse(JSON.stringify(document));
  brokenDoc.filters = { ...brokenDoc.filters, repositories: { mode: "nonsense", ids: [] } };
  fs.writeFileSync(broken, JSON.stringify(brokenDoc, null, 2));
  const brokenRun = run(["--import-config", broken], env);
  check("an unknown repository mode is refused", brokenRun.status === 1, brokenRun.out);

  const stillThere = run(["--diagnostics"], env);
  let still = null;
  try {
    still = JSON.parse(stillThere.out);
  } catch {
    // Reported by the check below.
  }
  check(
    "the previous filters survived the refusal",
    JSON.stringify(still?.filters?.repositories ?? {}).includes("github:acme/secret"),
    JSON.stringify(still?.filters),
  );

  console.log("");
  if (failures) {
    console.log("FILTERS FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("FILTERS PASSED: filters persist and never touch official counts");
  }
}

main();

#!/usr/bin/env node
/**
 * scripts/e2e/achievement-builder.mjs
 *
 * Spec 22: a user achievement is JSON, validated before it is stored.
 *
 * The assertions that matter:
 *  - a valid definition is stored and survives an export/import round trip;
 *  - a definition that shadows a built-in is refused (spec 13.3);
 *  - a rule that breaks the complexity limits is refused;
 *  - a refused definition leaves nothing behind.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".achievement-builder");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
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

// The wire shape is flat: serde's internally-tagged enum inlines a newtype
// variant's fields. A nested { Condition: {...} } object is a silent trap that
// produces "missing field metric".
function rule(metric, atLeast) {
  return { op: "condition", metric, at_least: atLeast };
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
    JSON.stringify(
      {
        schemaVersion: 1,
        emails: ["me@example.com"],
        onboardingCompleted: true,
      },
      null,
      2,
    ),
  );
  const env = { ...process.env, LOCALAPPDATA: appData };

  const writeDefinition = (name, body) => {
    const file = path.join(scratch, name + ".json");
    fs.writeFileSync(file, JSON.stringify(body, null, 2));
    return file;
  };

  // 1. A valid definition is accepted and stored.
  const good = writeDefinition("good", {
    id: "ten-commits",
    name: "Ten Commits",
    description: "Land ten valid commits.",
    rule: rule("commits", 10),
  });
  const accepted = run(["--add-achievement", good], env);
  check("a valid definition is accepted", accepted.status === 0, accepted.out);
  const stored = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  check("it is stored", (stored.userAchievements ?? []).length === 1, JSON.stringify(stored.userAchievements));
  check("its id is preserved", stored.userAchievements?.[0]?.id === "ten-commits", String(stored.userAchievements?.[0]?.id));

  // 2. It survives an export and re-import.
  const exported = path.join(scratch, "export.json");
  const exportRun = run(["--export-config", exported], env);
  check("the configuration exports", exportRun.status === 0, exportRun.out);
  const document = JSON.parse(fs.readFileSync(exported, "utf8"));
  check("the user achievement is in the export", (document.userAchievements ?? []).length === 1, JSON.stringify(document.userAchievements));

  const importRun = run(["--import-config", exported], env);
  check("it imports back", importRun.status === 0, importRun.out);

  // 3. A rule that shadows a built-in is refused.
  const shadow = writeDefinition("shadow", {
    id: "first_commit",
    name: "Redefined",
    description: "",
    rule: rule("commits", 1),
  });
  const shadowRun = run(["--add-achievement", shadow], env);
  check("shadowing a built-in is refused", shadowRun.status === 1, shadowRun.out);
  check("the refusal says why", /built-in achievement/.test(shadowRun.out), shadowRun.out);

  // 4. A duplicate user id is refused.
  const duplicate = writeDefinition("duplicate", {
    id: "ten-commits",
    name: "Again",
    description: "",
    rule: rule("commits", 5),
  });
  const duplicateRun = run(["--add-achievement", duplicate], env);
  check("a duplicate id is refused", duplicateRun.status === 1, duplicateRun.out);

  // 5. A rule that is too complex is refused.
  let deep = rule("commits", 1);
  for (let i = 0; i < 12; i += 1) deep = { op: "all", children: [deep] };
  const complex = writeDefinition("complex", {
    id: "too-deep",
    name: "Too Deep",
    description: "",
    rule: deep,
  });
  const complexRun = run(["--add-achievement", complex], env);
  check("an over-complex rule is refused", complexRun.status === 1, complexRun.out);
  check("the refusal says why", /too complex/.test(complexRun.out), complexRun.out);

  // 6. Refusals left nothing behind.
  const after = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  check("no refused definition was stored", (after.userAchievements ?? []).length === 1, JSON.stringify(after.userAchievements));

  console.log("");
  if (failures) {
    console.log("ACHIEVEMENT BUILDER FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("ACHIEVEMENT BUILDER PASSED: valid rules persist, invalid ones are refused");
  }
}

main();

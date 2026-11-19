#!/usr/bin/env node
/**
 * scripts/e2e/theme-manager.mjs
 *
 * Spec 23: theme packages are untrusted input, and the settings page can switch,
 * import, export and reset them.
 *
 * The assertions that matter, in the order a user meets them:
 *  - the built-in theme always exists and is the active one on a fresh install;
 *  - importing a valid package stores it and applying it changes the popup style;
 *  - a package containing code is refused and leaves nothing behind;
 *  - the style survives a restart (it is written to settings.json, not held in memory);
 *  - resetting goes back to the default;
 *  - exporting round-trips: what was exported re-imports under the same name.
 *
 * # Why the real binary
 *
 * The validation lives in `crates/themes`, but the wiring from a dialog path through
 * `settings.json` into the popup is only real in the app. A unit test would pass while
 * the page's button called a command that was never registered.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".theme-manager");

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

/** A popup style that is legal but visibly not the default. */
function style(overrides = {}) {
  return {
    width: 480,
    height: null,
    scalePercent: 100,
    iconSize: 64,
    cornerRadius: 14,
    screenMargin: 24,
    corner: "top_left",
    opacityPercent: 88,
    displaySeconds: 7,
    animationMs: 320,
    backgroundBlur: 8,
    ...overrides,
  };
}

function packageJson(name, popup) {
  return { schemaVersion: 1, name, popup };
}

function writePackage(name, body) {
  const file = path.join(scratch, name + ".json");
  fs.writeFileSync(file, typeof body === "string" ? body : JSON.stringify(body, null, 2));
  return file;
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
  const themesPath = path.join(dataDir, "themes.json");
  const env = { ...process.env, LOCALAPPDATA: appData };

  const settings = () => JSON.parse(fs.readFileSync(settingsPath, "utf8"));
  const readThemes = () => (fs.existsSync(themesPath) ? JSON.parse(fs.readFileSync(themesPath, "utf8")) : null);

  // 1. A fresh install has exactly one theme, and it is the built-in.
  const list0 = run(["--list-themes"], env);
  check("the theme list can be read", list0.status === 0, list0.out);
  check("the built-in theme exists", /THEME name=Default builtin=yes/.test(list0.out), list0.out);
  check("it is the active one on a fresh install", /THEME ACTIVE name=Default/.test(list0.out), list0.out);
  check("no user theme is stored yet", readThemes() === null || (readThemes().themes ?? []).length === 0, JSON.stringify(readThemes()));

  // 2. A valid package imports and is remembered.
  const midnight = writePackage("midnight", packageJson("Midnight", style()));
  const imported = run(["--import-theme", midnight], env);
  check("a valid theme imports", imported.status === 0, imported.out);
  check("it is stored under its own name", /THEME OK name=Midnight/.test(imported.out), imported.out);
  check("the themes file now holds it", (readThemes()?.themes ?? []).some((t) => t.name === "Midnight"), JSON.stringify(readThemes()));

  // 3. Applying it changes the style in effect, in the settings document.
  const applied = run(["--apply-theme", "Midnight"], env);
  check("the theme applies", applied.status === 0, applied.out);
  check("the popup style changed", settings().popup?.width === 480, JSON.stringify(settings().popup));
  check("the corner came with it", settings().popup?.corner === "top_left", JSON.stringify(settings().popup));
  const list1 = run(["--list-themes"], env);
  check("the list reports it as active", /THEME ACTIVE name=Midnight/.test(list1.out), list1.out);

  // 4. The style survives a restart: this run reads the file, it does not remember.
  const list2 = run(["--list-themes"], env);
  check("a later process still sees it active", /THEME ACTIVE name=Midnight/.test(list2.out), list2.out);

  // 5. A package that carries code is refused, and nothing is left behind.
  const before = (readThemes()?.themes ?? []).length;
  const evil = writePackage(
    "evil",
    JSON.stringify({
      schemaVersion: 1,
      name: "Evil",
      popup: style(),
      script: "alert(1)",
    }),
  );
  const refused = run(["--import-theme", evil], env);
  check("a package with a script is refused", refused.status === 1, refused.out);
  check("the refusal names the problem", /code|script/i.test(refused.out), refused.out);
  check("nothing was stored by the refused import", (readThemes()?.themes ?? []).length === before, JSON.stringify(readThemes()));

  // 6. A stylesheet is refused too: this is the other half of "no executable payload".
  const styled = writePackage(
    "styled",
    JSON.stringify({ schemaVersion: 1, name: "Styled", popup: style(), css: "body{display:none}" }),
  );
  const styledRun = run(["--import-theme", styled], env);
  check("a package with a stylesheet is refused", styledRun.status === 1, styledRun.out);

  // 7. Out-of-range values are clamped rather than trusted.
  const wide = writePackage("wide", packageJson("Wide", style({ width: 5000, corner: "bottom_right" })));
  const wideRun = run(["--import-theme", wide], env);
  check("an out-of-range theme is accepted but clamped", wideRun.status === 0, wideRun.out);
  const wideStored = (readThemes()?.themes ?? []).find((t) => t.name === "Wide");
  check("the stored width is inside the limit", wideStored?.popup?.width <= 600, JSON.stringify(wideStored?.popup));

  // 8. Export round-trips: the same package re-imports under the same name.
  const exported = path.join(scratch, "exported.json");
  const exportRun = run(["--export-theme", exported], env);
  check("the style in effect exports", exportRun.status === 0, exportRun.out);
  const document = JSON.parse(fs.readFileSync(exported, "utf8"));
  check("the export carries the active theme's name", document.name === "Midnight", String(document.name));
  check("the export carries the style", document.popup?.width === 480, JSON.stringify(document.popup));

  const reimportRun = run(["--import-theme", exported], env);
  check("it imports back", reimportRun.status === 0, reimportRun.out);
  check("under the same name, so it replaces rather than piles up",
    (readThemes()?.themes ?? []).filter((t) => t.name === "Midnight").length === 1,
    JSON.stringify(readThemes()?.themes?.map((t) => t.name)));

  // 9. Deleting a stored theme is refused for the built-in and works for the rest.
  const deleteBuiltin = run(["--remove-theme", "Default"], env);
  check("the built-in cannot be deleted", deleteBuiltin.status !== 0, deleteBuiltin.out);
  const deleteWide = run(["--remove-theme", "Wide"], env);
  check("a stored theme can be deleted", deleteWide.status === 0, deleteWide.out);
  check("it is gone from the file", !(readThemes()?.themes ?? []).some((t) => t.name === "Wide"), JSON.stringify(readThemes()));

  // 10. Reset puts the default style back.
  const reset = run(["--reset-theme"], env);
  check("the default can be restored", reset.status === 0, reset.out);
  check("the width is the default again", settings().popup?.width === 364, JSON.stringify(settings().popup));
  const list3 = run(["--list-themes"], env);
  check("the list reports the default as active", /THEME ACTIVE name=Default/.test(list3.out), list3.out);

  // 11. The imported theme is still there after a reset: resetting the STYLE must not
  //     delete the user's packages.
  check("the imported package survived the reset",
    (readThemes()?.themes ?? []).some((t) => t.name === "Midnight"),
    JSON.stringify(readThemes()?.themes?.map((t) => t.name)));

  console.log("");
  if (failures) {
    console.log("THEME MANAGER FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("THEME MANAGER PASSED: import/apply/export/reset all work, code-bearing packages are refused");
  }
}

main();

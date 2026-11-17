#!/usr/bin/env node
/**
 * scripts/e2e/asset-import.mjs
 *
 * Spec 20: imported assets are validated by their bytes, not their names.
 *
 * The import path is exercised through the same command the UI calls, without the
 * file dialog: the dialog only produces a path, and a path is not the risky part.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".asset-import");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures += 1;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function run(args, env) {
  const r = spawnSync(appExe, args, { env, timeout: 40000, windowsHide: true, encoding: "utf8" });
  return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
}

const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0, 0, 0, 0]);
const SVG_OK = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><circle r="4"/></svg>');
const SVG_BAD = Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>');
const HTML = Buffer.from('<html><script>alert(1)</script></html>');

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(path.join(dataDir, "settings.json"), JSON.stringify({ schemaVersion: 1, emails: [], onboardingCompleted: true }, null, 2));
  const env = { ...process.env, LOCALAPPDATA: appData };
  const assetsDir = path.join(dataDir, "assets");

  const write = (name, bytes) => {
    const file = path.join(scratch, name);
    fs.writeFileSync(file, bytes);
    return file;
  };

  // 1. A real PNG is accepted.
  const png = write("icon.png", PNG);
  const okRun = run(["--import-asset", png], env);
  check("a real PNG is accepted", okRun.status === 0, okRun.out);
  check("it was stored", fs.existsSync(path.join(assetsDir, "icon.png")), assetsDir);

  // 2. A script document renamed to .png is refused.
  const fake = write("fake.png", HTML);
  const fakeRun = run(["--import-asset", fake], env);
  check("a renamed script document is refused", fakeRun.status === 1, fakeRun.out);
  check("the refusal says why", /not a recognised/.test(fakeRun.out), fakeRun.out);
  check(
    "it was not stored",
    !fs.existsSync(path.join(assetsDir, "fake.png")),
    fs.readdirSync(assetsDir).join(", "),
  );

  // 3. A safe SVG is accepted; one carrying a script is not.
  const safeSvg = write("badge.svg", SVG_OK);
  check("a plain SVG is accepted", run(["--import-asset", safeSvg], env).status === 0);
  const unsafeSvg = write("evil.svg", SVG_BAD);
  const evilRun = run(["--import-asset", unsafeSvg], env);
  check("an SVG containing a script is refused", evilRun.status === 1, evilRun.out);
  check("the refusal names the construct", /script/.test(evilRun.out), evilRun.out);

  // 4. The stored set is what survived.
  const stored = fs.readdirSync(assetsDir).sort();
  check("only the accepted files are present", stored.join(",") === "badge.svg,icon.png", stored.join(","));

  console.log("");
  if (failures) {
    console.log("ASSET IMPORT FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("ASSET IMPORT PASSED: real files are stored, disguised ones are refused");
  }
}

main();

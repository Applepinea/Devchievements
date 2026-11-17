#!/usr/bin/env node
/**
 * scripts/e2e/diagnostics-export.mjs
 *
 * Spec 26.2: a diagnostics export is redacted by default, the raw form exists only
 * as an explicit choice, and the file says what it contains -- because a file gets
 * forwarded without the sentence that explained it.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".diagnostics-export");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function exportTo(dataRoot, target, raw) {
  const args = ["--export-diagnostics", target];
  if (raw) args.push("--raw");
  const log = path.join(scratch, path.basename(target) + ".log");
  const fd = fs.openSync(log, "w");
  const r = spawnSync(appExe, args, {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: dataRoot },
    timeout: 60000,
  });
  fs.closeSync(fd);
  const out = fs.readFileSync(log, "utf8");
  const text = fs.existsSync(target) ? fs.readFileSync(target, "utf8") : "";
  return { status: r.status, out, text };
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

  const redactedPath = path.join(scratch, "redacted.json");
  const redacted = exportTo(dataRoot, redactedPath, false);
  check("the redacted export succeeds", redacted.status === 0, redacted.out);
  check("it announces itself on stdout", /DIAGNOSTICS EXPORTED/.test(redacted.out), redacted.out);
  check("the file says it is redacted", /"redacted": true/.test(redacted.text), redacted.text.slice(0, 200));
  check(
    "the redacted file carries no warning",
    !/"warning"/.test(redacted.text),
    redacted.text.slice(0, 200),
  );
  check(
    "the redacted file masks the user name in the data path",
    !redacted.text.includes("csah1350"),
    redacted.text.slice(0, 400),
  );

  const rawPath = path.join(scratch, "raw.json");
  const raw = exportTo(dataRoot, rawPath, true);
  check("the raw export succeeds", raw.status === 0, raw.out);
  check("it is labelled raw", /raw=true/.test(raw.out), raw.out);
  check("the raw file warns in its own content", /"warning"/.test(raw.text), raw.text.slice(0, 200));
  check(
    "the warning names what makes it unsafe",
    /not safe to share/.test(raw.text),
    raw.text.slice(0, 400),
  );

  console.log("");
  if (failures) {
    console.log("DIAGNOSTICS EXPORT FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("DIAGNOSTICS EXPORT PASSED: redacted by default, raw only on request and labelled");
  }
}

main();

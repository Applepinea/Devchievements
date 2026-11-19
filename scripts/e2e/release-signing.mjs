#!/usr/bin/env node
/**
 * scripts/e2e/release-signing.mjs
 *
 * Spec 40.3: a release build must sign with a key from CI secrets, not with the
 * development key that ships in this repository.
 *
 * # The bug this exists for
 *
 * `scripts/dev/build-installer.bat` used to set `TAURI_SIGNING_PRIVATE_KEY`
 * unconditionally, overwriting whatever CI provided, and defaulted the password on
 * every path including a would-be release. A release built that way produces a `.sig`
 * that anyone with the repository can reproduce, so the signature proves nothing.
 * The comment above those lines claimed the opposite, which is how it survived review.
 *
 * # Why a harness and not a unit test
 *
 * The guard lives in a batch file. Its failure mode is a subprocess exit code, and it
 * was measured to report 0 from inside a parenthesised `if` block -- a real bug that
 * only a real invocation catches.
 *
 * The script is not run to completion: bundling takes minutes and needs the MSVC
 * toolchain. Each case runs a truncated copy that stops right after the signing
 * decision, so what is asserted is exactly the decision, not the packaging that
 * follows it.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const installer = path.join(repoRoot, "scripts", "dev", "build-installer.bat");
const scratch = path.join(__dirname, ".release-signing");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

/**
 * Build a probe from the real script: everything up to the end of the signing block,
 * then print the variables and exit.
 *
 * Deriving the probe from the real file (rather than restating the logic) is the point:
 * if someone edits the script, the probe moves with it and the assertions still apply.
 */
function probe(name) {
  const lines = fs.readFileSync(installer, "utf8").split(/\r?\n/);
  const end = lines.indexOf(":signing_done");
  if (end < 0) {
    throw new Error("build-installer.bat no longer has a :signing_done label");
  }
  const head = lines.slice(0, end + 1);
  head.push('echo PROBE_KEY=%TAURI_SIGNING_PRIVATE_KEY%');
  head.push('echo PROBE_PASSWORD=%TAURI_SIGNING_PRIVATE_KEY_PASSWORD%');
  head.push("exit /b 0");

  const file = path.join(scratch, name + ".bat");
  // ASCII, CRLF: cmd.exe on a GBK code page mis-reads UTF-8, and the repository rule
  // for .bat/.ps1 is the same one that governs scripts/env.ps1.
  fs.writeFileSync(file, head.join("\r\n") + "\r\n", "ascii");
  return file;
}

/** Run a probe with a controlled environment. */
function run(file, overrides) {
  // A clean environment would lose SystemRoot/PATH and cmd.exe would not start, so
  // the signing variables are explicitly removed instead of building from nothing.
  const env = { ...process.env };
  for (const key of [
    "DEVCH_RELEASE",
    "TAURI_SIGNING_PRIVATE_KEY",
    "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  ]) {
    delete env[key];
  }
  Object.assign(env, overrides);
  const r = spawnSync(process.env.ComSpec || "cmd.exe", ["/c", file], {
    env,
    windowsHide: true,
    encoding: "utf8",
    timeout: 60000,
  });
  return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
}

function main() {
  if (!fs.existsSync(installer)) {
    console.error("missing installer script: " + installer);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  // 1. Local development: the checked-in key is used, and the script says so loudly.
  {
    const file = probe("dev");
    const r = run(file, {});
    check("a development build runs", r.status === 0, r.out);
    check(
      "it warns that the key is not for distribution",
      /NOT for distribution/.test(r.out),
      r.out,
    );
    check(
      "it uses the repository development key",
      /PROBE_KEY=.*_toolchain[\\/]updater[\\/]devchievements\.key/.test(r.out),
      r.out,
    );
    check("it supplies the development password", /PROBE_PASSWORD=devchievements-dev/.test(r.out), r.out);
  }

  // 2. Release without a key must ABORT. This is the case the old script got wrong:
  //    it silently substituted the development key.
  {
    const file = probe("release-no-key");
    const r = run(file, { DEVCH_RELEASE: "1" });
    check("a release build with no key fails", r.status === 7, "exit " + r.status + "\n" + r.out);
    check("it says the key is missing", /TAURI_SIGNING_PRIVATE_KEY is empty/.test(r.out), r.out);
    check(
      "it does not fall back to the development key",
      !/devchievements\.key|devchievements-dev/.test(r.out),
      r.out,
    );
  }

  // 3. Release with a key but no password must also ABORT.
  {
    const file = probe("release-no-password");
    const r = run(file, {
      DEVCH_RELEASE: "1",
      TAURI_SIGNING_PRIVATE_KEY: "C:\\ci\\release.key",
    });
    check("a release build with no password fails", r.status === 7, "exit " + r.status + "\n" + r.out);
    check(
      "it says the password is missing",
      /TAURI_SIGNING_PRIVATE_KEY_PASSWORD is empty/.test(r.out),
      r.out,
    );
    check(
      "it does not fall back to the development password",
      !/PROBE_PASSWORD=devchievements-dev/.test(r.out),
      r.out,
    );
  }

  // 4. Release with both: the CI values must survive. The old script overwrote the key
  //    unconditionally, so this is the assertion that pins the fix.
  {
    const file = probe("release-ok");
    const r = run(file, {
      DEVCH_RELEASE: "1",
      TAURI_SIGNING_PRIVATE_KEY: "C:\\ci\\release.key",
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "s3cret-from-ci",
    });
    check("a release build with both secrets proceeds", r.status === 0, r.out);
    check(
      "the CI key is not overwritten",
      /PROBE_KEY=C:\\ci\\release\.key/.test(r.out),
      r.out,
    );
    check(
      "the CI password is not overwritten",
      /PROBE_PASSWORD=s3cret-from-ci/.test(r.out),
      r.out,
    );
    check(
      "the development fallback is not mentioned",
      !/devchievements-dev/.test(r.out),
      r.out,
    );
  }

  // 5. The comment that claimed the opposite must be gone: a comment that describes a
  //    behaviour the code does not have is worse than no comment.
  {
    const text = fs.readFileSync(installer, "utf8");
    check(
      "the script documents the two paths",
      /DEVCH_RELEASE=1/.test(text) && /local development/.test(text),
      text.slice(0, 400),
    );
  }

  console.log("");
  if (failures) {
    console.log("RELEASE SIGNING FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("RELEASE SIGNING PASSED: a release build refuses to sign with the development key");
  }
}

main();

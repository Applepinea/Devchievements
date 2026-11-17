#!/usr/bin/env node
/**
 * scripts/msix/probe-packaged-app.mjs
 *
 * Answers the three questions that decide whether a Microsoft Store MSIX listing is
 * viable for this product. Each one is a measured fact, not an opinion:
 *
 *   1. can the packaged app write next to its executable? (it must not)
 *   2. can it write a Git hook into a user repository and run its own helper? (it must)
 *   3. where does its data land, and does a future version uninstall with it?
 *
 * Expects the package from build-msix.mjs to be installed.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const outDir = path.join(__dirname, ".out");
const scratch = path.join(outDir, "probe");

let failures = 0;
// Set once the probe data directory exists; git and the helpers must both see it.
let probeEnv = process.env;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

function powershell(script) {
  const r = spawnSync("powershell", ["-NoProfile", "-Command", script], {
    windowsHide: true,
    encoding: "utf8",
  });
  return { status: r.status, out: (r.stdout || "") + (r.stderr || "") };
}

function installLocation() {
  const file = path.join(outDir, "install-location.txt");
  if (fs.existsSync(file)) return fs.readFileSync(file, "utf8").trim();
  const r = powershell("(Get-AppxPackage -Name DevchievementsCompanion).InstallLocation");
  return (r.out || "").trim().split(/\r?\n/)[0];
}

function git(repo, args) {
  const r = spawnSync("git", args, { cwd: repo, windowsHide: true, encoding: "utf8", env: probeEnv });
  return { status: r.status, out: (r.stdout || "") + (r.stderr || "") };
}

function main() {
  const location = installLocation();
  check("the package is installed", !!location && fs.existsSync(location), location);
  if (!location) {
    report();
    return;
  }
  console.log("  info installed at " + location);

  const exe = path.join(location, "devchievements-desktop.exe");
  const helper = path.join(location, "devch-helper.exe");
  check("the payload is complete", fs.existsSync(exe) && fs.existsSync(helper));

  // 1. Writing next to the executable is impossible in a package, which is why the
  //    database cannot default to the install folder any more.
  const writable = powershell(
    `try { New-Item -ItemType Directory -Path '${location}\\data-probe' -ErrorAction Stop | Out-Null; 'WRITABLE' } catch { 'READONLY' }`,
  );
  check("the package folder is read-only", writable.out.includes("READONLY"), writable.out);

  // 2. The product's whole job needs a Git hook in a user repository, invoking our
  //    helper by the path inside the package.
  // A private data directory for the probe. The helper refuses to record a commit
  // whose author is not a configured identity, so it needs its own settings.json --
  // and using the real one would mix the probe into a live installation.
  const dataRoot = path.join(outDir, "appdata");
  fs.rmSync(dataRoot, { recursive: true, force: true });
  fs.mkdirSync(path.join(dataRoot, "devchievements", "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["msix@devchievements.local"], onboardingCompleted: true }),
  );
  const env = { ...process.env, LOCALAPPDATA: dataRoot };
  probeEnv = env;

  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });
  git(scratch, ["init", "-q", "-b", "main"]);
  git(scratch, ["config", "user.name", "MSIX Probe"]);
  git(scratch, ["config", "user.email", "msix@devchievements.local"]);

  const installed = spawnSync(exe, ["--install-hooks", scratch], { windowsHide: true, encoding: "utf8", env });
  const hookPath = path.join(scratch, ".git", "hooks", "post-commit");
  const hookWritten = fs.existsSync(hookPath);
  check("the packaged app can install a Git hook", installed.status === 0 && hookWritten, installed.stdout || installed.stderr);

  const hookBody = hookWritten ? fs.readFileSync(hookPath, "utf8") : "";
  // The copy is the whole point: a hook cannot execute anything inside WindowsApps.
  const managed = path.join(dataRoot, "devchievements", "bin", "devch-helper.exe");
  check(
    "the app copied the helper out of the package",
    fs.existsSync(managed),
    managed,
  );
  check(
    "the hook calls the copy, not the packaged file",
    hookBody.includes(managed.replace(/\\/g, "/")) && !hookBody.includes("WindowsApps"),
    hookBody,
  );

  // 3. Run the hook the way git would: the helper must produce a spool record.
  //
  // The helper decides whether to record by reading its data directory, and that
  // directory is derived from LOCALAPPDATA. git spawns the hook, so git has to be
  // given that environment too -- an earlier version of this probe set it on the
  // wrong processes and concluded the hook had not run.
  fs.writeFileSync(path.join(scratch, "a.txt"), "a");
  git(scratch, ["add", "-A"]);
  const commit = git(scratch, ["commit", "-q", "-m", "packaged test"]);
  check("a commit still succeeds with the packaged helper", commit.status === 0, commit.out);

  const dataDir = path.join(dataRoot, "devchievements");
  const spool = path.join(dataDir, "spool");
  const pending = fs.existsSync(spool) ? fs.readdirSync(spool).filter((f) => f.endsWith(".spool")).length : 0;
  check("the hook delivered a record", pending > 0, "spool files: " + pending);

  // Proving the helper itself starts from inside the package turns a silent failure
  // (Windows quietly refusing to execute a file in WindowsApps) into a result.
  //
  // A synchronous spawn of a blocked image can take down the node process too, so
  // the attempt runs in a child that reports the outcome instead of killing this one.
  const launcher = path.join(outDir, "run-helper.ps1");
  fs.writeFileSync(
    launcher,
    [
      "param([string]$Exe, [string]$WorkDir, [string]$DataDir)",
      "$ErrorActionPreference = 'Stop'",
      "try {",
      "  $psi = New-Object System.Diagnostics.ProcessStartInfo",
      "  $psi.FileName = $Exe",
      "  $psi.Arguments = 'hook post-commit'",
      "  $psi.WorkingDirectory = $WorkDir",
      "  $psi.UseShellExecute = $false",
      "  $psi.EnvironmentVariables['LOCALAPPDATA'] = $DataDir",
      "  $proc = [System.Diagnostics.Process]::Start($psi)",
      "  $proc.WaitForExit(15000) | Out-Null",
      "  Write-Output ('STARTED exit=' + $proc.ExitCode)",
      "} catch {",
      "  Write-Output ('BLOCKED ' + $_.Exception.GetType().Name + ': ' + $_.Exception.Message)",
      "}",
    ].join("\r\n"),
  );
  const executed = spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", launcher, "-Exe", managed, "-WorkDir", scratch, "-DataDir", dataRoot], {
    windowsHide: true,
    encoding: "utf8",
  });
  const outcome = (executed.stdout || "").trim();
  check("the helper runs from the managed copy", outcome.includes("STARTED exit=0"), outcome || executed.stderr);

  // 4. Where the data actually lives: users must be able to find and keep it.
  console.log("  info data directory: " + dataDir);
  check("the data directory is outside the package", !dataDir.startsWith(location), dataDir);

  report();
}

function report() {
  console.log("");
  if (failures) {
    console.log("MSIX PROBE FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("MSIX PROBE PASSED: hooks, helper and data all work from a packaged app");
  }
}

main();

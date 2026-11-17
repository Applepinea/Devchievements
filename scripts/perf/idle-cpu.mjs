#!/usr/bin/env node
/**
 * scripts/perf/idle-cpu.mjs
 *
 * Spec 44.1 target: the companion costs less than 1% CPU while idle.
 *
 * Why the whole process tree: the window is WebView2, which runs its own processes
 * (msedgewebview2.exe). Measuring only the executable we started would report a
 * number that no user would recognise in Task Manager.
 *
 * Method: start the app with an isolated data directory, let it settle, then compare
 * the sum of CPU time across the tree at the start and end of a fixed window. Reported
 * both as a share of one core and as a share of the machine, because "1%" in the spec
 * is only meaningful next to which of the two it means.
 *
 * Usage: node scripts/perf/idle-cpu.mjs [--seconds 60]
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const outDir = path.join(__dirname, ".out");
const scratch = path.join(outDir, "idle");

function arg(name, fallback) {
  const i = process.argv.indexOf("--" + name);
  return i >= 0 && process.argv[i + 1] ? Number(process.argv[i + 1]) : fallback;
}
const SECONDS = arg("seconds", 60);

// Sum CPU time (ms) of the app process and everything it spawned.
function treeCpuMs(rootPid) {
  const script = [
    "$root = " + rootPid,
    "$all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId",
    "$ids = @($root)",
    "$found = $true",
    "while ($found) {",
    "  $found = $false",
    "  foreach ($p in $all) {",
    "    if ($ids -contains $p.ParentProcessId -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $found = $true }",
    "  }",
    "}",
    "$total = 0",
    "foreach ($id in $ids) {",
    "  $proc = Get-Process -Id $id -ErrorAction SilentlyContinue",
    "  if ($proc) { $total += $proc.TotalProcessorTime.TotalMilliseconds }",
    "}",
    "Write-Output ([math]::Round($total, 1))",
    "Write-Output ($ids -join ',')",
  ].join("; ");
  const r = spawnSync("powershell", ["-NoProfile", "-Command", script], { encoding: "utf8", windowsHide: true });
  const lines = (r.stdout || "").trim().split(/\r?\n/);
  return { ms: Number(lines[0] || 0), pids: (lines[1] || "").split(",").filter(Boolean).length };
}

const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing " + appExe + "; build it first");
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  const dataDir = path.join(dataRoot, "devchievements");
  fs.mkdirSync(path.join(dataDir, "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["idle@devchievements.local"], onboardingCompleted: true }),
  );
  const env = { ...process.env, LOCALAPPDATA: dataRoot };

  const appLog = path.join(scratch, "app.log");
  const fd = fs.openSync(appLog, "w");
  const app = spawn(appExe, [], { stdio: ["ignore", fd, fd], windowsHide: true, env });
  process.on("exit", () => {
    try {
      app.kill();
    } catch {}
  });

  const deadline = Date.now() + 30000;
  let ready = false;
  while (Date.now() < deadline && !ready) {
    ready = fs.readFileSync(appLog, "utf8").includes("IDENTITY from settings.json");
    if (!ready) sleep(100);
  }
  if (!ready) {
    console.error("the app never became ready; see " + appLog);
    process.exit(1);
  }

  // Let start-up work (first drain, WebView2 warm-up) finish before sampling.
  sleep(10000);

  const before = treeCpuMs(app.pid);
  const t0 = Date.now();
  sleep(SECONDS * 1000);
  const elapsed = Date.now() - t0;
  const after = treeCpuMs(app.pid);

  try {
    app.kill();
  } catch {}
  fs.closeSync(fd);

  const deltaMs = after.ms - before.ms;
  const corePercent = (deltaMs / elapsed) * 100;
  const machinePercent = corePercent / os.cpus().length;
  const report = {
    measured_at: new Date().toISOString(),
    method: "sum of TotalProcessorTime across the app process tree over a fixed idle window",
    spec_target_percent: 1,
    device: os.hostname(),
    os: os.version() + " (" + os.release() + ")",
    cpus: os.cpus().length + " x " + (os.cpus()[0] || {}).model,
    profile: "debug (dev build, no optimisation)",
    window_seconds: +(elapsed / 1000).toFixed(1),
    processes: Math.max(before.pids, after.pids),
    cpu_ms: +deltaMs.toFixed(1),
    percent_of_one_core: +corePercent.toFixed(3),
    percent_of_machine: +machinePercent.toFixed(3),
    within_budget: machinePercent < 1,
  };
  fs.writeFileSync(path.join(outDir, "idle-cpu.json"), JSON.stringify(report, null, 2));

  console.log("");
  console.log("window: " + report.window_seconds + " s, processes: " + report.processes);
  console.log("cpu: " + report.cpu_ms + " ms -> " + report.percent_of_one_core + "% of one core, " + report.percent_of_machine + "% of the machine");
  console.log("target: < 1% idle -> " + (report.within_budget ? "WITHIN" : "OVER"));
  console.log("report: " + path.join(outDir, "idle-cpu.json"));
  process.exit(report.within_budget ? 0 : 1);
}

main();

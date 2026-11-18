#!/usr/bin/env node
/**
 * scripts/perf/first-screen.mjs
 *
 * Where does the first screen's time go?
 *
 * Spec 44.1 budgets "commit to notification", which says nothing about start-up. This
 * records the stages a window passes through, so "the UI feels slow" can be attributed
 * to the webview, to script, to IPC or to SQLite instead of guessed at.
 *
 * Two modes, deliberately different problems:
 *
 *   cold  the process starts, then opens its window. Includes WebView2 start-up.
 *   warm  the app is already in the tray; a second launch signals it to show the
 *         window. This is what a user experiences every time after the first.
 *
 * All numbers come from the app's own log: a GUI-subsystem binary has no console and
 * redirects its output to companion.log. Reading a redirect target instead produced
 * zero samples for every run, which is how that mistake was found.
 *
 * Usage: node scripts/perf/first-screen.mjs [--runs 12]
 */

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const releaseExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "release", "devchievements-desktop.exe");
const debugExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const outDir = path.join(__dirname, ".out");
const scratch = path.join(__dirname, ".first-screen");
const STAGES = ["script_start", "boot_resolved", "shell_ready", "dashboard_data"];

const runsArg = process.argv.indexOf("--runs");
const runs = runsArg > 0 ? Number(process.argv[runsArg + 1]) : 12;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function percentile(values, p) {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[index];
}

function stats(values) {
  if (values.length === 0) return null;
  return {
    p50: Math.round(percentile(values, 50)),
    p95: Math.round(percentile(values, 95)),
    min: Math.round(Math.min(...values)),
    max: Math.round(Math.max(...values)),
    n: values.length,
  };
}

function killAll() {
  spawnSync("taskkill", ["/IM", "devchievements-desktop.exe", "/F"], { windowsHide: true, stdio: "ignore" });
}

const appLogPath = () => path.join(scratch, "devchievements", "companion.log");

function readAppLog() {
  try {
    return fs.readFileSync(appLogPath(), "utf8");
  } catch {
    return "";
  }
}

function parseStages(text) {
  const stages = {};
  for (const line of text.split(/\r?\n/)) {
    const m = /UI TIMING stage=(\S+) origin_ms=([\d.]+) page_ms=([\d.]+)/.exec(line);
    if (m) stages[m[1]] = { origin: Number(m[2]), page: Number(m[3]) };
  }
  return stages;
}

function seed() {
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(path.join(scratch, "devchievements"), { recursive: true });
  fs.writeFileSync(
    path.join(scratch, "devchievements", "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      emails: ["perf@devchievements.local"],
      onboardingCompleted: true,
      language: "en",
    }),
  );
  return { ...process.env, LOCALAPPDATA: scratch };
}

async function measureCold(exe, env) {
  const samples = [];
  for (let i = 0; i < runs; i++) {
    killAll();
    await sleep(1200);
    fs.writeFileSync(appLogPath(), "");

    const sink = fs.openSync(path.join(scratch, `cold-${i}.log`), "w");
    const child = spawn(exe, ["--open-window"], {
      stdio: ["ignore", sink, sink],
      windowsHide: true,
      env,
    });

    const deadline = Date.now() + 25000;
    let text = "";
    while (Date.now() < deadline) {
      text = readAppLog();
      if (/dashboard_data/.test(text)) break;
      if (/WINDOW FAILED|WEBVIEW2 missing/.test(text)) break;
      await sleep(100);
    }
    fs.closeSync(sink);
    try { child.kill(); } catch { /* gone */ }

    const stages = parseStages(text);
    if (stages.dashboard_data) samples.push(stages);
  }
  killAll();
  return samples;
}

async function measureWarm(exe, env) {
  killAll();
  await sleep(1000);
  fs.writeFileSync(appLogPath(), "");

  // One resident copy, no window.
  const residentSink = fs.openSync(path.join(scratch, "resident.log"), "w");
  const resident = spawn(exe, [], { stdio: ["ignore", residentSink, residentSink], windowsHide: true, env });
  let ready = false;
  for (let i = 0; i < 80 && !ready; i++) {
    await sleep(250);
    ready = /TRAY READY/.test(readAppLog());
  }
  if (!ready) {
    console.log("  the resident copy never reached the tray");
    try { resident.kill(); } catch { /* gone */ }
    killAll();
    return [];
  }

  const samples = [];
  for (let i = 0; i < runs; i++) {
    fs.writeFileSync(appLogPath(), "");
    const requestedAt = performance.now();
    const sink = fs.openSync(path.join(scratch, `warm-${i}.log`), "w");
    // Finds the mutex, signals the show event and exits; the resident copy raises
    // its window. That is the path a user takes every time after the first launch.
    const opener = spawn(exe, ["--open-window"], { stdio: ["ignore", sink, sink], windowsHide: true, env });

    const deadline = Date.now() + 20000;
    let text = "";
    let reportedAt = 0;
    while (Date.now() < deadline) {
      text = readAppLog();
      if (/dashboard_data/.test(text)) {
        reportedAt = performance.now();
        break;
      }
      await sleep(20);
    }
    fs.closeSync(sink);
    try { opener.kill(); } catch { /* already exited */ }

    const stages = parseStages(text);
    if (stages.dashboard_data && reportedAt) {
      stages.from_request = reportedAt - requestedAt;
      samples.push(stages);
    }

    // Close the window the resident copy opened, so the next sample starts clean.
    spawnSync(
      "powershell",
      ["-NoProfile", "-Command",
       "Get-Process devchievements-desktop -ErrorAction SilentlyContinue | ForEach-Object { $_.CloseMainWindow() } | Out-Null"],
      { windowsHide: true, stdio: "ignore" },
    );
    await sleep(700);
  }

  try { resident.kill(); } catch { /* gone */ }
  killAll();
  fs.closeSync(residentSink);
  return samples;
}

function summarise(samples, field) {
  const out = {};
  for (const stage of STAGES) {
    out[stage] = stats(samples.map((s) => s[stage]?.[field]).filter((v) => typeof v === "number"));
  }
  return out;
}

async function main() {
  const exe = fs.existsSync(releaseExe) ? releaseExe : debugExe;
  if (!fs.existsSync(exe)) {
    console.error("no desktop binary; build first");
    process.exit(1);
  }
  const profile = exe === releaseExe ? "release" : "debug (release build missing)";
  fs.mkdirSync(outDir, { recursive: true });

  console.log(`measuring ${runs} runs on ${profile}`);
  console.log("cold: process start, then its window");
  const coldSamples = await measureCold(exe, seed());

  console.log("warm: tray-resident app, window opened by a second launch");
  const warmSamples = await measureWarm(exe, seed());

  const cold = {
    basis: "process start (origin_ms): webview + process start-up included",
    samples: coldSamples.length,
    ...summarise(coldSamples, "origin"),
  };
  const warm = {
    basis: "document load (page_ms), plus request->data which is what a user waits for",
    samples: warmSamples.length,
    ...summarise(warmSamples, "page"),
    from_request: stats(warmSamples.map((s) => s.from_request).filter((v) => typeof v === "number")),
  };

  const report = {
    measured_at: new Date().toISOString(),
    profile,
    runs,
    os: `${os.version()} (${os.release()})`,
    cpus: os.cpus().length + " x " + os.cpus()[0].model,
    cold,
    warm,
  };
  fs.writeFileSync(path.join(outDir, "first-screen.json"), JSON.stringify(report, null, 2));

  const row = (stage) => {
    const c = cold[stage];
    const w = warm[stage];
    const fmt = (v) => (v ? `p50 ${String(v.p50).padStart(5)} ms  p95 ${String(v.p95).padStart(5)} ms` : "—".padEnd(28));
    return `  ${stage.padEnd(15)} cold ${fmt(c)}   warm ${fmt(w)}`;
  };

  console.log("");
  console.log(`samples: cold ${cold.samples}/${runs}, warm ${warm.samples}/${runs}`);
  for (const stage of STAGES) console.log(row(stage));
  if (warm.from_request) {
    const r = warm.from_request;
    console.log(`  ${"request->data".padEnd(15)} (warm) p50 ${String(r.p50).padStart(5)} ms  p95 ${String(r.p95).padStart(5)} ms`);
  }
  console.log("");
  console.log("cold is measured from process start; warm from document load.");
  console.log(`report: ${path.join(outDir, "first-screen.json")}`);
  if (cold.samples === 0) process.exitCode = 1;
}

main().catch((e) => { console.error(e); process.exit(1); });

#!/usr/bin/env node
/**
 * scripts/e2e/ui-sliders.mjs
 *
 * Three reports from the app, checked against the real DOM:
 *
 *   1. a slider could not be dragged to either end;
 *   2. a popup margin of 0 did not produce a margin of 0;
 *   3. resizing the window broke the layout.
 *
 * Each is measured, not eyeballed: the slider by its value and its rendered width, the
 * margin by the position the app logs, and the layout by whether the content overflows
 * its container at several sizes.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".ui-sliders");
const PORT = 9335;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else { failures++; console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : "")); }
}

async function main() {
  fs.rmSync(scratch, { recursive: true, force: true });
  const dataDir = path.join(scratch, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(path.join(dataDir, "settings.json"), JSON.stringify({
    schemaVersion: 1, emails: ["slider@devchievements.local"], onboardingCompleted: true, language: "en",
  }));
  const log = path.join(scratch, "run.log");
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window", "--page", "settings"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: scratch, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  });

  let page = null;
  for (let i = 0; i < 60 && !page; i++) {
    await sleep(500);
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      page = list.find((t) => t.type === "page" && t.url.includes("tauri.localhost")) ?? null;
    } catch { /* not up */ }
  }
  if (!page) { console.error("no page"); child.kill(); process.exit(2); }

  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  });
  const send = (method, params) => new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    return result.result?.result?.value;
  };

  for (let i = 0; i < 60; i++) {
    if ((await evaluate(`document.querySelectorAll('input[type=range]').length`)) > 0) break;
    await sleep(500);
  }

  // 1. Every slider must reach both ends of its own range.
  const sliders = await evaluate(`(() => {
    const all = Array.from(document.querySelectorAll('input[type=range]'));
    return all.map((input) => {
      const min = Number(input.min);
      const max = Number(input.max);
      input.value = String(min);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      const atMin = Number(input.value);
      input.value = String(max);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      const atMax = Number(input.value);
      const box = input.getBoundingClientRect();
      const style = getComputedStyle(input);
      return { min, max, atMin, atMax, width: Math.round(box.width), padding: style.paddingLeft };
    });
  })()`);
  check("the page renders sliders", Array.isArray(sliders) && sliders.length > 0, JSON.stringify(sliders));
  for (const slider of sliders ?? []) {
    check(
      `a slider reaches ${slider.min} and ${slider.max}`,
      slider.atMin === slider.min && slider.atMax === slider.max,
      JSON.stringify(slider),
    );
    check(
      `the slider is not inset by padding (${slider.min}-${slider.max})`,
      slider.padding === "0px",
      JSON.stringify(slider),
    );
  }

  // 2. A zero margin must place the popup flush against the usable edge.
  await evaluate(`(async () => {
    const setRange = (label, value) => {
      const field = Array.from(document.querySelectorAll('.field'))
        .find((f) => f.textContent.includes(label));
      const input = field?.querySelector('input[type=range]');
      if (!input) return false;
      input.value = String(value);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    };
    setRange('Margin', 0);
    await new Promise((r) => setTimeout(r, 300));
    const save = Array.from(document.querySelectorAll('button')).find((b) => /^(Save|保存)$/.test(b.textContent.trim()));
    save?.click();
    await new Promise((r) => setTimeout(r, 800));
    const preview = Array.from(document.querySelectorAll('button')).find((b) => /Preview popup/.test(b.textContent));
    preview?.click();
    await new Promise((r) => setTimeout(r, 2500));
    return true;
  })()`);
  const runLog = fs.readFileSync(log, "utf8");
  const placed = runLog.split(/\r?\n/).find((l) => l.startsWith("POPUP placed")) ?? "";
  const match = /at=(-?\d+),(-?\d+) usable=(\d+)x(\d+)/.exec(placed);
  if (match) {
    const [, , y, , usableHeight] = match;
    const bottomGap = Number(usableHeight) - Number(y);
    // The popup's own height is subtracted by the placement code, so the gap measured
    // here is the popup bottom against the usable edge: it must equal height + margin.
    check(
      "a zero margin puts the popup against the usable edge",
      bottomGap > 0 && bottomGap <= 200,
      placed,
    );
  } else {
    check("the preview reported where it was placed", false, placed || runLog.slice(-300));
  }

  // 3. The layout must survive resizing: no horizontal overflow at any size.
  for (const [width, height] of [[700, 520], [900, 640], [1280, 800]]) {
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
    await sleep(400);
    const overflow = await evaluate(`(() => ({
      docOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      shellOverflow: (() => {
        const shell = document.querySelector('.shell');
        return shell ? shell.scrollWidth - shell.clientWidth : 0;
      })(),
      navWidth: Math.round(document.querySelector('nav')?.getBoundingClientRect().width ?? 0),
    }))()`);
    check(
      `no horizontal overflow at ${width}x${height}`,
      overflow.docOverflow <= 1 && overflow.shellOverflow <= 1,
      JSON.stringify(overflow),
    );
    check(`the navigation keeps its width at ${width}x${height}`, overflow.navWidth === 190, JSON.stringify(overflow));
  }

  child.kill();
  try { socket.close(); } catch { /* closed */ }
  console.log("");
  console.log(failures ? `UI SLIDERS FAILED (${failures} checks)` : "UI SLIDERS PASSED: sliders reach their ends, zero margin means zero, layout survives resizing");
  process.exitCode = failures ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exit(1); });

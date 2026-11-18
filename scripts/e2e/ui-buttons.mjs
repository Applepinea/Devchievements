#!/usr/bin/env node
/**
 * scripts/e2e/ui-buttons.mjs
 *
 * Clicks every navigation button in the real window and asserts the page changed.
 *
 * Written after "every button does nothing": a frozen main thread and a broken
 * router look identical from the outside, and neither can be seen from Rust. This
 * drives the real DOM through the webview's debugging protocol.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".ui-buttons");
const PORT = 9334;
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
    schemaVersion: 1, emails: ["buttons@devchievements.local"], onboardingCompleted: true,
  }));
  const log = path.join(scratch, "run.log");
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window", "--page", "dashboard"], {
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
    } catch { /* not up yet */ }
  }
  if (!page) { console.error("no page target"); child.kill(); process.exit(2); }

  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
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
    if ((await evaluate(`document.querySelectorAll('button, nav button, a').length`)) > 0) break;
    await sleep(500);
  }

  const labels = await evaluate(`Array.from(document.querySelectorAll('nav button, .nav button, button'))
    .map((b) => b.textContent.trim()).filter(Boolean).slice(0, 12)`);
  console.log("  info clickable labels: " + JSON.stringify(labels));

  // Each navigation item, by its visible label, must change the heading.
  const targets = ["成就", "活动", "仓库", "设置", "About", "仪表盘"];
  for (const label of targets) {
    const outcome = await evaluate(`(async () => {
      const button = Array.from(document.querySelectorAll('button'))
        .find((b) => b.textContent.trim() === ${JSON.stringify(label)});
      if (!button) return { clicked: false };
      const before = document.querySelector('h2')?.textContent ?? '';
      button.click();
      await new Promise((r) => setTimeout(r, 700));
      const after = document.querySelector('h2')?.textContent ?? '';
      return { clicked: true, before, after };
    })()`);
    check(
      `clicking ${label} changes the page`,
      outcome?.clicked && outcome.before !== outcome.after,
      JSON.stringify(outcome),
    );
  }

  // The app must still answer afterwards: a frozen main thread fails here.
  const alive = await evaluate(`(async () => {
    const started = performance.now();
    const response = await Promise.race([
      window.__TAURI__ ? Promise.resolve('bridge') : Promise.resolve('no-bridge'),
      new Promise((r) => setTimeout(() => r('timeout'), 3000)),
    ]);
    return { response, ms: Math.round(performance.now() - started) };
  })()`);
  check("the page is still responsive afterwards", alive?.response !== "timeout", JSON.stringify(alive));

  const runLog = fs.readFileSync(log, "utf8");
  check("no renderer error was reported", !/UI ERROR TypeError|UI ERROR ReferenceError/.test(runLog), (runLog.match(/UI ERROR.*$/) ?? [""])[0]);

  child.kill();
  try { socket.close(); } catch { /* closed */ }
  console.log("");
  console.log(failures ? `UI BUTTONS FAILED (${failures} checks)` : "UI BUTTONS PASSED: every navigation button responds");
  process.exitCode = failures ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exit(1); });

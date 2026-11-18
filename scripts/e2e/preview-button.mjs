#!/usr/bin/env node
/**
 * scripts/e2e/preview-button.mjs
 *
 * Clicks the real "Preview popup" button in the real settings window, through the
 * webview's own debugging protocol, and reports what happened to the click.
 *
 * Why this exists: the popup could be shown from Rust (--preview-popup) while the
 * button in the page still did nothing, and no test drove the button itself.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".preview-button");
const PORT = 9333;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  const response = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  return response.json();
}

async function main() {
  fs.rmSync(scratch, { recursive: true, force: true });
  const dataDir = path.join(scratch, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(
    path.join(dataDir, "settings.json"),
    JSON.stringify({
      schemaVersion: 1,
      emails: ["button@devchievements.local"],
      onboardingCompleted: true,
      popup: {
        width: 364, height: null, scalePercent: 100, iconSize: 52, cornerRadius: 0,
        screenMargin: 0, corner: "bottom_right", opacityPercent: 94,
        displaySeconds: 8, animationMs: 240, backgroundBlur: 0,
      },
    }),
  );
  const log = path.join(scratch, "run.log");
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window", "--page", "settings"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: {
      ...process.env,
      LOCALAPPDATA: scratch,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
  });

  let page = null;
  for (let i = 0; i < 60 && !page; i++) {
    await sleep(500);
    try {
      const list = await targets();
      // The first target is about:blank; the app's own page is the one served from
      // the Tauri origin. Matching on it is what makes this test look at the UI
      // instead of an empty document.
      page =
        list.find((t) => t.type === "page" && t.url.includes("tauri.localhost")) ??
        list.find((t) => t.type === "page" && t.url.includes("index.html")) ??
        null;
      if (!page && i === 20) {
        console.log("  info targets so far: " + JSON.stringify(list.map((t) => t.url)));
      }
    } catch {
      // not listening yet
    }
  }
  if (!page) {
    console.error("no debugging target; log:\n" + fs.readFileSync(log, "utf8"));
    child.kill();
    process.exit(2);
  }
  console.log("  info page target: " + page.url);

  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    }
  });
  const send = (method, params) =>
    new Promise((resolve) => {
      const id = nextId++;
      pending.set(id, resolve);
      socket.send(JSON.stringify({ id, method, params }));
    });

  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    return result.result?.result?.value;
  };

  // The debugging target exists before React has mounted, so an immediate query
  // returns an empty page. Wait for the UI to render rather than for the window.
  //
  // Waiting for *any* button is no longer enough: pages other than the dashboard are
  // now loaded on demand, so the navigation renders first and the page content
  // arrives a moment later. Wait for this page's own control, or the test measures
  // the chunk boundary instead of the button.
  for (let i = 0; i < 80; i++) {
    const ready = await evaluate(`
      Array.from(document.querySelectorAll('button'))
        .some((b) => /预览弹窗|Preview popup/.test(b.textContent || ''))
    `);
    if (ready) break;
    await sleep(250);
  }

  // 1. Which buttons does the page actually render?
  const buttons = await evaluate(`
    Array.from(document.querySelectorAll('button')).map((b) => b.textContent.trim()).filter(Boolean)
  `);
  console.log("  info buttons: " + JSON.stringify(buttons));

  // 2. Click the preview button and read the notice it sets.
  const clicked = await evaluate(`(async () => {
    const button = Array.from(document.querySelectorAll('button'))
      .find((b) => /预览弹窗|Preview popup/.test(b.textContent || ''));
    if (!button) return { found: false };
    button.click();
    // Creating a webview window takes noticeably longer than a click handler, so the
    const notice = Array.from(document.querySelectorAll('.notice'))
      .map((n) => n.textContent.trim())
      .filter(Boolean);
    return { found: true, notice };
  })()`);

  console.log("  info click result: " + JSON.stringify(clicked));
  await sleep(2000);
  const runLog = fs.readFileSync(log, "utf8");
  const popupLines = runLog.split(/\r?\n/).filter((l) => /POPUP|PREVIEW/.test(l));
  console.log("  info popup lines: " + JSON.stringify(popupLines));

  let failures = 0;
  const check = (label, ok, detail) => {
    if (ok) console.log("  ok   " + label);
    else { failures++; console.log("  FAIL " + label + (detail ? "\n        " + detail : "")); }
  };
  check("the page renders a preview button", !!clicked?.found, JSON.stringify(buttons));
  check("clicking it asks Rust to show a popup", popupLines.some((l) => /POPUP visible|POPUP preview shown|POPUP preview failed/.test(l)), JSON.stringify(popupLines));
  check("no error was reported to the page", !/UI ERROR/.test(runLog), (runLog.match(/UI ERROR.*$/) ?? [""])[0]);

  child.kill();
  try { socket.close(); } catch { /* already closed */ }
  console.log("");
  console.log(failures ? `PREVIEW BUTTON FAILED (${failures} checks)` : "PREVIEW BUTTON PASSED");
  process.exitCode = failures ? 1 : 0;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

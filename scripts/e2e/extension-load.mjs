#!/usr/bin/env node
/**
 * scripts/e2e/extension-load.mjs
 *
 * Loads the built extension into a real browser and proves, over CDP, that:
 *   1. the extension installs;
 *   2. its MV3 service worker runs our code (manifest name);
 *   3. the Rust core WASM instantiates inside that worker under the MV3 CSP.
 *
 * Why this shape: Chromium 137+ ignores --load-extension unless the build is
 * started with --enable-unsafe-extension-debugging, and even then the MV3 worker
 * is lazy, so "no worker" proves nothing. The sanctioned route is the CDP
 * Extensions.loadUnpacked command, which also returns the extension id.
 *
 * This is the evidence ADR-0001 was missing.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const extDir = path.join(repoRoot, "apps", "extension", ".output", "chrome-mv3");
const scratch = path.join(__dirname, ".extension");
const PORT = 9261;

const BROWSERS = [
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
];

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 500) : ""));
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Minimal CDP client over the browser websocket. */
function connect(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  const ready = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  socket.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    const entry = pending.get(msg.id);
    if (!entry) return;
    pending.delete(msg.id);
    if (msg.error) entry.reject(new Error(JSON.stringify(msg.error)));
    else entry.resolve(msg.result);
  });
  return {
    ready,
    send(method, params = {}, sessionId) {
      const id = nextId++;
      const payload = { id, method, params };
      if (sessionId) payload.sessionId = sessionId;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        socket.send(JSON.stringify(payload));
        setTimeout(() => {
          if (pending.delete(id)) reject(new Error(method + " timed out"));
        }, 20000);
      });
    },
    close() {
      socket.close();
    },
  };
}

async function main() {
  if (!fs.existsSync(extDir)) {
    console.error("missing build: " + extDir);
    process.exit(1);
  }
  const browserExe = BROWSERS.find((p) => fs.existsSync(p));
  if (!browserExe) {
    console.log("SKIP: no Chromium browser installed");
    process.exit(2);
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });
  const profile = path.join(scratch, "profile");
  fs.mkdirSync(profile, { recursive: true });

  const child = spawn(
    browserExe,
    [
      "--no-first-run",
      "--no-default-browser-check",
      "--enable-unsafe-extension-debugging",
      `--user-data-dir=${profile}`,
      `--remote-debugging-port=${PORT}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  let cdp = null;
  try {
    let version = null;
    for (let i = 0; i < 40; i += 1) {
      await sleep(500);
      try {
        const res = await fetch(`http://127.0.0.1:${PORT}/json/version`);
        version = await res.json();
        break;
      } catch {
        // Browser still starting.
      }
    }
    check("the browser exposed a debugging endpoint", !!version, "no /json/version");
    if (!version) return;

    cdp = connect(version.webSocketDebuggerUrl);
    await cdp.ready;

    const loaded = await cdp.send("Extensions.loadUnpacked", { path: extDir });
    check("the extension installs", !!loaded?.id, JSON.stringify(loaded));
    if (!loaded?.id) return;

    // Start the lazy worker by opening the popup page.
    const target = await cdp.send("Target.createTarget", {
      url: `chrome-extension://${loaded.id}/popup.html`,
    });
    check("the extension page opens", !!target?.targetId, JSON.stringify(target));

    let workerSession = null;
    for (let i = 0; i < 20 && !workerSession; i += 1) {
      await sleep(500);
      const list = await cdp.send("Target.getTargets");
      const worker = (list.targetInfos ?? []).find(
        (t) => t.type === "service_worker" && String(t.url).includes(loaded.id),
      );
      if (!worker) continue;
      const attached = await cdp.send("Target.attachToTarget", {
        targetId: worker.targetId,
        flatten: true,
      });
      workerSession = attached.sessionId;
    }
    check("the MV3 service worker is running", !!workerSession, "no service_worker target for our id");
    if (!workerSession) return;

    const name = await cdp.send(
      "Runtime.evaluate",
      { expression: "chrome.runtime.getManifest().name", returnByValue: true },
      workerSession,
    );
    check(
      "it is our extension",
      name?.result?.value === "Devchievements",
      JSON.stringify(name?.result?.value),
    );

    const wasm = await cdp.send(
      "Runtime.evaluate",
      {
        expression: `(async () => {
          const res = await fetch(chrome.runtime.getURL('wasm/devchievements_core.wasm'));
          const bytes = await res.arrayBuffer();
          const { instance } = await WebAssembly.instantiate(bytes, {});
          return { bytes: bytes.byteLength, len: instance.exports.devch_probe_len() };
        })()`,
        awaitPromise: true,
        returnByValue: true,
      },
      workerSession,
    );
    const value = wasm?.result?.value;
    check("the Rust core is packaged", (value?.bytes ?? 0) > 0, JSON.stringify(wasm));
    check("it instantiates inside the worker under the MV3 CSP", (value?.len ?? 0) > 0, JSON.stringify(wasm));
  } finally {
    cdp?.close();
    child.kill();
    await sleep(500);
  }

  console.log("");
  if (failures) {
    console.log("EXTENSION LOAD FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("EXTENSION LOAD PASSED: MV3 worker runs our code and instantiates the Rust core");
  }
}

main();

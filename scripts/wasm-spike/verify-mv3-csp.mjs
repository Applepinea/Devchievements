#!/usr/bin/env node
/**
 * scripts/wasm-spike/verify-mv3-csp.mjs
 *
 * ADR-0001 requirement 2 (spec 3.3 / 30.2): prove in a REAL browser that a
 * Manifest V3 extension page can instantiate the Rust Core wasm module, with the
 * CSP actually shipped in the built manifest.
 *
 * WHY THIS IS NOT A UNIT TEST: MV3 enforces a CSP that treats WebAssembly
 * compilation as eval-like. Whether it works depends on the shipped
 * content_security_policy string and on the browser build. Nothing short of
 * loading the extension in the browser answers the question.
 *
 * METHOD: launch Chrome with --load-extension pointing at the WXT output, open the
 * extension's own page over chromium's remote debugging protocol, and evaluate a
 * snippet that fetches the wasm asset and instantiates it.
 */

import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const extDir = path.join(repoRoot, "apps", "extension", ".output", "chrome-mv3");
const outDir = path.join(__dirname, ".out");
fs.mkdirSync(outDir, { recursive: true });

const CHROME_CANDIDATES = [
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
];

function findBrowser() {
  for (const p of CHROME_CANDIDATES) {
    if (fs.existsSync(p)) return p;
  }
  return null;
}

const PORT = 9333;

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function cdp(targetUrl) {
  const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
  const targets = await res.json();
  return targets.find((t) => t.url.startsWith(targetUrl)) ?? null;
}

async function main() {
  const report = { generated_at: new Date().toISOString(), checks: {} };

  const browser = findBrowser();
  if (!browser) {
    report.checks.browser_found = false;
    fs.writeFileSync(path.join(outDir, "mv3-csp.json"), JSON.stringify(report, null, 2) + "\n");
    console.error("no Chrome/Edge found");
    process.exitCode = 1;
    return;
  }
  report.browser = browser;
  report.checks.browser_found = true;

  if (!fs.existsSync(extDir)) {
    report.checks.extension_built = false;
    fs.writeFileSync(path.join(outDir, "mv3-csp.json"), JSON.stringify(report, null, 2) + "\n");
    console.error("extension not built at " + extDir);
    process.exitCode = 1;
    return;
  }
  report.checks.extension_built = true;

  const userDataDir = path.join(outDir, "chrome-profile");
  fs.rmSync(userDataDir, { recursive: true, force: true });

  const args = [
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${userDataDir}`,
    `--load-extension=${extDir}`,
    `--disable-extensions-except=${extDir}`,
    "--no-first-run",
    "--no-default-browser-check",
    // NOT headless: current Chrome ignores --load-extension in headless mode.
    "about:blank",
  ];

  const child = spawn(browser, args, { stdio: "ignore", windowsHide: true, detached: false });

  let extId = null;
  for (let i = 0; i < 40; i++) {
    await sleep(500);
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const found = list.find((t) => /^chrome-extension:\/\/[a-p]{32}\//.test(t.url));
      if (found) {
        const m = /^chrome-extension:\/\/([a-p]{32})\//.exec(found.url);
        extId = m?.[1] ?? null;
        break;
      }
      // The MV3 service worker may be the only extension target.
      const sw = list.find((t) => t.url.startsWith("chrome-extension://"));
      if (sw) {
        const m = /^chrome-extension:\/\/([a-p]{32})/.exec(sw.url);
        extId = m?.[1] ?? null;
        break;
      }
    } catch {
      /* browser not up yet */
    }
  }

  // If no extension target appeared, fall back to computing the unpacked
  // extension id locally (Chrome derives it from the absolute path).
  if (!extId) {
    const { createHash } = await import("node:crypto");
    const hash = createHash("sha256").update(extDir, "utf8").digest("hex").slice(0, 32);
    extId = hash.replace(/[0-9a-f]/g, (c) => String.fromCharCode(97 + parseInt(c, 16)));
    report.extension_id_source = "derived-from-path";
    report.checks.extension_target_seen = false;
  } else {
    report.extension_id_source = "browser-target";
    report.checks.extension_target_seen = true;
  }

  report.checks.extension_loaded = Boolean(extId);
  report.extension_id = extId;

  if (extId) {
    // Open the popup page as a normal tab so we can evaluate inside it.
    const popupUrl = `chrome-extension://${extId}/popup.html`;
    await fetch(`http://127.0.0.1:${PORT}/json/new?${encodeURIComponent(popupUrl)}`, { method: "PUT" });
    await sleep(2500);

    const target = await cdp(popupUrl);
    if (!target) {
      report.checks.popup_page_available = false;
    } else {
      report.checks.popup_page_available = true;
      const wsUrl = target.webSocketDebuggerUrl;

      const { WebSocket } = await import("node:ws").catch(() => ({ WebSocket: globalThis.WebSocket }));
      if (!WebSocket) {
        report.checks.cdp_available = false;
      } else {
        const socket = new WebSocket(wsUrl);
        await new Promise((res, rej) => {
          socket.onopen = res;
          socket.onerror = rej;
        });
        report.checks.cdp_available = true;

        const evaluate = (expression) =>
          new Promise((resolve, reject) => {
            const id = Math.floor(Math.random() * 1e9);
            const onMessage = (ev) => {
              const msg = JSON.parse(ev.data);
              if (msg.id === id) {
                socket.removeEventListener("message", onMessage);
                if (msg.error) reject(new Error(JSON.stringify(msg.error)));
                else resolve(msg.result?.result?.value);
              }
            };
            socket.addEventListener("message", onMessage);
            socket.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
          });

        // Wait for the popup's own probe to settle, then read its verdict.
        const status = await evaluate(
          "new Promise((res) => { const t0 = Date.now(); const tick = () => { const el = document.querySelector('section div'); if (el && el.textContent && !el.textContent.includes('loading')) return res(el.textContent); if (Date.now() - t0 > 8000) return res('TIMEOUT:' + (el && el.textContent)); setTimeout(tick, 200); }; tick(); })"
        );
        report.popup_status_text = status;
        report.checks.wasm_instantiated_under_mv3_csp =
          typeof status === "string" && status.startsWith("loaded");

        // Independent check: does the CSP actually contain wasm-unsafe-eval?
        const csp = await evaluate("JSON.stringify(chrome.runtime.getManifest().content_security_policy)");
        report.shipped_csp = csp;
        report.checks.csp_declares_wasm_unsafe_eval =
          typeof csp === "string" && csp.includes("wasm-unsafe-eval");

        socket.close();
      }
    }
  }

  fs.writeFileSync(path.join(outDir, "mv3-csp.json"), JSON.stringify(report, null, 2) + "\n");

  try {
    child.kill();
  } catch {
    /* ignore */
  }

  console.log(JSON.stringify(report, null, 2));
  const failed = Object.entries(report.checks).filter(([, v]) => v === false);
  if (failed.length) {
    console.error("FAILED CHECKS: " + failed.map(([k]) => k).join(", "));
    process.exitCode = 1;
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

#!/usr/bin/env node
/**
 * scripts/e2e/service-worker-restart.mjs
 *
 * Spec 17.7 / 36.2: an MV3 worker can be killed at any moment, and its state must
 * survive. Everything durable therefore lives in IndexedDB, never in module scope.
 *
 * # Why this needs a real kill
 *
 * Reading the store twice in one worker proves nothing: the data could be sitting in
 * a module-scope variable. The only honest test stops the worker and then reads again
 * through a fresh one, which is what the browser does to us in production.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const extDir = path.join(repoRoot, "apps", "extension", ".output", "chrome-mv3");
const scratch = path.join(__dirname, ".sw-restart");
const PORT = 9271;
const SHA = "abcdef0123456789abcdef0123456789abcdef01";

const BROWSERS = [
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
];

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

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
  const appData = path.join(scratch, "appdata");
  fs.mkdirSync(path.join(appData, "devchievements"), { recursive: true });

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
    { stdio: "ignore", env: { ...process.env, LOCALAPPDATA: appData } },
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
        // Still starting.
      }
    }
    check("the browser exposed a debugging endpoint", !!version, "no /json/version");
    if (!version) return;

    cdp = connect(version.webSocketDebuggerUrl);
    await cdp.ready;

    const loaded = await cdp.send("Extensions.loadUnpacked", { path: extDir });
    check("the extension installs", !!loaded?.id, JSON.stringify(loaded));
    if (!loaded?.id) return;

    const page = await cdp.send("Target.createTarget", {
      url: `chrome-extension://${loaded.id}/popup.html`,
    });
    const attached = await cdp.send("Target.attachToTarget", {
      targetId: page.targetId,
      flatten: true,
    });
    const pageSession = attached.sessionId;

    const evaluate = async (expression) => {
      const res = await cdp.send(
        "Runtime.evaluate",
        { expression, awaitPromise: true, returnByValue: true },
        pageSession,
      );
      return res?.result?.value;
    };

    // Warm up: the lazy worker may not be listening yet, so a stats round-trip proves
    // the channel before anything is asserted about its contents.
    for (let i = 0; i < 20; i += 1) {
      const ping = await evaluate(
        `new Promise((resolve) => chrome.runtime.sendMessage({ type: "devch:stats" }, resolve))`,
      );
      if (typeof ping?.events === "number") break;
      await sleep(250);
    }

    const pageUrl = `https://github.com/o/r/commit/${SHA}`;
    const recorded = await evaluate(
      `new Promise((resolve) => chrome.runtime.sendMessage({
         type: "devch:page",
         url: "${pageUrl}",
         meta: { "octolytics-dimension-commit_date": "2026-11-17T10:00:00Z" },
         text: {}
       }, resolve))`,
    );
    check("an event was recorded before the restart", recorded?.stored === true, JSON.stringify(recorded));

    const before = await evaluate(
      `new Promise((resolve) => chrome.runtime.sendMessage({ type: "devch:stats" }, resolve))`,
    );
    check("the worker reports it", before?.events === 1, JSON.stringify(before));

    // ---- the actual test: stop the worker, then read again ----
    // A fresh worker has empty module scope, so anything read back must have come
    // from IndexedDB.
    const workers = await cdp.send("Target.getTargets");
    const ours = (workers.targetInfos ?? []).filter(
      (t) => t.type === "service_worker" && String(t.url).includes(loaded.id),
    );
    check("our worker is running before the stop", ours.length === 1, JSON.stringify(ours.map((w) => w.url)));

    // ServiceWorker.stopAllWorkers is only available on a page target, and closing
    // the worker target is the browser-level equivalent. Either way the goal is the
    // same: no worker alive, so anything read back must come from IndexedDB.
    let stopped = false;
    try {
      await cdp.send("ServiceWorker.enable", {}, pageSession);
      await cdp.send("ServiceWorker.stopAllWorkers", {}, pageSession);
      stopped = true;
    } catch {
      // Fall back to closing the worker target directly.
    }
    if (!stopped) {
      for (const worker of ours) {
        await cdp.send("Target.closeTarget", { targetId: worker.targetId });
      }
    }
    await sleep(2000);

    const afterStop = await cdp.send("Target.getTargets");
    const stillThere = (afterStop.targetInfos ?? []).filter(
      (t) => t.type === "service_worker" && String(t.url).includes(loaded.id),
    );
    check("the worker was actually stopped", stillThere.length === 0, JSON.stringify(stillThere.map((w) => w.url)));

    // Reading again wakes a new worker; the data must come back.
    let after = null;
    for (let i = 0; i < 20; i += 1) {
      after = await evaluate(
        `new Promise((resolve) => chrome.runtime.sendMessage({ type: "devch:stats" }, resolve))`,
      );
      if (typeof after?.events === "number") break;
      await sleep(300);
    }
    check("the restarted worker answers", typeof after?.events === "number", JSON.stringify(after));
    check("the event survived the restart", after?.events === 1, JSON.stringify(after));
    check("and it is still counted as a commit", after?.commits === 1, JSON.stringify(after));

    const events = await evaluate(
      `new Promise((resolve) => chrome.runtime.sendMessage({ type: "devch:events" }, resolve))`,
    );
    check("the event details survived too", events?.events?.[0]?.commitSha === SHA, JSON.stringify(events?.events?.[0]));
  } finally {
    cdp?.close();
    child.kill();
    await sleep(500);
  }

  console.log("");
  if (failures) {
    console.log("SERVICE WORKER RESTART FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("SERVICE WORKER RESTART PASSED: state survives a real worker kill");
  }
}

main();

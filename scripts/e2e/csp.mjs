#!/usr/bin/env node
/**
 * scripts/e2e/csp.mjs
 *
 * Proves the Content Security Policy in tauri.conf.json is enforced, not merely configured.
 *
 * Why this exists: the page harnesses show the strict policy breaks nothing legitimate.
 * They would pass just as happily if the policy had been dropped entirely, because a
 * dropped policy breaks nothing either. The only way to tell those two apart is to try
 * something the policy forbids and watch the renderer refuse.
 *
 * So this spawns the real devchievements-desktop.exe under WebView2, drives it over the
 * remote debugging port (same machinery as scripts/e2e/ui-buttons.mjs), and injects an
 * inline script into the live document. The acceptance criterion for spec T8 is exactly
 * that: an injected inline script must be rejected.
 *
 * A blank window would satisfy every "refused" check, so the harness also asserts the
 * bundle that shipped with the build rendered, and that inline styles (which React uses
 * and the policy deliberately allows) still apply.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const scratch = path.join(__dirname, ".csp");
const PORT = 9335;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 600) : ""));
  }
}

async function main() {
  if (!fs.existsSync(appExe)) {
    console.error("no app binary at " + appExe + "\nBuild it first: scripts/dev/build-desktop.bat build");
    process.exit(2);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  const dataDir = path.join(scratch, "devchievements");
  fs.mkdirSync(dataDir, { recursive: true });
  fs.writeFileSync(path.join(dataDir, "settings.json"), JSON.stringify({
    schemaVersion: 1, emails: ["csp@devchievements.local"], onboardingCompleted: true,
  }));
  const log = path.join(scratch, "run.log");
  const fd = fs.openSync(log, "w");
  const child = spawn(appExe, ["--open-window", "--page", "dashboard"], {
    stdio: ["ignore", fd, fd],
    windowsHide: true,
    env: { ...process.env, LOCALAPPDATA: scratch, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=" + PORT },
  });

  let page = null;
  for (let i = 0; i < 60 && !page; i++) {
    await sleep(500);
    try {
      const list = await (await fetch("http://127.0.0.1:" + PORT + "/json/list")).json();
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
  const browserLog = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
      return;
    }
    if (message.method === "Log.entryAdded") browserLog.push(message.params.entry);
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
  const policyEntries = () => browserLog.filter((e) => /Content Security Policy/i.test(e.text ?? ""));

  // Log.enable replays what the renderer buffered from page start, so this also covers
  // the launch itself: startup must not produce violations of its own.
  await send("Log.enable", {});
  await send("Runtime.enable", {});
  for (let i = 0; i < 40; i++) {
    if ((await evaluate("document.querySelectorAll('button').length")) > 0) break;
    await sleep(500);
  }
  await sleep(500);

  const early = policyEntries();
  check("the app started with no CSP violation logged", early.length === 0, early.map((e) => e.text).join(" | "));

  // Positive control: a blank window would pass every refusal check below.
  const booted = await evaluate("({ buttons: document.querySelectorAll('button').length, heading: document.querySelector('h2') ? document.querySelector('h2').textContent : '' })");
  check("the bundle that shipped with the build rendered", (booted?.buttons ?? 0) > 0 && Boolean(booted?.heading), JSON.stringify(booted));

  const watch = "  const seen = [];\n" +
    "  const onViolation = (e) => seen.push({ directive: e.violatedDirective, effective: e.effectiveDirective, blocked: e.blockedURI, sample: e.sample, policy: e.originalPolicy });\n" +
    "  document.addEventListener('securitypolicyviolation', onViolation);\n";

  // The acceptance criterion itself: an injected inline script must not run.
  const inline = await evaluate("(async () => {\n" + watch +
    "  const script = document.createElement('script');\n" +
    "  script.textContent = 'window.__CSP_INLINE_RAN = true;';\n" +
    "  document.head.appendChild(script);\n" +
    "  await new Promise((r) => setTimeout(r, 500));\n" +
    "  document.removeEventListener('securitypolicyviolation', onViolation);\n" +
    "  script.remove();\n" +
    "  return { ran: window.__CSP_INLINE_RAN === true, seen };\n" +
    "})()");
  check("an injected inline script does not execute", inline?.ran === false, JSON.stringify(inline));
  const inlineSeen = inline?.seen?.[0];
  check("the renderer reported a script-src violation for it",
    Boolean(inlineSeen) && /script-src/.test(String(inlineSeen.directive ?? "") + " " + String(inlineSeen.effective ?? "")),
    JSON.stringify(inline?.seen));
  if (inlineSeen?.policy) console.log("  info policy in force: " + String(inlineSeen.policy).replace(/\s+/g, " ").slice(0, 400));

  // A second route into the same directive: an inline event handler attribute.
  const attr = await evaluate("(async () => {\n" + watch +
    "  document.body.setAttribute('onclick', 'window.__CSP_ATTR_RAN = true;');\n" +
    "  document.body.click();\n" +
    "  await new Promise((r) => setTimeout(r, 300));\n" +
    "  document.removeEventListener('securitypolicyviolation', onViolation);\n" +
    "  document.body.removeAttribute('onclick');\n" +
    "  return { ran: window.__CSP_ATTR_RAN === true, seen };\n" +
    "})()");
  check("an injected inline event handler does not execute", attr?.ran === false, JSON.stringify(attr));

  const ev = await evaluate("(() => { try { const value = eval('1 + 1'); return { allowed: true, value }; } catch (error) { return { allowed: false, name: error.name, message: error.message }; } })()");
  console.log("  info eval from this context: " + JSON.stringify(ev));

  // The policy allows inline styles on purpose: React sets them and the app must look
  // the same. If this ever fails the policy went too far.
  const style = await evaluate("(() => { const div = document.createElement('div'); div.setAttribute('style', 'color: rgb(1, 2, 3)'); document.body.appendChild(div); const color = getComputedStyle(div).color; div.remove(); return color; })()");
  check("inline styles still apply, so React keeps its look", style === "rgb(1, 2, 3)", style);

  const reacted = await evaluate("(async () => { const before = document.querySelector('h2') ? document.querySelector('h2').textContent : ''; const buttons = Array.from(document.querySelectorAll('button')); const next = buttons.find((b) => b.textContent.trim() === '\u6210\u5c31'); if (!next) return { clicked: false, before }; next.click(); await new Promise((r) => setTimeout(r, 700)); return { clicked: true, before, after: document.querySelector('h2') ? document.querySelector('h2').textContent : '' }; })()");
  check("the app still responds after the blocked injections", reacted?.clicked === true && reacted.before !== reacted.after, JSON.stringify(reacted));

  const late = policyEntries().slice(early.length);
  if (late.length > 0) console.log("  info refusal text: " + late[0].text);

  const runLog = fs.readFileSync(log, "utf8");
  check("the native side logged no renderer error", !/UI ERROR/.test(runLog), (runLog.match(/UI ERROR.*$/) ?? [""])[0]);

  child.kill();
  try { socket.close(); } catch { /* closed */ }
  console.log("");
  console.log(failures
    ? "CSP FAILED (" + failures + " checks)"
    : "CSP PASSED: the strict policy is live and refuses an injected inline script");
  process.exitCode = failures ? 1 : 0;
}

main().catch((e) => { console.error(e); process.exit(1); });

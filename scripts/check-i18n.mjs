#!/usr/bin/env node
/**
 * scripts/check-i18n.mjs
 *
 * Every string the UI translates must exist in the Chinese dictionary. A missing
 * entry is not fatal at runtime (it falls back to English), which is exactly why it
 * needs a check: a half-translated screen looks intentional.
 *
 * It also reports strings in the dictionary that nothing uses, because those are
 * either a leftover or a typo in a key, and both are worth deleting.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const src = path.join(repoRoot, "apps", "desktop", "src");
const i18nPath = path.join(src, "i18n.ts");

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (/\.tsx?$/.test(entry.name)) out.push(full);
  }
  return out;
}

// Keys used by the UI: t("...") and tf("...").
const used = new Set();
// Keys are compared in their runtime form, not their source form: `\\n` in a string
// literal and the two characters backslash-n are the same key only after decoding,
// and a Windows path in a help text is exactly where that difference shows up.
for (const file of walk(src)) {
  if (file.endsWith("i18n.ts")) continue;
  const text = fs.readFileSync(file, "utf8");
  for (const match of text.matchAll(/\bt[f]?\(\s*"((?:[^"\\]|\\.)*)"/g)) {
    try {
      used.add(JSON.parse('"' + match[1] + '"'));
    } catch {
      // A literal with an escape JSON does not accept; keep it raw so the check
      // still reports it instead of crashing.
      used.add(match[1]);
    }
  }
  for (const match of text.matchAll(/\bt[f]?\(\s*'((?:[^'\\]|\\.)*)'/g)) {
    used.add(match[1].replace(/\\'/g, "'"));
  }
}

// Keys defined: the ZH object literal, read from source so no build step is needed.
const i18n = fs.readFileSync(i18nPath, "utf8");
const body = i18n.slice(i18n.indexOf("const ZH"), i18n.indexOf("};", i18n.indexOf("const ZH")));
const defined = new Set();
for (const match of body.matchAll(/^\s*((?:"(?:[^"\\]|\\.)*")|(?:'(?:[^'\\]|\\.)*'))\s*:/gm)) {
  const raw = match[1];
  const inner = raw.slice(1, -1);
  defined.add(raw.startsWith('"') ? JSON.parse('"' + inner + '"') : inner.replace(/\\'/g, "'"));
}

const missing = [...used].filter((k) => !defined.has(k)).sort();
const unused = [...defined].filter((k) => !used.has(k)).sort();

console.log("used " + used.size + ", defined " + defined.size);
for (const key of missing) console.log("  MISSING  " + JSON.stringify(key));
for (const key of unused) console.log("  unused   " + JSON.stringify(key));

if (missing.length) {
  console.log("\nI18N FAILED: " + missing.length + " string(s) have no Chinese translation");
  process.exit(1);
}
console.log("\nI18N PASSED: every translated string has an entry");
if (unused.length) console.log("(" + unused.length + " unused entries; harmless but worth deleting)");

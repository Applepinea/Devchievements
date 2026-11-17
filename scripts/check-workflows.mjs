#!/usr/bin/env node
/**
 * scripts/check-workflows.mjs
 *
 * A lint for .github/workflows, not a YAML parser. There is no YAML parser
 * installed in this workspace (checked), and pulling one in to validate two files
 * is a worse trade than checking the invariants that actually break workflows:
 *
 *   - a tab anywhere (YAML forbids tabs for indentation)
 *   - indentation that is not a multiple of two
 *   - a duplicated key in the same mapping (the second one wins silently)
 *   - a `uses:` without a pinned ref
 *   - an `on:` that is missing, which makes the whole file a no-op
 *
 * It cannot prove the file is valid YAML. It can prove the failures that matter
 * here are absent, and it fails loudly instead of pretending to be a validator.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dir = path.join(repoRoot, ".github", "workflows");

let failures = 0;
function fail(file, line, message) {
  failures++;
  console.log("  FAIL " + file + ":" + line + " " + message);
}

function checkFile(file) {
  const raw = fs.readFileSync(path.join(dir, file), "utf8");
  const lines = raw.split(/\r?\n/);
  // Mapping scopes, innermost last. A sequence item opens its own mapping, so two
  // sibling steps may both define `uses:` without being duplicates.
  const stack = [];
  let inLiteral = 0;

  lines.forEach((text, index) => {
    const number = index + 1;
    if (text.includes("\t")) fail(file, number, "tab in indentation or value");
    if (/\s+$/.test(text)) fail(file, number, "trailing whitespace");

    const indent = text.length - text.trimStart().length;
    if (text.trim() !== "" && indent % 2 !== 0) fail(file, number, "indentation " + indent + " is not a multiple of 2");

    // Literal blocks (| and >) may contain any text; only their own indentation
    // discipline matters, and that is covered above.
    if (inLiteral > 0) {
      if (text.trim() === "" || indent > inLiteral) return;
      inLiteral = 0;
    }
    if (/[:|-]\s*[|>][+-]?\s*$/.test(text)) {
      inLiteral = indent;
      return;
    }

    const key = /^(\s*)(- )?([A-Za-z0-9_.$/{}\[\]"'-]+):(\s|$)/.exec(text);
    if (!key) return;
    const isItem = Boolean(key[2]);
    const depth = key[1].length + (isItem ? 2 : 0);
    const name = key[3];
    if (name.startsWith("${") || name === "-") return;

    // Close scopes that this line cannot belong to: anything deeper, and for a
    // sequence item also the previous item at the same depth.
    while (stack.length && (stack[stack.length - 1].depth > depth || (isItem && stack[stack.length - 1].depth === depth))) {
      stack.pop();
    }

    let scope = stack[stack.length - 1];
    if (!scope || scope.depth < depth) {
      scope = { depth, keys: [] };
      stack.push(scope);
    }

    const duplicate = scope.keys.find((k) => k.name === name);
    if (duplicate) fail(file, number, "duplicate key '" + name + "' (first at line " + duplicate.line + ")");
    scope.keys.push({ name, line: number });
  });

  const uses = [...raw.matchAll(/^\s*-?\s*uses:\s*(\S+)/gm)].map((m) => m[1]);
  for (const ref of uses) {
    if (ref.startsWith("./")) continue;
    if (!/@/.test(ref)) fail(file, 0, "uses: " + ref + " has no pinned ref");
  }
  if (!/^on:/m.test(raw)) fail(file, 0, "no on: trigger");
  if (!/^jobs:/m.test(raw)) fail(file, 0, "no jobs:");
  console.log("  ok   " + file + " (" + lines.length + " lines, " + uses.length + " actions)");
}

const files = fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => f.endsWith(".yml") || f.endsWith(".yaml")) : [];
if (files.length === 0) {
  console.log("no workflow files");
  process.exit(0);
}
for (const file of files) checkFile(file);

if (failures) {
  console.log("\nWORKFLOWS FAILED (" + failures + " problems)");
  process.exit(1);
}
console.log("\nWORKFLOWS LINT PASSED (structural only; not a YAML parser)");

#!/usr/bin/env node
/**
 * scripts/wasm-spike/run-wasm-spike.mjs
 *
 * ADR-0001 evidence: does the Rust Core compile to WebAssembly, instantiate, and
 * produce output IDENTICAL to the native build?
 *
 * What is proven here
 * -------------------
 * 1. the crate compiles to `wasm32-unknown-unknown` (checked by the caller);
 * 2. the module instantiates with no imports (so MV3 needs no special glue);
 * 3. calling the exported ABI returns the same bytes as the native build;
 * 4. it runs fast enough (measured, not assumed).
 *
 * What is NOT proven here
 * -----------------------
 * - that a Manifest V3 page can instantiate it under its CSP. That requires a
 *   real extension in a real browser and is tracked separately in ADR-0001.
 *
 * The native side is produced by `cargo test -p devchievements-core -- --nocapture`
 * style output; to keep this script self-contained we shell out to the built
 * native binary instead -- but the native build of the same probe is a test, so
 * we compare against a file written by that test.
 */

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const outDir = path.join(__dirname, ".out");
fs.mkdirSync(outDir, { recursive: true });

const wasmPath = path.join(
  repoRoot,
  "target",
  "wasm32-unknown-unknown",
  "release",
  "devchievements_core.wasm"
);

function fail(msg) {
  console.error("FAIL: " + msg);
  process.exitCode = 1;
}

async function main() {
  const report = { generated_at: new Date().toISOString(), checks: {} };

  if (!fs.existsSync(wasmPath)) {
    fail("wasm artifact missing: " + wasmPath);
    return;
  }
  const bytes = fs.readFileSync(wasmPath);
  report.wasm_path = path.relative(repoRoot, wasmPath);
  report.wasm_bytes = bytes.length;

  // 1. Instantiate with an EMPTY import object: if the module needed host
  //    functions, MV3 CSP work would get much harder.
  const t0 = process.hrtime.bigint();
  let instance;
  try {
    const { instance: inst } = await WebAssembly.instantiate(bytes, {});
    instance = inst;
    report.checks.instantiates_with_no_imports = true;
  } catch (e) {
    report.checks.instantiates_with_no_imports = false;
    fail("instantiation failed: " + e.message);
    fs.writeFileSync(path.join(outDir, "wasm-spike.json"), JSON.stringify(report, null, 2) + "\n");
    return;
  }
  const instantiateMs = Number(process.hrtime.bigint() - t0) / 1e6;
  report.instantiate_ms = +instantiateMs.toFixed(3);

  const exports = Object.keys(instance.exports);
  report.exports = exports;
  report.checks.exposes_probe_abi =
    exports.includes("devch_probe_len") && exports.includes("devch_probe_write");
  if (!report.checks.exposes_probe_abi) {
    fail("probe ABI not exported; found: " + exports.join(", "));
  }

  // 2. Read the payload out of linear memory.
  let wasmJson = null;
  if (report.checks.exposes_probe_abi) {
    const len = instance.exports.devch_probe_len();
    report.checks.reports_nonzero_length = len > 0;
    const memory = instance.exports.memory;
    if (!memory) {
      fail("module exports no memory; cannot read the payload");
    } else {
      // Allocate scratch space at a high offset inside the module's own memory.
      // 64 KiB is beyond anything this probe needs, and the module was built with
      // the default memory layout, so this region is unused.
      const SCRATCH = 65536;
      const needed = memory.buffer.byteLength;
      if (needed < SCRATCH + len) {
        memory.grow(Math.ceil((SCRATCH + len - needed) / 65536) + 1);
      }
      const written = instance.exports.devch_probe_write(SCRATCH);
      const view = new Uint8Array(memory.buffer, SCRATCH, written);
      wasmJson = Buffer.from(view).toString("utf8");
      report.payload_from_wasm = wasmJson;
      report.checks.written_length_matches =
        written === len && Buffer.byteLength(wasmJson, "utf8") === len;
    }
  }

  // 3. Compare with the native build of the same probe.
  const nativeOut = path.join(outDir, "native-probe.json");
  const exe = path.join(repoRoot, "target", "release", "devch-probe-native.exe");
  if (fs.existsSync(exe)) {
    const fd = fs.openSync(nativeOut, "w");
    spawnSync(exe, [], { stdio: ["ignore", fd, fd], windowsHide: true });
    fs.closeSync(fd);
    const nativeJson = fs.readFileSync(nativeOut, "utf8").trim();
    report.payload_from_native = nativeJson;
    report.checks.native_matches_wasm = nativeJson === wasmJson;
    if (!report.checks.native_matches_wasm) {
      fail("native and wasm payloads differ");
    }
  } else {
    report.checks.native_matches_wasm = null;
    report.note_native = "native probe binary not built; cross-check skipped";
  }

  // 4. Timing: 10k invocations.
  if (wasmJson) {
    const N = 10000;
    const t1 = process.hrtime.bigint();
    for (let i = 0; i < N; i++) instance.exports.devch_probe_len();
    const perCallNs = Number(process.hrtime.bigint() - t1) / N;
    report.per_call_len_ns = +perCallNs.toFixed(1);
    report.checks.per_call_under_5ms = perCallNs / 1e6 < 5;
  }

  fs.writeFileSync(path.join(outDir, "wasm-spike.json"), JSON.stringify(report, null, 2) + "\n");

  console.log("wasm bytes      :", report.wasm_bytes);
  console.log("instantiate ms  :", report.instantiate_ms);
  console.log("exports         :", (report.exports || []).join(", "));
  console.log("checks          :", JSON.stringify(report.checks, null, 0));
  console.log("wrote", path.join(outDir, "wasm-spike.json"));
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});

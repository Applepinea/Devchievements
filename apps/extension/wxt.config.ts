import { readFileSync } from "node:fs";
import { defineConfig } from "wxt";

// The public key pins the extension id for unpacked builds. Without it the id is
// derived from the install path, so allowed_origins in the Native Messaging host
// manifest could never match a stable value.
const extensionKey = readFileSync(new URL("./extension-key.pub", import.meta.url), "utf8").trim();

/**
 * Devchievements extension — Phase 0 scaffold.
 *
 * SCOPE RIGHT NOW: prove that WXT + Manifest V3 builds a loadable extension and
 * that a MV3 page can instantiate the Rust Core WASM module under a CSP that
 * allows `wasm-unsafe-eval` (spec 3.3 / 30.2). Nothing here implements product
 * behaviour yet: Phase 3a is gated behind checkpoint B.
 *
 * WHY THE CSP IS EXPLICIT: Manifest V3 forbids `unsafe-eval` by default, and
 * WebAssembly.instantiate() is treated as eval-like. So the extension_manifest
 * MUST declare `'wasm-unsafe-eval'`; relying on defaults silently breaks WASM.
 */
export default defineConfig({
  modules: ["@wxt-dev/module-react"],
  manifest: {
    name: "Devchievements",
    description: "Achievements for Developers",
    version: "0.0.0",
    key: extensionKey,
    // No <all_urls>: spec 27.2 limits us to verifiable GitHub activity.
    // nativeMessaging is what makes chrome.runtime.connectNative exist at all;
    // without it the bridge to the desktop companion silently cannot be built.
    permissions: ["storage", "notifications", "nativeMessaging"],
    host_permissions: ["https://github.com/*"],
    // Explicit CSP so 'wasm-unsafe-eval' is provably present and testable.
    content_security_policy: {
      extension_pages:
        "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'",
    },
    // Distinct dev vs store IDs matter for Native Messaging allowlisting
    // (spec 18.4). The dev key is generated locally and never used for release.
    // Content scripts are declared by WXT from entrypoints/*.content.ts. Hand-
    // writing the manifest entry pointed at a file the build never produced, so the
    // browser refused to install the extension at all (measured: "cannot load
    // JavaScript content-scripts/github.js").
  },
});

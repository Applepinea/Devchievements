#!/usr/bin/env node
/**
 * scripts/dev/unregister-native-host.mjs
 *
 * Removes what register-native-host.mjs wrote.
 *
 * Kept separate and explicit because registration touches a real, user-scoped part of
 * the system (HKCU and the local data directory). A test that leaves those behind is
 * a test that changes the machine silently.
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const hostName = "com.devchievements.host";

function main() {
  const local = process.env.LOCALAPPDATA;
  if (!local) throw new Error("LOCALAPPDATA is not set");

  const manifestPath = path.join(local, "devchievements", "native", hostName + ".json");
  const keys = [
    "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\" + hostName,
    "HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\" + hostName,
  ];

  let removed = 0;
  for (const key of keys) {
    try {
      execFileSync("reg.exe", ["delete", key, "/f"], { stdio: "ignore" });
      removed += 1;
    } catch {
      // Already absent: that is the desired end state, not a failure.
    }
  }

  if (fs.existsSync(manifestPath)) fs.rmSync(manifestPath, { force: true });

  console.log("removed " + removed + " registry key(s)");
  console.log("manifest: " + (fs.existsSync(manifestPath) ? "still present" : "removed"));
}

main();

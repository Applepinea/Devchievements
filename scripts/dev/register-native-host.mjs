#!/usr/bin/env node
/**
 * scripts/dev/register-native-host.mjs
 *
 * Registers the Native Messaging host with Chrome and Edge.
 *
 * What has to line up, and why each piece exists:
 *  - the extension id must be stable, or allowed_origins can never match; that id
 *    comes from the public key committed next to the extension;
 *  - the host manifest must contain the absolute path of the host binary, because the
 *    browser launches it directly;
 *  - the registry entry is how the browser finds the manifest at all.
 *
 * Everything is written under HKCU: no administrator rights, no machine-wide change.
 */

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const hostName = "com.devchievements.host";

function extensionId() {
  const keyPath = path.join(repoRoot, "apps", "extension", "extension-key.pub");
  const der = Buffer.from(fs.readFileSync(keyPath, "utf8").trim(), "base64");
  const digest = crypto.createHash("sha256").update(der).digest();
  return [...digest.subarray(0, 16)]
    .flatMap((b) => [b >> 4, b & 0x0f])
    .map((n) => String.fromCharCode(97 + n))
    .join("");
}

function dataDir() {
  const local = process.env.LOCALAPPDATA;
  if (!local) throw new Error("LOCALAPPDATA is not set");
  return path.join(local, "devchievements");
}

function main() {
  const hostExe = path.join(repoRoot, "target", "debug", "devchievements-native-host.exe");
  if (!fs.existsSync(hostExe)) {
    console.error("missing host binary: " + hostExe);
    process.exit(1);
  }

  const id = extensionId();
  const dir = path.join(dataDir(), "native");
  fs.mkdirSync(dir, { recursive: true });
  const manifestPath = path.join(dir, hostName + ".json");

  const manifest = {
    name: hostName,
    description: "Devchievements native messaging host",
    path: hostExe,
    type: "stdio",
    allowed_origins: ["chrome-extension://" + id + "/"],
  };
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

  const browsers = [
    "HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\" + hostName,
    "HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\" + hostName,
  ];
  for (const key of browsers) {
    execFileSync("reg.exe", ["add", key, "/ve", "/t", "REG_SZ", "/d", manifestPath, "/f"], {
      stdio: "ignore",
    });
  }

  console.log("extension id: " + id);
  console.log("manifest:     " + manifestPath);
  console.log("registered:   " + browsers.length + " browser(s)");
}

main();

#!/usr/bin/env node
/**
 * Search the MSYS2 mirror listing for packages whose name matches a pattern.
 *
 * The mirror index is a plain HTML listing; we page through it with Range
 * requests because the whole file is tens of megabytes.
 */
import https from "node:https";

const pattern = process.argv[2] || "gettext";

function getRange(url, start, end) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { Range: `bytes=${start}-${end}`, "user-agent": "devchievements-dev-env" }, timeout: 60000 }, (res) => {
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks).toString("utf8") }));
    });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
  });
}

const URL = "https://repo.msys2.org/mingw/mingw64/";
const found = new Set();
const CHUNK = 4 * 1024 * 1024;
let start = 0;
let total = null;

for (let i = 0; i < 40; i++) {
  const end = start + CHUNK - 1;
  const r = await getRange(URL, start, end);
  if (r.status !== 206 && r.status !== 200) {
    console.error("range request failed: " + r.status);
    break;
  }
  for (const m of r.body.matchAll(/href="([^"]*\.pkg\.tar\.zst)"/g)) {
    if (m[1].includes(pattern) && !m[1].includes(".sig")) found.add(m[1]);
  }
  const consumed = r.body.length;
  if (consumed === 0) break;
  start += consumed;
  if (consumed < CHUNK) break; // reached the end
}

console.log("matches for '" + pattern + "': " + found.size);
for (const n of [...found].sort()) console.log("  " + n);

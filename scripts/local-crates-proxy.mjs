#!/usr/bin/env node
/**
 * local-crates-proxy.mjs
 *
 * WHY THIS EXISTS (host workaround, NOT part of the product)
 * ---------------------------------------------------------
 * This development host has a broken Windows schannel credential chain:
 * every CLI that performs TLS through schannel fails with
 *
 *     SEC_E_NO_CREDENTIALS (0x8009030e)
 *
 * which breaks cargo's registry access (cargo links libcurl + schannel on
 * Windows). Node.js, however, uses its own OpenSSL stack and works fine.
 *
 * This script runs a *local sparse registry* on 127.0.0.1 that:
 *   - serves the crates.io sparse index (proxied through Node's https),
 *   - serves its own config.json whose "dl" points back at itself,
 *   - downloads .crate files from static.crates.io through Node's https.
 *
 * cargo is then pointed at it with a source replacement (see
 * scripts/cargo-local-registry.ps1), so cargo never opens a schannel socket.
 *
 * SECURITY / SCOPE
 * ----------------
 * - Binds to 127.0.0.1 only.
 * - Only proxies GET to index.crates.io and static.crates.io. Nothing else.
 * - Never stores credentials. Crates are cached on disk under .crate-cache/.
 * - This file must never be shipped in a product build.
 *
 * USAGE
 * -----
 *   node scripts/local-crates-proxy.mjs [--port 8787] [--cache <dir>]
 *
 * Then, in another shell:
 *   . .\scripts\cargo-local-registry.ps1 -Port 8787
 */

import http from "node:http";
import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const out = { port: 8787, cache: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--port") out.port = Number(argv[++i]);
    else if (argv[i] === "--cache") out.cache = argv[++i];
  }
  if (!out.cache) out.cache = path.resolve(__dirname, "..", ".crate-cache");
  return out;
}

const args = parseArgs(process.argv.slice(2));
fs.mkdirSync(args.cache, { recursive: true });

const INDEX_ORIGIN = "https://index.crates.io";
const DL_ORIGIN = "https://static.crates.io";
const SELF = `http://127.0.0.1:${args.port}`;

/** Follow redirects and stream the response body to the client. */
function proxyGet(url, res, redirects = 0) {
  if (redirects > 6) {
    res.writeHead(502, { "content-type": "text/plain" });
    res.end("too many redirects");
    return;
  }
  const lib = url.startsWith("https:") ? https : http;
  const req = lib.get(
    url,
    { timeout: 60000, headers: { "user-agent": "devchievements-local-crates-proxy" } },
    (upstream) => {
      const code = upstream.statusCode ?? 502;
      if (code >= 300 && code < 400 && upstream.headers.location) {
        upstream.resume();
        const next = new URL(upstream.headers.location, url).toString();
        return proxyGet(next, res, redirects + 1);
      }
      const headers = { "content-type": upstream.headers["content-type"] ?? "application/octet-stream" };
      if (upstream.headers["content-length"]) headers["content-length"] = upstream.headers["content-length"];
      res.writeHead(code, headers);
      upstream.pipe(res);
      upstream.on("error", () => res.destroy());
    }
  );
  req.on("timeout", () => req.destroy(new Error("upstream timeout")));
  req.on("error", (err) => {
    if (res.headersSent) return res.destroy();
    res.writeHead(502, { "content-type": "text/plain" });
    res.end("upstream error: " + err.message);
  });
}

function fetchBuffer(url) {
  return new Promise((resolve, reject) => {
    const lib = url.startsWith("https:") ? https : http;
    const req = lib.get(
      url,
      { timeout: 60000, headers: { "user-agent": "devchievements-local-crates-proxy" } },
      (upstream) => {
        const code = upstream.statusCode ?? 502;
        if (code >= 300 && code < 400 && upstream.headers.location) {
          upstream.resume();
          return resolve(fetchBuffer(new URL(upstream.headers.location, url).toString()));
        }
        if (code !== 200) {
          upstream.resume();
          return reject(new Error(`HTTP ${code} for ${url}`));
        }
        const chunks = [];
        upstream.on("data", (c) => chunks.push(c));
        upstream.on("end", () => resolve(Buffer.concat(chunks)));
        upstream.on("error", reject);
      }
    );
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
  });
}

const stats = { index: 0, dl: 0, cached: 0, errors: 0 };

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", SELF);
  const p = url.pathname;

  // 1) sparse registry config: point "dl" back at us
  if (p === "/index/config.json") {
    const body = JSON.stringify({ dl: `${SELF}/dl/{crate}/{version}`, api: null });
    res.writeHead(200, { "content-type": "application/json" });
    res.end(body);
    return;
  }

  // 2) sparse index entries: /index/<prefix path>
  if (p.startsWith("/index/")) {
    stats.index++;
    return proxyGet(INDEX_ORIGIN + p.slice("/index".length), res);
  }

  // 3) crate downloads: /dl/<name>/<version>
  const m = /^\/dl\/([^/]+)\/([^/]+)$/.exec(p);
  if (m) {
    const [, name, version] = m;
    const file = path.join(args.cache, `${name}-${version}.crate`);
    if (fs.existsSync(file)) {
      stats.cached++;
      res.writeHead(200, {
        "content-type": "application/octet-stream",
        "content-length": String(fs.statSync(file).size),
      });
      fs.createReadStream(file).pipe(res);
      return;
    }
    stats.dl++;
    const upstreamUrl = `${DL_ORIGIN}/crates/${name}/${name}-${version}.crate`;
    try {
      const buf = await fetchBuffer(upstreamUrl);
      fs.writeFileSync(file, buf);
      res.writeHead(200, { "content-type": "application/octet-stream", "content-length": String(buf.length) });
      res.end(buf);
    } catch (err) {
      stats.errors++;
      console.error(`[dl] FAILED ${name} ${version}: ${err.message}`);
      res.writeHead(502, { "content-type": "text/plain" });
      res.end("download failed: " + err.message);
    }
    return;
  }

  if (p === "/health") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: true, ...stats }));
    return;
  }

  stats.errors++;
  res.writeHead(404, { "content-type": "text/plain" });
  res.end("not found: " + p);
});

server.listen(args.port, "127.0.0.1", () => {
  console.log(`[local-crates-proxy] listening on ${SELF}`);
  console.log(`[local-crates-proxy] cache dir: ${args.cache}`);
});

setInterval(() => {
  console.log(`[local-crates-proxy] index=${stats.index} dl=${stats.dl} cached=${stats.cached} errors=${stats.errors}`);
}, 15000).unref();

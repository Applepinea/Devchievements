#!/usr/bin/env node
/**
 * scripts/generate-sounds.mjs
 *
 * Generates the SYNTHETIC sounds the app owns.
 *
 * Only the plain-commit acknowledgement is synthesised. The reward sound
 * (achievement.wav) is a supplied asset checked in next to it and is never regenerated
 * here -- overwriting user-provided media with a tone would be a surprising thing for a
 * build script to do.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const outDir = path.join(repoRoot, "apps", "desktop", "assets", "sounds");

const SAMPLE_RATE = 22050;

function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => {
    data.writeInt16LE(Math.round(Math.max(-1, Math.min(1, v)) * 32767), i * 2);
  });
  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + data.length, 4);
  header.write("WAVE", 8, "ascii");
  header.write("fmt ", 12, "ascii");
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(SAMPLE_RATE, 24);
  header.writeUInt32LE(SAMPLE_RATE * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36, "ascii");
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
}

/**
 * A short, quiet blip.
 *
 * The envelope avoids the click a hard start/stop produces; the low gain is deliberate
 * because this fires on every single commit.
 */
function blip({ hz, ms, gain }) {
  const total = Math.round((ms / 1000) * SAMPLE_RATE);
  const attack = Math.round(0.008 * SAMPLE_RATE);
  const release = Math.round(0.04 * SAMPLE_RATE);
  const out = new Float32Array(total);
  for (let i = 0; i < total; i++) {
    const v = Math.sin((2 * Math.PI * hz * i) / SAMPLE_RATE);
    let env = 1;
    if (i < attack) env = i / attack;
    else if (i > total - release) env = Math.max(0, (total - i) / release);
    out[i] = v * env * gain;
  }
  return out;
}

fs.mkdirSync(outDir, { recursive: true });

const commitBytes = wav(blip({ hz: 420, ms: 55, gain: 0.22 }));
fs.writeFileSync(path.join(outDir, "commit.wav"), commitBytes);

console.log("wrote commit.wav (" + (commitBytes.length / 1024).toFixed(1) + " KiB)");
console.log("reward sound is a checked-in asset and is not regenerated");

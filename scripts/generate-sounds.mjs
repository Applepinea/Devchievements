#!/usr/bin/env node
/**
 * scripts/generate-sounds.mjs
 *
 * Generates the built-in notification sounds (spec 19: Phase 2a needs built-in
 * defaults only).
 *
 * WHY GENERATED: shipping hand-picked audio would need licensing and reviewing for a
 * placeholder. These are short synthesised tones -- enough to prove the selection and
 * playback path end to end, and trivially replaceable later by the Sound Manager.
 *
 * The tones are deliberately distinguishable: higher rarity is higher and longer, and
 * daily-rank tones use a squarer waveform than rarity tones so the two families can be
 * told apart by ear.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const outDir = path.join(repoRoot, "apps", "desktop", "assets", "sounds");

const SAMPLE_RATE = 22050;

/** Build a mono 16-bit PCM WAV. */
function wav(samples) {
  const data = Buffer.alloc(samples.length * 2);
  samples.forEach((v, i) => {
    const clamped = Math.max(-1, Math.min(1, v));
    data.writeInt16LE(Math.round(clamped * 32767), i * 2);
  });

  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + data.length, 4);
  header.write("WAVE", 8, "ascii");
  header.write("fmt ", 12, "ascii");
  header.writeUInt32LE(16, 16);          // PCM chunk size
  header.writeUInt16LE(1, 20);           // format = PCM
  header.writeUInt16LE(1, 22);           // channels
  header.writeUInt32LE(SAMPLE_RATE, 24);
  header.writeUInt32LE(SAMPLE_RATE * 2, 28);
  header.writeUInt16LE(2, 32);           // block align
  header.writeUInt16LE(16, 34);          // bits
  header.write("data", 36, "ascii");
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
}

/**
 * One tone with a smooth envelope.
 *
 * The envelope matters: a hard start and stop produces a click that sounds like a
 * broken install. 8 ms attack and 40 ms release are enough to avoid it.
 */
function tone({ hz, ms, square = false, gain = 0.35 }) {
  const total = Math.round((ms / 1000) * SAMPLE_RATE);
  const attack = Math.round(0.008 * SAMPLE_RATE);
  const release = Math.round(0.04 * SAMPLE_RATE);
  const out = new Float32Array(total);

  for (let i = 0; i < total; i++) {
    const t = i / SAMPLE_RATE;
    const phase = 2 * Math.PI * hz * t;
    let v = square ? Math.sign(Math.sin(phase)) * 0.6 : Math.sin(phase);

    let env = 1;
    if (i < attack) env = i / attack;
    else if (i > total - release) env = Math.max(0, (total - i) / release);
    out[i] = v * env * gain;
  }
  return out;
}

/** Two tones in sequence, for the higher rarities. */
function twoTone(a, b) {
  const first = tone(a);
  const second = tone(b);
  const out = new Float32Array(first.length + second.length);
  out.set(first, 0);
  out.set(second, first.length);
  return out;
}

const sounds = {
  // Fallback and plain commit.
  default: tone({ hz: 520, ms: 90 }),
  commit: tone({ hz: 420, ms: 55, gain: 0.22 }),

  // Rarity family: rising pitch and length with rarity.
  "rarity-common": tone({ hz: 523, ms: 90 }),
  "rarity-uncommon": tone({ hz: 587, ms: 110 }),
  "rarity-rare": tone({ hz: 659, ms: 130 }),
  "rarity-epic": twoTone({ hz: 784, ms: 110 }, { hz: 988, ms: 130 }),
  "rarity-legendary": twoTone({ hz: 880, ms: 120 }, { hz: 1319, ms: 180 }),

  // Daily-rank family: squarer timbre so it is clearly a different event.
  "daily-common": tone({ hz: 392, ms: 80, square: true }),
  "daily-uncommon": tone({ hz: 440, ms: 90, square: true }),
  "daily-rare": tone({ hz: 494, ms: 100, square: true }),
  "daily-epic": tone({ hz: 587, ms: 120, square: true }),
  "daily-legendary": twoTone({ hz: 659, ms: 110, square: true }, { hz: 784, ms: 140, square: true }),
};

fs.mkdirSync(outDir, { recursive: true });
let total = 0;
for (const [name, samples] of Object.entries(sounds)) {
  const bytes = wav(samples);
  fs.writeFileSync(path.join(outDir, name + ".wav"), bytes);
  total += bytes.length;
}
console.log("wrote " + Object.keys(sounds).length + " sounds, " + (total / 1024).toFixed(1) + " KiB total");
console.log("dir: " + path.relative(repoRoot, outDir));

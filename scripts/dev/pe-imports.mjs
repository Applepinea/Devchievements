#!/usr/bin/env node
/**
 * Print the DLL import names of a PE file.
 *
 * Needed because a Windows process that dies with 0xC0000135 (STATUS_DLL_NOT_FOUND)
 * reports nothing on stderr, so we have to read the import table ourselves.
 */
import fs from "node:fs";

const file = process.argv[2];
const b = fs.readFileSync(file);
if (b.toString("ascii", 0, 2) !== "MZ") throw new Error("not a PE");

const peOff = b.readUInt32LE(0x3c);
const machine = b.readUInt16LE(peOff + 4);
const numSections = b.readUInt16LE(peOff + 6);
const sizeOptHdr = b.readUInt16LE(peOff + 20);
const optOff = peOff + 24;
const magic = b.readUInt16LE(optOff);
const pe32plus = magic === 0x20b;

const ddOff = optOff + (pe32plus ? 112 : 96);
const importRva = b.readUInt32LE(ddOff + 8);
const importSize = b.readUInt32LE(ddOff + 12);

const sections = [];
let sOff = optOff + sizeOptHdr;
for (let i = 0; i < numSections; i++) {
  const o = sOff + i * 40;
  sections.push({
    name: b.toString("ascii", o, o + 8).replace(/\0+$/, ""),
    vsize: b.readUInt32LE(o + 8),
    vaddr: b.readUInt32LE(o + 12),
    rawSize: b.readUInt32LE(o + 16),
    rawPtr: b.readUInt32LE(o + 20),
  });
}

function rvaToOffset(rva) {
  for (const s of sections) {
    if (rva >= s.vaddr && rva < s.vaddr + Math.max(s.vsize, s.rawSize)) {
      return s.rawPtr + (rva - s.vaddr);
    }
  }
  return -1;
}

const dlls = [];
let desc = rvaToOffset(importRva);
while (desc > 0 && desc + 20 <= b.length) {
  const nameRva = b.readUInt32LE(desc + 12);
  if (nameRva === 0) break;
  const nameOff = rvaToOffset(nameRva);
  if (nameOff < 0) break;
  let end = nameOff;
  while (b[end] !== 0) end++;
  dlls.push(b.toString("ascii", nameOff, end));
  desc += 20;
}

console.log(JSON.stringify({ machine: "0x" + machine.toString(16), pe32plus, importSize, dlls }, null, 2));

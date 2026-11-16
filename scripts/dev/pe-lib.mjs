/**
 * Minimal PE import-table reader.
 *
 * WHY IN-PROCESS: the DSH sandbox denies pipe-based stdio capture for nested
 * processes, so calling out to a helper script silently produced empty results
 * (that bug hid a real dependency failure for one full round). Keeping the parser
 * in the same process removes the failure mode entirely.
 */

import fs from "node:fs";

const SYSTEM_DLLS = new Set([
  "KERNEL32.dll", "ADVAPI32.dll", "msvcrt.dll", "USER32.dll", "SHELL32.dll",
  "ole32.dll", "oleaut32.dll", "WS2_32.dll", "bcrypt.dll", "ntdll.dll",
  "api-ms-win-crt-runtime-l1-1-0.dll",
]);

/** Return { machine, pe32plus, dlls } for a PE file, or null if it is not a PE. */
export function readPe(file) {
  let b;
  try {
    b = fs.readFileSync(file);
  } catch {
    return null;
  }
  if (b.length < 0x40 || b.toString("ascii", 0, 2) !== "MZ") return null;

  const peOff = b.readUInt32LE(0x3c);
  if (b.toString("ascii", peOff, peOff + 2) !== "PE") return null;

  const machine = b.readUInt16LE(peOff + 4);
  const numSections = b.readUInt16LE(peOff + 6);
  const sizeOptHdr = b.readUInt16LE(peOff + 20);
  const optOff = peOff + 24;
  const pe32plus = b.readUInt16LE(optOff) === 0x20b;

  const ddOff = optOff + (pe32plus ? 112 : 96);
  const importRva = b.readUInt32LE(ddOff + 8);

  const sections = [];
  const sOff = optOff + sizeOptHdr;
  for (let i = 0; i < numSections; i++) {
    const o = sOff + i * 40;
    sections.push({
      vsize: b.readUInt32LE(o + 8),
      vaddr: b.readUInt32LE(o + 12),
      rawSize: b.readUInt32LE(o + 16),
      rawPtr: b.readUInt32LE(o + 20),
    });
  }

  const rvaToOffset = (rva) => {
    for (const s of sections) {
      if (rva >= s.vaddr && rva < s.vaddr + Math.max(s.vsize, s.rawSize)) {
        return s.rawPtr + (rva - s.vaddr);
      }
    }
    return -1;
  };

  const dlls = [];
  let desc = rvaToOffset(importRva);
  while (desc > 0 && desc + 20 <= b.length) {
    const nameRva = b.readUInt32LE(desc + 12);
    if (nameRva === 0) break;
    const nameOff = rvaToOffset(nameRva);
    if (nameOff < 0) break;
    let end = nameOff;
    while (end < b.length && b[end] !== 0) end++;
    dlls.push(b.toString("ascii", nameOff, end));
    desc += 20;
  }

  return { machine: "0x" + machine.toString(16), pe32plus, dlls };
}

/** DLLs this PE needs that do not ship with Windows. */
export function nonSystemImports(file) {
  const pe = readPe(file);
  if (!pe) return [];
  return pe.dlls.filter((d) => !SYSTEM_DLLS.has(d) && !SYSTEM_DLLS.has(d.toLowerCase()));
}

export { SYSTEM_DLLS };

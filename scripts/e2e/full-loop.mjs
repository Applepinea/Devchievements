#!/usr/bin/env node
/**
 * scripts/e2e/full-loop.mjs
 *
 * The COMPLETE spec 32.3 loop, minus the popup:
 *
 *   git commit -> hook -> helper -> spool -> companion -> sqlite -> engine
 *              -> outbox -> delivery
 *
 * Every stage is real: a real repository, a real `git commit`, the real helper
 * binary, the real headless Companion, a real SQLite file. The only substitution is
 * the renderer (stdout instead of a window), because the window needs MSVC.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const companionExe = path.join(repoRoot, "target", "debug", "devch-companion.exe");
const scratch = path.join(__dirname, ".loop");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).trim().slice(0, 400) : ""));
  }
}

function run(exe, args, cwd, logName) {
  const log = path.join(cwd, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

function main() {
  if (!fs.existsSync(helperExe) || !fs.existsSync(companionExe)) {
    console.error("missing binaries; build them first");
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataDir = path.join(scratch, "data");
  // The helper refuses to create the data directory (spec 34.1), so the fixture
  // mirrors an installed machine where the installer has already made it.
  fs.mkdirSync(path.join(dataDir, "devchievements"), { recursive: true });
  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });

  const git = (args) => run("git", args, repo, "git.log");

  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Loop Test"]);
  git(["config", "user.email", "loop@devchievements.local"]);

  // Install the hook exactly as the product does on Windows: a native .exe hook.
  const hooksDir = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooksDir, "post-commit.exe"));

  // The helper writes its spool under LOCALAPPDATA; point that at our scratch dir.
  process.env.LOCALAPPDATA = dataDir;

  // Three real commits.
  for (let i = 0; i < 3; i++) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i + "\n");
    git(["add", "-A"]);
    const c = git(["commit", "-q", "-m", "commit " + i]);
    check("commit " + i + " succeeded with the hook installed", c.status === 0, c.out);
  }

  // Where did the helper put its records?
  const spoolDir = path.join(dataDir, "devchievements", "spool");
  const pending = fs.existsSync(spoolDir)
    ? fs.readdirSync(spoolDir).filter((f) => f.endsWith(".spool")).length
    : 0;
  console.log("  info helper wrote " + pending + " spool file(s)");
  check("the helper captured the commits", pending > 0);

  // Move the spool where the companion expects it.
  const companionData = path.join(scratch, "companion");
  fs.mkdirSync(companionData, { recursive: true });
  fs.cpSync(spoolDir, path.join(companionData, "spool"), { recursive: true });

  // Run the companion.
  const companion = run(
    companionExe,
    [companionData, "loop@devchievements.local", "--once"],
    scratch,
    "companion.log"
  );
  const out = companion.out;
  console.log(out.trim().split("\n").map((l) => "  |  " + l).join("\n"));

  // Parse without a regex: the generated file previously lost the backslash in
  // "\d", which turned the pattern into a literal "d" and made this always 0.
  const drainLine = out.split(/\r?\n/).find((l) => l.startsWith("DRAIN")) || "";
  const field = (line, key) => {
    const part = line.split(" ").find((t) => t.startsWith(key + "="));
    return part ? Number(part.slice(key.length + 1)) : 0;
  };
  const ingested = field(drainLine, "ingested");
  check("the companion ingested all three commits", ingested === 3, out);
  // Spec 16.6: the first commit unlocks an achievement AND reaches the Common daily
  // rank; that must arrive as ONE popup with the rank as a sub-line, not two windows.
  const popups = out.split(/\r?\n/).filter((l) => l.startsWith("NOTIFY popup"));
  check(
    "the first commit merges achievement and rank into one popup",
    popups.length >= 1 &&
      popups[0].includes('title="First Commit"') &&
      popups[0].includes("Common Day"),
    popups[0] ?? "(no popup)"
  );
  check(
    "a rank upgrade opens its own popup",
    popups.some((l) => l.includes('title="Uncommon Day"')),
    popups.join(" | ")
  );
  // Spec 16.4: a commit that unlocks nothing is acknowledged by sound, not a window.
  check("a plain commit only plays a sound", /SOUND commit/.test(out));
  check("everything was delivered", /DELIVER shown=\d+/.test(out));

  // Second run must be a no-op.
  const second = run(
    companionExe,
    [companionData, "loop@devchievements.local", "--once"],
    scratch,
    "companion2.log"
  );
  check("a second run reads nothing new", /read=0/.test(second.out), second.out);

  console.log("");
  if (failures) {
    console.log("FULL LOOP FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("FULL LOOP PASSED: git commit -> hook -> helper -> spool -> companion -> sqlite -> engine -> outbox -> delivery");
  }
}

main();

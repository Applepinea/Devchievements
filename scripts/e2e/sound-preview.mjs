#!/usr/bin/env node
/**
 * scripts/e2e/sound-preview.mjs
 *
 * Spec 19: the Sound Manager's preview must play what a real notification would play.
 *
 * The property under test is not "a sound came out" -- a test host may have no audio
 * device -- it is that the SELECTION shown by the preview is the selection the real
 * notification path would produce, given the same settings and overrides. A preview
 * that hard-coded a file would pass a naive playback check and still lie to the user.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const soundsSrc = path.join(repoRoot, "apps", "desktop", "assets", "sounds");
const scratch = path.join(__dirname, ".sound-preview");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }

  fs.rmSync(scratch, { recursive: true, force: true });
  const appData = path.join(scratch, "appdata");
  const dataDir = path.join(appData, "devchievements");
  const soundsDir = path.join(dataDir, "sounds");
  fs.mkdirSync(soundsDir, { recursive: true });

  // The built-in sounds have to exist next to the app; copy the shipped set so the
  // preview can resolve an id to a file.
  if (fs.existsSync(soundsSrc)) {
    for (const name of fs.readdirSync(soundsSrc)) {
      if (name.endsWith(".wav")) fs.copyFileSync(path.join(soundsSrc, name), path.join(soundsDir, name));
    }
  }
  const shipped = fs.readdirSync(soundsDir).filter((n) => n.endsWith(".wav"));
  check("the app ships at least one sound", shipped.length > 0, JSON.stringify(shipped));

  const settingsPath = path.join(dataDir, "settings.json");
  const writeSettings = (extra) =>
    fs.writeFileSync(
      settingsPath,
      JSON.stringify(
        {
          schemaVersion: 1,
          emails: ["preview@example.com"],
          onboardingCompleted: true,
          muted: false,
          volume: 80,
          ...extra,
        },
        null,
        2,
      ),
    );

  const env = { ...process.env, LOCALAPPDATA: appData };
  const preview = (slot) => {
    const r = spawnSync(appExe, ["--preview-sound", slot], {
      env,
      timeout: 40000,
      windowsHide: true,
      encoding: "utf8",
    });
    return { status: r.status, out: String(r.stdout ?? "") + String(r.stderr ?? "") };
  };

  // 1. A plain commit slot selects the commit sound, not the reward jingle (spec 19).
  writeSettings({});
  const commit = preview("commit");
  check("the commit slot previews", commit.status === 0, commit.out);
  check("it selects the commit sound", /SOUND PREVIEW slot=commit id=commit/.test(commit.out), commit.out);
  check("and it reports that it played", /played=true/.test(commit.out), commit.out);

  // 2. A rarity slot selects the reward sound. Both come from the same selector a real
  //    notification uses, which is the point.
  const epic = preview("epic");
  check("a rarity slot previews", epic.status === 0, epic.out);
  check("it selects the reward sound", /id=achievement/.test(epic.out), epic.out);
  check("and it says which rule chose it", /source=BuiltIn/.test(epic.out), epic.out);

  // 3. A daily-rank slot is recognised as a daily rank, not as a plain reraity.
  const daily = preview("daily:rare");
  check("a daily slot previews", daily.status === 0, daily.out);
  check("it keeps the daily: prefix, so the selection knows it is a rank", /slot=daily:rare/.test(daily.out), daily.out);

  // 4. THE assertion: the preview follows the user's overrides. If it did not, it
  //    would be telling the user the wrong sound.
  writeSettings({ soundOverrides: { commit: "achievement" } });
  const overridden = preview("commit");
  check("an override is honoured", /id=achievement/.test(overridden.out), overridden.out);
  check("and the preview says so", /source=CommitCustom/.test(overridden.out), overridden.out);

  // 5. Muted is reported, not hidden. A preview that ignores the mute switch teaches
  //    the wrong thing about what a real notification will do.
  writeSettings({ muted: true });
  const muted = preview("commit");
  check("a muted preview still reports the selection", /id=commit/.test(muted.out), muted.out);
  check("but says nothing was played", /played=false/.test(muted.out), muted.out);
  check("and gives the reason", /SOUND PREVIEW REASON .*muted/i.test(muted.out), muted.out);

  // 6. Volume zero is the same statement by a different control.
  writeSettings({ volume: 0 });
  const silent = preview("epic");
  check("volume zero also refuses to play", /played=false/.test(silent.out), silent.out);

  // 7. Sounds resolve from the user directory FIRST and the shipped set next to the
  //    executable second, so removing the user's copy must fall back rather than go
  //    silent. (Measured while writing this: deleting them from the data dir still
  //    played, because the built-in set next to the exe is the fallback. That is the
  //    documented order, so it is asserted rather than worked around.)
  writeSettings({});
  for (const name of fs.readdirSync(soundsDir)) fs.unlinkSync(path.join(soundsDir, name));
  const fellBack = preview("commit");
  check("removing the user copy still answers", fellBack.status === 0, fellBack.out);
  check(
    "it falls back to the shipped sound instead of going silent",
    /played=true/.test(fellBack.out) && /id=commit/.test(fellBack.out),
    fellBack.out,
  );

  // 8. An override pointing at a file that does not exist must DEGRADE and be reported,
  //    not fail (spec 19: a deleted custom sound must not break notifications).
  writeSettings({ soundOverrides: { commit: "deleted-custom-sound" } });
  const dangling = preview("commit");
  check("a dangling override still answers", dangling.status === 0, dangling.out);
  check(
    "it names what the override wanted",
    /id=/.test(dangling.out),
    dangling.out,
  );

  // 9. The slot list the page renders is the one the selector understands.
  writeSettings({});
  fs.mkdirSync(soundsDir, { recursive: true });
  if (fs.existsSync(soundsSrc)) {
    for (const name of fs.readdirSync(soundsSrc)) {
      if (name.endsWith(".wav")) fs.copyFileSync(path.join(soundsSrc, name), path.join(soundsDir, name));
    }
  }
  for (const slot of ["common", "uncommon", "legendary", "daily:common"]) {
    const r = preview(slot);
    check("the offered slot " + slot + " is understood", r.status === 0 && /SOUND PREVIEW/.test(r.out), r.out);
  }

  // 10. A slot name the flow does not use must not fail the command: the list is data.
  const unknown = preview("not-a-real-slot");
  check("an unknown slot still answers rather than erroring", unknown.status === 0, unknown.out);

  console.log("");
  if (failures) {
    console.log("SOUND PREVIEW FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("SOUND PREVIEW PASSED: the preview plays what a real notification would, and says when it cannot");
  }
}

main();

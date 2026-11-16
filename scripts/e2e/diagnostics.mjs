#!/usr/bin/env node
/**
 * scripts/e2e/diagnostics.mjs
 *
 * Verifies the diagnostics snapshot against a database the app actually filled.
 *
 * The point is not that the command prints something -- it is that the numbers match
 * reality and that repository identifiers come out aliased (spec 26.2).
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appExe = path.join(repoRoot, "apps", "desktop", "src-tauri", "target", "debug", "devchievements-desktop.exe");
const helperExe = path.join(repoRoot, "target", "debug", "devch-helper.exe");
const scratch = path.join(__dirname, ".diagnostics");

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 400) : ""));
  }
}

function runFile(exe, args, cwd, logName, env) {
  const log = path.join(scratch, logName);
  const fd = fs.openSync(log, "w");
  const r = spawnSync(exe, args, { cwd, stdio: ["ignore", fd, fd], windowsHide: true, env: env ?? process.env });
  fs.closeSync(fd);
  return { status: r.status, out: fs.readFileSync(log, "utf8") };
}

function main() {
  if (!fs.existsSync(appExe)) {
    console.error("missing desktop binary: " + appExe);
    process.exit(1);
  }
  fs.rmSync(scratch, { recursive: true, force: true });
  fs.mkdirSync(scratch, { recursive: true });

  const dataRoot = path.join(scratch, "data");
  fs.mkdirSync(path.join(dataRoot, "devchievements", "spool"), { recursive: true });
  fs.writeFileSync(
    path.join(dataRoot, "devchievements", "settings.json"),
    JSON.stringify({ schemaVersion: 1, emails: ["diag@devchievements.local"] }, null, 2)
  );

  const repo = path.join(scratch, "repo");
  fs.mkdirSync(repo, { recursive: true });
  // The helper inherits LOCALAPPDATA from the caller, so the commits below must run
  // with the scratch root or they land in the real user profile.
  const env = { ...process.env, LOCALAPPDATA: dataRoot };
  const git = (args) => runFile("git", args, repo, "git.log", env);
  git(["init", "-q", "-b", "main"]);
  git(["config", "user.name", "Diag Test"]);
  git(["config", "user.email", "diag@devchievements.local"]);
  const hooks = path.join(repo, ".git", "hooks");
  fs.mkdirSync(hooks, { recursive: true });
  fs.copyFileSync(helperExe, path.join(hooks, "post-commit.exe"));

  for (let i = 0; i < 3; i++) {
    fs.writeFileSync(path.join(repo, "f" + i + ".txt"), "c" + i + "\n");
    git(["add", "-A"]);
    git(["commit", "-q", "-m", "commit " + i]);
  }

  // Fill the database through the normal path, then ask for a snapshot.
  const drained = runFile(
    path.join(repoRoot, "target", "debug", "devch-companion.exe"),
    [path.join(dataRoot, "devchievements"), "diag@devchievements.local", "--once"],
    scratch,
    "companion.log",
    env
  );
  check("the companion ingested the commits", /ingested=3/.test(drained.out), drained.out);

  const diag = runFile(appExe, ["--diagnostics"], scratch, "diagnostics.json", env);
  check("diagnostics exited successfully", diag.status === 0, diag.out);

  let parsed = null;
  try {
    parsed = JSON.parse(diag.out);
  } catch (e) {
    check("diagnostics printed valid JSON", false, String(e) + "\n" + diag.out.slice(0, 300));
  }
  if (parsed && !parsed.snapshot) {
    check("the snapshot is present", false, diag.out.slice(0, 300));
  }

  if (parsed && parsed.snapshot) {
    check("diagnostics printed valid JSON", true);
    check("it reports 3 events", parsed.snapshot.events === 3, JSON.stringify(parsed.snapshot.events));
    check(
      "it reports one repository",
      Array.isArray(parsed.snapshot.repositories) && parsed.snapshot.repositories.length === 1,
      JSON.stringify(parsed.snapshot.repositories)
    );
    const alias = parsed.snapshot.repositories?.[0]?.alias ?? "";
    check("the repository is aliased, not raw", /^repo-[0-9a-f]+$/.test(alias), alias);
    check("the raw path does not leak", !alias.includes("\\\\") && !alias.includes("/"), alias);
    check("it reports the unlocked achievement", (parsed.snapshot.unlocked_achievements ?? []).includes("first_commit"), JSON.stringify(parsed.snapshot.unlocked_achievements));
    check("it reports the outbox", Array.isArray(parsed.snapshot.outbox) && parsed.snapshot.outbox.length > 0, JSON.stringify(parsed.snapshot.outbox));
    check("it exposes the sound set", (parsed.sounds ?? []).includes("achievement"), JSON.stringify(parsed.sounds));
    check("it exposes the toast state", typeof parsed.toast?.ready === "boolean", JSON.stringify(parsed.toast));
    check("the data directory is redacted", !String(parsed.dataDir).includes("csa"), String(parsed.dataDir));
  }

  console.log("");
  if (failures) {
    console.log("DIAGNOSTICS FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("DIAGNOSTICS PASSED: snapshot matches the real database");
  }
}

main();

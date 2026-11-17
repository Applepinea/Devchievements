#!/usr/bin/env node
/**
 * scripts/msix/build-msix.mjs
 *
 * Packs the built desktop app as an MSIX and proves what a packaged app can and
 * cannot do on this machine.
 *
 * Why MSIX is being tried at all: the Microsoft Store re-signs MSIX packages, so a
 * Store listing needs no code signing certificate, and the Store handles updates.
 * The installer path (NSIS, an .exe) requires the publisher to sign the installer
 * and every PE file inside it, so it cannot use that route for free.
 *
 * The one question this script exists to answer: a packaged app cannot write next
 * to its own executable, and this product writes Git hooks into the user's
 * repositories and keeps its database in a data folder. If either is impossible
 * under MSIX, the Store is not a fit and we should know that from measurement.
 *
 * Usage: node scripts/msix/build-msix.mjs
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..");
const appRoot = path.join(repoRoot, "apps", "desktop", "src-tauri");
const releaseDir = path.join(appRoot, "target", "release");
const outDir = path.join(__dirname, ".out");
const stage = path.join(outDir, "stage");
const markerTemplate = path.join(appRoot, "msix-data-marker.txt");

const SDK_BIN = "C:\\Program Files (x86)\\Windows Kits\\10\\bin";
const IDENTITY_NAME = "DevchievementsCompanion";
const PUBLISHER = "CN=Devchievements";
const PUBLISHER_DISPLAY = "Devchievements";
const VERSION = readVersion();

let failures = 0;
function check(label, ok, detail) {
  if (ok) console.log("  ok   " + label);
  else {
    failures++;
    console.log("  FAIL " + label + (detail ? "\n        " + String(detail).slice(0, 600) : ""));
  }
}

function readVersion() {
  const config = JSON.parse(fs.readFileSync(path.join(appRoot, "tauri.conf.json"), "utf8"));
  // MSIX requires four numeric parts.
  return config.version.split(".").concat(["0"]).slice(0, 4).join(".");
}

function sdkTool(name) {
  if (!fs.existsSync(SDK_BIN)) return null;
  for (const version of fs.readdirSync(SDK_BIN)) {
    const candidate = path.join(SDK_BIN, version, "x64", name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

function run(exe, args, options = {}) {
  const result = spawnSync(exe, args, { windowsHide: true, encoding: "utf8", ...options });
  return { status: result.status, out: (result.stdout || "") + (result.stderr || "") };
}

function xmlEscape(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

const manifest = `<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
         xmlns:uap5="http://schemas.microsoft.com/appx/manifest/uap/windows10/5"
         IgnorableNamespaces="uap uap5 rescap">
  <Identity Name="${IDENTITY_NAME}" Publisher="${xmlEscape(PUBLISHER)}" Version="${VERSION}" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Devchievements</DisplayName>
    <PublisherDisplayName>${PUBLISHER_DISPLAY}</PublisherDisplayName>
    <Logo>Assets\\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="en-us" />
    <Resource Language="zh-cn" />
  </Resources>
  <Applications>
    <Application Id="Devchievements" Executable="devchievements-desktop.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Devchievements" Description="Achievements for Developers"
        BackgroundColor="transparent" Square150x150Logo="Assets\\Square150x150Logo.png"
        Square44x44Logo="Assets\\Square44x44Logo.png" />
      <Extensions>
        <!-- startupTask is an application extension, not a package one: Windows shows
             it in Settings and Task Manager, which is where a packaged app startup
             belongs. Disabled by default, matching the app default. -->
        <uap5:Extension Category="windows.startupTask" Executable="devchievements-desktop.exe" EntryPoint="Windows.FullTrustApplication">
          <uap5:StartupTask TaskId="DevchievementsStartup" Enabled="false" DisplayName="Devchievements" />
        </uap5:Extension>
      </Extensions>
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
`;

function main() {
  const exe = path.join(releaseDir, "devchievements-desktop.exe");
  if (!fs.existsSync(exe)) {
    console.error("missing " + exe + "; build the release first");
    process.exit(1);
  }
  const makeappx = sdkTool("makeappx.exe");
  const signtool = sdkTool("signtool.exe");
  if (!makeappx) {
    console.error("makeappx.exe not found under " + SDK_BIN);
    process.exit(2);
  }

  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(path.join(stage, "Assets"), { recursive: true });

  // Payload: exactly what the NSIS bundle carries.
  for (const file of ["devchievements-desktop.exe", "devch-helper.exe"]) {
    fs.copyFileSync(path.join(releaseDir, file), path.join(stage, file));
  }
  fs.cpSync(path.join(releaseDir, "sounds"), path.join(stage, "sounds"), { recursive: true });
  // The runtime bootstrapper travels inside the package so a machine without
  // WebView2 can be repaired from the error dialog instead of a browser trip.
  // It is copied out before running: nothing inside WindowsApps may be executed
  // without the package identity.
  const bootstrapper = path.join(appRoot, "..", "assets", "webview2", "MicrosoftEdgeWebView2Setup.exe");
  check("the runtime bootstrapper is available", fs.existsSync(bootstrapper), bootstrapper);
  if (fs.existsSync(bootstrapper)) fs.copyFileSync(bootstrapper, path.join(stage, "webview2-setup.exe"));
  fs.writeFileSync(path.join(stage, "AppxManifest.xml"), manifest);
  // The Store checks these sizes, and a placeholder would be visible to users.
  const iconDir = path.join(appRoot, "icons", "store");
  for (const name of [
    "StoreLogo.png",
    "Square44x44Logo.png",
    "Square150x150Logo.png",
    "Square310x310Logo.png",
    "Wide310x150Logo.png",
  ]) {
    const source = path.join(iconDir, name);
    check("icon " + name + " exists", fs.existsSync(source), source);
    if (fs.existsSync(source)) fs.copyFileSync(source, path.join(stage, "Assets", name));
  }

  const msix = path.join(outDir, "Devchievements.msix");
  const packed = run(makeappx, ["pack", "/d", stage, "/p", msix, "/o"]);
  check("makeappx packs the package", packed.status === 0, packed.out);
  if (packed.status !== 0) {
    report();
    return;
  }

  // A package cannot be installed unless its publisher matches the signature, so
  // the test signs it with a throwaway certificate. The Store would re-sign it.
  if (signtool) {
    const pfx = path.join(outDir, "test.pfx");
    const made = run("powershell", [
      "-NoProfile",
      "-Command",
      `$c = New-SelfSignedCertificate -Type Custom -Subject '${PUBLISHER}' -KeyUsage DigitalSignature -FriendlyName DevchievementsMSIXTest -CertStoreLocation 'Cert:\\CurrentUser\\My' -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3'); $p = ConvertTo-SecureString -String 'test' -Force -AsPlainText; Export-PfxCertificate -Cert $c -FilePath '${pfx}' -Password $p | Out-Null; $c.Thumbprint`,
    ]);
    const thumbprint = (made.out || "").trim().split(/\r?\n/).pop();
    const signed = run(signtool, ["sign", "/fd", "SHA256", "/f", pfx, "/p", "test", msix]);
    check("signtool signs the package", signed.status === 0, signed.out);

    // The trust check and the install must happen in the SAME elevated process:
    // trusting the certificate only inside this unelevated session left Add-AppxPackage
    // with 0x800B0109 ("certificate chain not trusted"). One script, one elevation,
    // and the answer is about the package rather than about which session trusted what.
    // Elevation is needed to trust a test certificate and register a package. It is
    // a property of sideloading a self-signed build, not of MSIX: the Store re-signs.
    const helper = path.join(__dirname, "install-msix.ps1");
    const install = run("powershell", [
      "-NoProfile",
      "-Command",
      `Start-Process powershell -Verb RunAs -Wait -PassThru -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','${helper}','-Msix','${msix}','-Pfx','${pfx}' | ForEach-Object { $_.ExitCode }`,
    ]);
    // The elevated child does not inherit this process's stdout, so the evidence of a

    const registered = run("powershell", ["-NoProfile", "-Command", "(Get-AppxPackage -Name DevchievementsCompanion).InstallLocation"]);
    const location = (registered.out || "").trim().split(/\r?\n/)[0];
    check("the package is registered", location.length > 0, registered.out);
    if (location) {
      fs.writeFileSync(path.join(outDir, "install-location.txt"), location);
      console.log("  info installed at " + location);
    }
    check("the package installs (elevated)", install.status === 0, install.out);
  } else {
    check("signtool is available", false, "not found");
  }

  report();
}

function report() {
  console.log("");
  if (failures) {
    console.log("MSIX FAILED (" + failures + " checks)");
    process.exitCode = 1;
  } else {
    console.log("MSIX BUILT: " + path.join(outDir, "Devchievements.msix"));
  }
}

main();

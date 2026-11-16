# scripts/install-companion.ps1
#
# Usage (from the repository root):
#   .\scripts\install-companion.ps1
#
# Why this is a separate step from running the app:
#
#   Registering a notification identity means writing a Start Menu shortcut and a
#   HKCU key. That is installation, not runtime. An application that silently
#   rewrites the Start Menu every launch looks exactly like something malicious, and
#   a standard user account is often not allowed to do it anyway. So the app only
#   CHECKS (--toast-status) and this script REGISTERS.
#
# What it does:
#   1. asks the app to write the .lnk at a workspace path;
#   2. copies it into the user's Start Menu Programs folder;
#   3. writes HKCU\Software\Classes\AppUserModelId\<id> for the display name;
#   4. asks the shell to notice (--notify-shell), because the notification platform
#      otherwise keeps its cached app list until the next sign-in;
#   5. verifies by reading System.AppUserModel.ID back through the shell.
#
# Keep this file ASCII-only (the host ANSI codepage is 936).

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $repoRoot
$exe = Join-Path $repoRoot 'apps\desktop\src-tauri\target\debug\devchievements-desktop.exe'
$appId = 'Devchievements.Companion'
$displayName = 'Devchievements'

if (-not (Test-Path $exe)) {
    throw ("install: missing " + $exe + " -- build it first with: node scripts/verify.mjs")
}

$staging = Join-Path $workspace '_toolprobe\install-staging'
New-Item -ItemType Directory -Path $staging -Force | Out-Null
$stagedLnk = Join-Path $staging 'Devchievements.lnk'
Remove-Item $stagedLnk -Force -ErrorAction SilentlyContinue

Write-Host 'install: writing the shortcut'
& $exe --register-shortcut $stagedLnk | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'install: the app could not write the shortcut' }

$programs = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$installed = Join-Path $programs 'Devchievements.lnk'
Write-Host ('install: copying to ' + $installed)
Copy-Item $stagedLnk $installed -Force

Write-Host 'install: registering the display name'
$key = 'HKCU:\Software\Classes\AppUserModelId\' + $appId
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name 'DisplayName' -Value $displayName

Write-Host 'install: notifying the shell'
& $exe --notify-shell | Out-Host

Write-Host 'install: verifying'
$shell = New-Object -ComObject Shell.Application
$folder = $shell.Namespace($programs)
$item = $folder.ParseName('Devchievements.lnk')
if (-not $item) { throw 'install: the shortcut is not visible to the shell' }
$actual = $item.ExtendedProperty('System.AppUserModel.ID')
Write-Host ('install: System.AppUserModel.ID = [' + $actual + ']')
if ($actual -ne $appId) { throw 'install: the shortcut does not carry our AppUserModelID' }

& $exe --toast-status | Out-Host
Write-Host 'install: done'

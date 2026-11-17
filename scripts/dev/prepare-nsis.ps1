# Prepare the NSIS toolset the Tauri bundler expects.
#
# Why this exists: tauri-bundler caches its tools in %LOCALAPPDATA%\tauri and
# extracts nsis-3.13.zip there itself. In this environment that extraction step
# fails with os error 5 (access denied), so the toolset is pre-extracted once and
# copied into the exact layout the bundler validates. When the layout is already
# complete the bundler skips extraction entirely.
#
# The layout the bundler checks (tauri-bundler/src/bundle/windows/nsis/mod.rs):
#   <tools>/NSIS/{makensis.exe, NSIS.exe, Bin/makensis.exe, Stubs/..., Include/...,
#                 Plugins/x86-unicode/additional/nsis_tauri_utils.dll}
# NSIS.exe is also hash-verified; a wrong or missing one triggers a re-extract.
#
# Pure ASCII: Windows PowerShell 5.1 reads .ps1 as ANSI (AGENTS.md 4.2.1).

$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$toolchain = Join-Path (Split-Path -Parent $repoRoot) '_toolchain'
$nsisSource = Join-Path $toolchain 'tauri-tools\nsis\nsis-3.13'
$zipSource = Join-Path $toolchain 'tauri-tools\nsis-3.13.zip'
$utilsDll = Join-Path $toolchain 'tauri-tools\utils\nsis_tauri_utils.dll'

$toolsDir = Join-Path $env:LOCALAPPDATA 'tauri'
$nsisDir = Join-Path $toolsDir 'NSIS'

$required = @(
  'makensis.exe',
  'NSIS.exe',
  'Bin/makensis.exe',
  'Stubs/lzma-x86-unicode',
  'Stubs/lzma_solid-x86-unicode',
  'Include/MUI2.nsh',
  'Include/FileFunc.nsh',
  'Include/x64.nsh',
  'Include/nsDialogs.nsh',
  'Include/WinMessages.nsh',
  'Include/Win/COM.nsh',
  'Include/Win/Propkey.nsh',
  'Include/Win/RestartManager.nsh',
  'Plugins/x86-unicode/NSISdl.dll',
  'Plugins/x86-unicode/System.dll',
  'Plugins/x86-unicode/nsDialogs.dll',
  'Plugins/x86-unicode/additional/nsis_tauri_utils.dll'
)

function Test-Toolset {
  param([string] $Root)
  foreach ($rel in $required) {
    if (-not (Test-Path (Join-Path $Root $rel))) { return $false }
  }
  return $true
}

if (Test-Toolset $nsisDir) {
  Write-Output "NSIS toolset already complete at $nsisDir"
  exit 0
}

if (-not (Test-Path $nsisSource)) {
  if (-not (Test-Path $zipSource)) {
    Write-Error "Neither $nsisSource nor $zipSource exists. Run scripts/dev/fetch-nsis.ps1 first."
    exit 1
  }
  New-Item -ItemType Directory -Path (Split-Path -Parent $nsisSource) -Force | Out-Null
  Expand-Archive -LiteralPath $zipSource -DestinationPath (Split-Path -Parent $nsisSource) -Force
}

if (-not (Test-Path $utilsDll)) {
  Write-Error "Missing $utilsDll (nsis_tauri_utils.dll v0.5.3, sha1 75197FEE3C6A814FE035788D1C34EAD39349B860)."
  exit 1
}

New-Item -ItemType Directory -Path $toolsDir -Force | Out-Null
if (Test-Path $nsisDir) { Remove-Item $nsisDir -Recurse -Force }
Copy-Item -LiteralPath $nsisSource -Destination $nsisDir -Recurse -Force

$pluginDir = Join-Path $nsisDir 'Plugins\x86-unicode\additional'
New-Item -ItemType Directory -Path $pluginDir -Force | Out-Null
Copy-Item -LiteralPath $utilsDll -Destination (Join-Path $pluginDir 'nsis_tauri_utils.dll') -Force

if (-not (Test-Toolset $nsisDir)) {
  Write-Error "NSIS toolset still incomplete at $nsisDir"
  exit 1
}

Write-Output "NSIS toolset ready at $nsisDir"
exit 0

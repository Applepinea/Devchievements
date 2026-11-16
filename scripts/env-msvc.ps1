# scripts/env-msvc.ps1
#
# Usage (from the repository root, AFTER scripts/env.ps1):
#   . .\scripts\env-msvc.ps1
#
# Why this file exists:
#   The MSVC linker needs INCLUDE, LIB and the MSVC bin directory, and NONE of them
#   are in the default PATH. Worse, this host has a trap:
#
#     C:\Users\<user>\AppData\Local\hermes\git\usr\bin\link.exe
#
#   is a GNU 'link' utility, NOT the MSVC linker. It sits earlier in PATH, so a
#   plain 'cargo build' with the msvc toolchain invokes the WRONG linker and fails
#   with a confusing error. Prepending the MSVC bin directory is what fixes it.
#
#   vcvarsall.bat is the supported way to obtain the correct environment, so this
#   script runs it once in a child cmd, captures the result, and imports the
#   variables into the current session. Nothing is written to the registry.
#
# Keep this file ASCII-only (the host ANSI codepage is 936).

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $repoRoot
$dumpBat = Join-Path $PSScriptRoot 'dev\vcvars-dump.bat'

if (-not (Test-Path $dumpBat)) {
    throw "env-msvc.ps1: missing $dumpBat"
}

$dumpFile = Join-Path $workspace '_toolprobe\vcvars.txt'
$dumpDir = Split-Path -Parent $dumpFile
if (-not (Test-Path $dumpDir)) { New-Item -ItemType Directory -Path $dumpDir -Force | Out-Null }

# File redirection, never a pipe: this host's sandbox denies piped stdio to children.
& cmd.exe /c "`"$dumpBat`" > `"$dumpFile`" 2>&1"
if ($LASTEXITCODE -ne 0) {
    throw 'env-msvc.ps1: vcvarsall failed. Is the VS Build Tools workload installed?'
}

$vars = @{}
foreach ($line in (Get-Content $dumpFile)) {
    if ($line -match '^([A-Za-z_][A-Za-z0-9_]*)=(.*)$') {
        $vars[$matches[1]] = $matches[2]
    }
}

foreach ($name in @('Path', 'INCLUDE', 'LIB', 'LIBPATH')) {
    $key = $vars.Keys | Where-Object { $_ -ieq $name } | Select-Object -First 1
    if ($key) {
        Set-Item -Path "env:$name" -Value $vars[$key]
    }
}

# The dump also exports VSCMD_* which rustc/cc-rs read to locate the toolchain.
foreach ($key in $vars.Keys) {
    if ($key -like 'VSCMD_*') { Set-Item -Path "env:$key" -Value $vars[$key] }
}

# Prepend the MSVC bin directory so the correct link.exe wins over Git's coreutils.
$msvcBin = ($env:Path -split ';') | Where-Object { $_ -like '*\VC\Tools\MSVC\*\bin\HostX64\x64' } | Select-Object -First 1
if ($msvcBin -and ($env:Path -notlike "$msvcBin;*")) {
    $env:Path = "$msvcBin;$env:Path"
}

$link = Get-Command link.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if ($link) {
    if ($link.Source -like '*\git\usr\bin\*') {
        Write-Warning "env-msvc.ps1: link.exe resolves to $($link.Source) which is NOT the MSVC linker."
    } else {
        Write-Host "env-msvc.ps1: link.exe = $($link.Source)"
    }
}
# Let verify.mjs build the desktop app too, without duplicating toolchain discovery.
$msvcCargo = Join-Path $workspace "_toolchain\cargo\bin\cargo.exe"
if (Test-Path $msvcCargo) { $env:DEVCH_MSVC_CARGO = $msvcCargo }

$rc = Get-Command rc.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if ($rc) { Write-Host "env-msvc.ps1: rc.exe   = $($rc.Source)" } else { Write-Warning 'env-msvc.ps1: rc.exe not found (Tauri resource compilation will fail)' }
Write-Host ("env-msvc.ps1: MSVC     = " + $env:VSCMD_VER)

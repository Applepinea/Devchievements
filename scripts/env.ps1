# scripts/env.ps1
#
# Usage (from the repository root):
#   . .\scripts\env.ps1
#
# Why this file exists:
#   The Rust toolchain for this project is installed INSIDE the workspace
#   (<workspace>/_toolchain) and was installed with --no-modify-path, i.e. it is
#   fully isolated and can be removed by deleting one directory.
#   Every new shell must re-export these variables.
#
#   Also, on this host %LOCALAPPDATA%\npm-cache is not writable by the sandbox
#   (measured: EPERM), so npm/pnpm caches and stores are redirected into the
#   workspace.
#
# IMPORTANT: this script is executed by Windows PowerShell 5.1 (the host exports
# a 936/GBK ANSI codepage and reads .ps1 files as ANSI unless they carry a UTF-8
# BOM). Keep this file ASCII-only. Chinese documentation belongs in .md files.
#
# This file only sets process-scoped environment variables. It never writes the
# registry or the user's persistent environment, and it never edits user files.

$ErrorActionPreference = 'Stop'

$repoRoot  = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $repoRoot
$toolchain = Join-Path $workspace '_toolchain'

$env:RUSTUP_HOME = Join-Path $toolchain 'rustup'
$env:CARGO_HOME  = Join-Path $toolchain 'cargo'
$cargoBin        = Join-Path $env:CARGO_HOME 'bin'

if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = $cargoBin + ';' + $env:PATH
}

# npm / pnpm cache must live inside the workspace (outside is not writable).
$npmCache = Join-Path $workspace '_npm-cache'
if (-not (Test-Path $npmCache)) {
    New-Item -ItemType Directory -Path $npmCache -Force | Out-Null
}
$env:NPM_CONFIG_CACHE = $npmCache
$env:NPM_CONFIG_STORE_DIR = Join-Path $workspace '_pnpm-store'

# This host has a broken schannel credential chain: every CLI that uses Windows
# schannel for TLS fails with SEC_E_NO_CREDENTIALS. Git works when it uses its
# bundled OpenSSL backend instead. GIT_CONFIG_* injects that setting for the
# current process ONLY; the user's global git config is never modified.
$env:GIT_CONFIG_COUNT = '1'
$env:GIT_CONFIG_KEY_0 = 'http.sslBackend'
$env:GIT_CONFIG_VALUE_0 = 'openssl'

# DSH-bundled Node and pnpm (no system-wide pnpm is installed).
$dshNode = 'C:\Users\csa\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe'
$dshPnpm = 'C:\Users\csa\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\pnpm\bin\pnpm.mjs'

if ((Test-Path $dshNode) -and (Test-Path $dshPnpm)) {
    Set-Alias -Name pnpm -Value Invoke-DshPnpm -Scope Global -Force
    function global:Invoke-DshPnpm {
        & $dshNode $dshPnpm @args
    }
}

Write-Host ("env.ps1: RUSTUP_HOME = " + $env:RUSTUP_HOME)
Write-Host ("env.ps1: CARGO_HOME  = " + $env:CARGO_HOME)
Write-Host ("env.ps1: NPM cache   = " + $env:NPM_CONFIG_CACHE)
if (Test-Path (Join-Path $cargoBin 'cargo.exe')) {
    Write-Host ("env.ps1: cargo       = " + (& (Join-Path $cargoBin 'cargo.exe') --version 2>&1))
} else {
    Write-Warning "env.ps1: cargo.exe not found; run scripts/install-toolchain.ps1 first"
}
Write-Host  "env.ps1: pnpm        = dsh node + pnpm.mjs wrapper"
Write-Host  "env.ps1: git TLS     = http.sslBackend=openssl (process-scoped only)"

# cargo cannot use schannel on this host either (same SEC_E_NO_CREDENTIALS bug).
# Point it at the local sparse-registry proxy instead. The proxy itself must be
# started separately: scripts\start-crates-proxy.ps1
$registryScript = Join-Path $PSScriptRoot 'cargo-local-registry.ps1'
if (Test-Path $registryScript) {
    & $registryScript -Port 8787 | Out-Null
    Write-Host "env.ps1: cargo src   = local sparse proxy http://127.0.0.1:8787/index/"
    $proxyUrl = 'http://127.0.0.1:8787/health'
    $healthy = $false
    try {
        $resp = Invoke-WebRequest -Uri $proxyUrl -UseBasicParsing -TimeoutSec 3
        $healthy = ($resp.StatusCode -eq 200)
    } catch {
        $healthy = $false
    }
    if (-not $healthy) {
        Write-Warning 'env.ps1: crates proxy is NOT running. Start it with:'
        Write-Warning '           .\scripts\start-crates-proxy.ps1'
    }
}


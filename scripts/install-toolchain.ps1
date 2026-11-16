# scripts/install-toolchain.ps1
#
# One-shot, idempotent setup of the ISOLATED development toolchain for this repo.
#
# WHY A SCRIPT: the toolchain lives inside the workspace (ADR-0012) so it can be
# deleted without a trace, but that means it must be reconstructable. Everything
# here was discovered experimentally; without a script the knowledge would rot.
#
# WHAT IT SETS UP (all under <workspace>/_toolchain):
#   1. rustup + stable-x86_64-pc-windows-gnu + wasm32-unknown-unknown
#   2. as.exe            (binutils); without it dlltool cannot make import libs
#   3. mingw-w64 GCC     (gcc + cc1 + gcc-libs); SQLite's C sources need it
#   4. binutils ar.exe   (cc-rs archives objects with ar)
#   5. sysroot           (crt + headers + winpthreads + default-manifest.o)
#   6. cargo sparse registry proxy, because this host's schannel TLS is broken
#
# It is safe to run repeatedly. Keep this file ASCII-only (Windows PowerShell 5.1
# reads .ps1 as ANSI under this host's GBK codepage).

param(
    [switch]$SkipRust
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspace = Split-Path -Parent $repoRoot
$toolchain = Join-Path $workspace '_toolchain'
$node = 'C:\Users\csa\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe'

if (-not (Test-Path $node)) {
    $cmd = Get-Command node -ErrorAction SilentlyContinue
    if ($cmd) { $node = $cmd.Source }
}
if (-not $node) { throw 'install-toolchain.ps1: node.exe not found' }

function Step($name, $script) {
    Write-Host ''
    Write-Host ('=== ' + $name + ' ===') -ForegroundColor Cyan
    & $node (Join-Path $PSScriptRoot ('dev\' + $script))
    if ($LASTEXITCODE -ne 0) {
        Write-Warning ($name + ' reported a failure (exit ' + $LASTEXITCODE + ')')
    }
}

if (-not $SkipRust) {
    $rustup = Join-Path $toolchain 'cargo\bin\rustup.exe'
    if (-not (Test-Path $rustup)) {
        throw 'Rust is not installed. Run scripts/install-rust.ps1 first (or pass -SkipRust to skip).'
    }
    Write-Host 'Rust toolchain present.' -ForegroundColor Green
}

# 2. assembler (needed by dlltool for Windows import libraries)
Step 'binutils for Rust toolchain (as.exe)' 'install-mingw-binutils.mjs'
Step 'DLL closure for as.exe' 'resolve-mingw-deps.mjs'

# 3-5. a real C toolchain (needed by libsqlite3-sys via cc-rs)
Step 'mingw-w64 GCC' 'install-mingw-gcc.mjs'
Step 'GCC runtime DLL closure' 'complete-gcc-deps.mjs'
Step 'mingw binutils tools (ar, ranlib, ...)' 'complete-mingw-binutils.mjs'
Step 'mingw sysroot (crt, headers, pthreads, manifest)' 'complete-mingw-sysroot.mjs'

Write-Host ''
Write-Host 'Toolchain setup complete.' -ForegroundColor Green
Write-Host 'Now run:  . .\scripts\env.ps1'

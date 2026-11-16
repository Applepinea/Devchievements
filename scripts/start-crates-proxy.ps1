# scripts/start-crates-proxy.ps1
#
# Starts the local crates.io sparse-registry proxy in a NEW background process
# and waits until it answers /health. Idempotent: if it is already running,
# nothing is started.
#
# WHY (host workaround, NOT part of the product)
# ----------------------------------------------
# This host's Windows schannel credential chain is broken
# (SEC_E_NO_CREDENTIALS / 0x8009030e). cargo links libcurl+schannel on Windows,
# so it cannot reach crates.io. Node.js uses OpenSSL and works. The proxy bridges
# the two, bound to 127.0.0.1 only.
#
# Revert: stop the node process and delete .cargo/config.toml.
#
# Keep this file ASCII-only (Windows PowerShell 5.1 reads .ps1 as ANSI).

param(
    [int]$Port = 8787
)

$ErrorActionPreference = 'Stop'

function Test-ProxyUp {
    param([int]$P)
    try {
        $u = 'http://127.0.0.1:' + $P + '/health'
        $r = Invoke-WebRequest -Uri $u -UseBasicParsing -TimeoutSec 3
        return ($r.StatusCode -eq 200)
    } catch {
        return $false
    }
}

if (Test-ProxyUp -P $Port) {
    Write-Host "start-crates-proxy.ps1: already running on port $Port"
    return
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$node = 'C:\Users\csa\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe'
if (-not (Test-Path $node)) {
    $cmd = Get-Command node -ErrorAction SilentlyContinue
    if ($cmd) { $node = $cmd.Source }
}
if (-not $node) { throw 'start-crates-proxy.ps1: node.exe not found' }

$script = Join-Path $PSScriptRoot 'local-crates-proxy.mjs'
$logDir = Join-Path $repoRoot '.logs'
if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir -Force | Out-Null }
$stdout = Join-Path $logDir 'crates-proxy.out.log'
$stderr = Join-Path $logDir 'crates-proxy.err.log'

$startArgs = @{
    FilePath = $node
    ArgumentList = @($script, '--port', "$Port")
    WorkingDirectory = $repoRoot
    WindowStyle = 'Hidden'
    RedirectStandardOutput = $stdout
    RedirectStandardError = $stderr
}
Start-Process @startArgs | Out-Null

for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Milliseconds 250
    if (Test-ProxyUp -P $Port) {
        Write-Host "start-crates-proxy.ps1: listening on http://127.0.0.1:$Port"
        Write-Host "start-crates-proxy.ps1: log -> $stdout"
        return
    }
}
throw "start-crates-proxy.ps1: proxy did not become healthy; see $stderr"

# Install the locally built MSIX for testing.
#
# Requires administrator rights: trusting a test certificate and registering a
# package both need them. The Store path does not: Microsoft re-signs the package,
# so this elevation is a property of sideloading a self-signed build, not of MSIX.
#
# The previous test install is removed first. Add-AppxPackage refuses to replace a
# package with the same version, and a stale registration would make every probe
# describe the previous build instead of the one just packed.
#
# ASCII only (the host's ANSI code page is 936).
param(
  [Parameter(Mandatory = $true)][string]$Msix,
  [Parameter(Mandatory = $true)][string]$Pfx
)

$ErrorActionPreference = 'Stop'

$existing = Get-AppxPackage -Name 'DevchievementsCompanion'
if ($existing) {
  Remove-AppxPackage -Package $existing.PackageFullName
  Write-Output ('REMOVED ' + $existing.PackageFullName)
}

Import-PfxCertificate -FilePath $Pfx -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' -Password (ConvertTo-SecureString -String 'test' -Force -AsPlainText) | Out-Null
Write-Output 'CERTIFICATE TRUSTED'

try {
  Add-AppxPackage -Path $Msix -ErrorAction Stop
} catch {
  Write-Output ('INSTALL ERROR: ' + $_.Exception.Message)
  exit 1
}

$pkg = Get-AppxPackage -Name 'DevchievementsCompanion'
if ($pkg) {
  Write-Output ('PACKAGE INSTALLED ' + $pkg.PackageFullName)
  Write-Output ('INSTALL LOCATION ' + $pkg.InstallLocation)
} else {
  Write-Output 'INSTALL FAILED: package not registered'
  exit 1
}

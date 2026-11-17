# Install the locally built MSIX for testing.
#
# Requires administrator rights: trusting a test certificate and registering a
# package both need them. The Store path does not: Microsoft re-signs the package,
# so this elevation is a property of sideloading a self-signed build, not of MSIX.
#
# ASCII only (the host's ANSI code page is 936).
param(
  [Parameter(Mandatory = $true)][string]$Msix,
  [Parameter(Mandatory = $true)][string]$Pfx
)

$ErrorActionPreference = 'Stop'

Import-PfxCertificate -FilePath $Pfx -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' -Password (ConvertTo-SecureString -String 'test' -Force -AsPlainText) | Out-Null
Write-Output 'CERTIFICATE TRUSTED'

Add-AppxPackage -Path $Msix
$pkg = Get-AppxPackage -Name 'DevchievementsCompanion'
if ($pkg) {
  Write-Output ('PACKAGE INSTALLED ' + $pkg.PackageFullName)
  Write-Output ('INSTALL LOCATION ' + $pkg.InstallLocation)
} else {
  Write-Output 'INSTALL FAILED: package not registered'
  exit 1
}

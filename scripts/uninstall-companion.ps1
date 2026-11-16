# scripts/uninstall-companion.ps1
#
# Removes everything scripts/install-companion.ps1 created. User data is left
# untouched on purpose: deleting a database is a destructive action that belongs to
# an explicit 'delete my data' command, not to an uninstall.

$ErrorActionPreference = 'Continue'

$appId = 'Devchievements.Companion'
$programs = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$installed = Join-Path $programs 'Devchievements.lnk'

if (Test-Path $installed) { Remove-Item $installed -Force; Write-Host 'uninstall: shortcut removed' }
else { Write-Host 'uninstall: no shortcut found' }

$key = 'HKCU:\Software\Classes\AppUserModelId\' + $appId
if (Test-Path $key) { Remove-Item $key -Recurse -Force; Write-Host 'uninstall: AUMID key removed' }
else { Write-Host 'uninstall: no AUMID key found' }

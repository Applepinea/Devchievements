@echo off
REM Build a distributable Windows installer.
REM
REM Why a batch file: the same MSVC environment problem as build-desktop.bat, plus
REM tauri-cli needs the same CARGO_HOME/RUSTUP_HOME the workspace uses.
REM
REM What this produces: an NSIS installer under bundle/nsis/ together with the .sig
REM that signs it for the updater (spec 40.3). It is NOT Authenticode-signed, so
REM Windows shows a SmartScreen warning until a code signing certificate exists
REM (spec 40.4). Two different signatures; one is done, the other is blocked.
setlocal
set DEVCH_TMP=%~dp0..\..\..\_toolchain\tmp
if not exist "%DEVCH_TMP%" mkdir "%DEVCH_TMP%"
set TMP=%DEVCH_TMP%
set TEMP=%DEVCH_TMP%
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64
if errorlevel 1 exit /b 2
set TMP=%DEVCH_TMP%
set TEMP=%DEVCH_TMP%
set CARGO_HOME=%~dp0..\..\..\_toolchain\cargo
set RUSTUP_HOME=%~dp0..\..\..\_toolchain\rustup
set CC=
set CXX=
REM The workspace rustup default is the GNU toolchain (dlltool.exe is absent), so
REM a plain cargo build resolves it and fails. Use the MSVC toolchain explicitly.
set MSVC_TOOLCHAIN=%~dp0..\..\..\_toolchain\rustup\toolchains\stable-x86_64-pc-windows-msvc
set RUSTC=%MSVC_TOOLCHAIN%\bin\rustc.exe
REM tauri-bundler caches its toolset in %LOCALAPPDATA%\tauri and ignores
REM TAURI_BUNDLER_TOOLS_DIR; it extracts nsis-3.13.zip there itself, and that
REM extraction fails with os error 5 in this environment. prepare-nsis.ps1 lays
REM the toolset out exactly as the bundler validates it, so extraction is skipped.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0prepare-nsis.ps1"
if errorlevel 1 exit /b 5
REM Spec 40.3: bundle.createUpdaterArtifacts is on, so the bundler signs the
REM installer and writes the .sig an update feed serves. Signing needs the private
REM key, which lives outside the repository on purpose (AGENTS.md 7.1: no keys in
REM the repo). Generate one with: cargo tauri signer generate -w <path>
REM A release build takes the password from the environment (CI secret); only the
REM local development key falls back to the checked-in development password.
set TAURI_SIGNING_PRIVATE_KEY=%~dp0..\..\..\_toolchain\updater\devchievements.key
if not defined TAURI_SIGNING_PRIVATE_KEY_PASSWORD set TAURI_SIGNING_PRIVATE_KEY_PASSWORD=devchievements-dev
if not exist "%TAURI_SIGNING_PRIVATE_KEY%" (
  echo ERROR: no updater signing key at %TAURI_SIGNING_PRIVATE_KEY%
  echo Generate one with: "%CARGO_HOME%\bin\cargo.exe" tauri signer generate -w "%TAURI_SIGNING_PRIVATE_KEY%"
  exit /b 6
)
REM The helper is a bundle resource; build it first so the path exists.
cd /d "%~dp0..\.."
"%CARGO_HOME%\bin\cargo.exe" build --release -p devchievements-helper
if errorlevel 1 exit /b 3
REM Built-in sounds are resolved next to the executable. The bundle maps them in
REM through tauri.conf.json; this copy makes a plain release run (no installer)
REM behave the same, which is how the sounds went missing before: only the debug
REM build ever received them.
if exist "%~dp0..\..\apps\desktop\assets\sounds" (
  if not exist "%~dp0..\..\apps\desktop\src-tauri\target\release\sounds" mkdir "%~dp0..\..\apps\desktop\src-tauri\target\release\sounds"
  xcopy /E /I /Y /Q "%~dp0..\..\apps\desktop\assets\sounds\*.wav" "%~dp0..\..\apps\desktop\src-tauri\target\release\sounds\" >nul
)
cd /d "%~dp0..\..\apps\desktop\src-tauri"
"%CARGO_HOME%\bin\cargo.exe" tauri build --no-bundle
if errorlevel 1 exit /b 4
"%CARGO_HOME%\bin\cargo.exe" tauri bundle
exit /b %errorlevel%

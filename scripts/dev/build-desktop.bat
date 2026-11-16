@echo off
REM Build the Tauri desktop app with the MSVC toolchain.
REM
REM Why a .bat: the MSVC linker needs INCLUDE/LIB and the MSVC bin directory, and
REM this host also has a GNU 'link.exe' from Git that shadows the real one. Calling
REM vcvarsall here makes the build reproducible from any caller instead of relying
REM on the ambient environment.
REM
REM TMP/TEMP redirection: link.exe creates 'lnk{GUID}.tmp' in TEMP. When TEMP points
REM at %LOCALAPPDATA%\Temp the link fails with LNK1104 (measured), because writes
REM outside the workspace are denied to child processes here. Pointing TEMP inside
REM the workspace fixes it and is harmless elsewhere.
REM
REM NOTE: never wrap a path in a parenthesised if-block here; "(x86)" in the
REM Visual Studio path closes the block early and the batch dies with a confusing
REM "was unexpected at this time".
REM
REM ASCII only (host ANSI codepage is 936).
setlocal

set DEVCH_TMP=%~dp0..\..\..\_toolchain\tmp
if not exist "%DEVCH_TMP%" mkdir "%DEVCH_TMP%"
set TMP=%DEVCH_TMP%
set TEMP=%DEVCH_TMP%

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64
if errorlevel 1 exit /b 2

if not defined INCLUDE exit /b 3
if not defined LIB exit /b 4

REM vcvarsall may reset TEMP; restore ours afterwards.
set TMP=%DEVCH_TMP%
set TEMP=%DEVCH_TMP%

set CARGO_HOME=%~dp0..\..\..\_toolchain\cargo
set RUSTUP_HOME=%~dp0..\..\..\_toolchain\rustup
REM Clear CC/CXX: inheriting a MinGW value from the GNU workspace build would make
REM cc-rs pick gcc for a MSVC target.
set CC=
set CXX=

cd /d "%~dp0..\..\apps\desktop\src-tauri"
"%CARGO_HOME%\bin\cargo.exe" +stable-x86_64-pc-windows-msvc %*
set BUILD_EXIT=%errorlevel%

REM Sound assets must sit next to the executable: the audio layer resolves
REM them relative to current_exe(). Copying here means every build has them.
if exist "%~dp0..\..\apps\desktop\assets\sounds" (
  if not exist "target\debug\sounds" mkdir "target\debug\sounds"
  xcopy /E /I /Y /Q "%~dp0..\..\apps\desktop\assets\sounds\*.wav" "target\debug\sounds\" >nul
)

exit /b %BUILD_EXIT%

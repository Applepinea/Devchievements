@echo off
REM Install the Tauri CLI with the MSVC toolchain.
REM
REM Why this is fussy on this host: the workspace rustup default is the GNU toolchain
REM (dlltool.exe missing here), and `cargo install --target` still resolves std from
REM the DEFAULT toolchain. Pointing CARGO/RUSTC at the MSVC toolchain directly is the
REM only way that works without internet-side rustup changes.
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
set MSVC_TOOLCHAIN=%~dp0..\..\..\_toolchain\rustup\toolchains\stable-x86_64-pc-windows-msvc
set CC=
set CXX=
set RUSTC=%MSVC_TOOLCHAIN%\bin\rustc.exe
"%CARGO_HOME%\bin\cargo.exe" install tauri-cli --version "^2" --locked
exit /b %errorlevel%

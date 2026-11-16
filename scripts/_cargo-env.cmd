@echo off
REM scripts/_cargo-env.cmd
REM
REM Environment for driving cargo from cmd.exe (where stderr redirection works
REM reliably; PowerShell 5.1 mangles cargo's progress output on this host).
REM
REM Keep this file ASCII-only. Mirrors scripts/env.ps1 for the Rust-relevant parts.

set "WS=%~dp0.."
set "WS=%WS%.."
for %%i in ("%WS%") do set "WS=%%~fi"

set "RUSTUP_HOME=%WS%\_toolchain\rustup"
set "CARGO_HOME=%WS%\_toolchain\cargo"
set "MINGW=%WS%\_toolchain\_mingw-gcc\mingw64\bin"
set "SELFCONTAINED=%WS%\_toolchain\rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained"

set "PATH=%MINGW%;%SELFCONTAINED%;%CARGO_HOME%\bin;%PATH%"
if exist "%MINGW%\gcc.exe" set "CC=%MINGW%\gcc.exe"

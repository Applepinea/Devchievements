@echo off
REM Build the Tauri desktop app with the MSVC toolchain.
REM
REM Why a .bat: the MSVC linker needs INCLUDE/LIB and the MSVC bin directory, and
REM this host also has a GNU 'link.exe' from Git that shadows the real one. Calling
REM vcvarsall here makes the build reproducible from any caller instead of relying
REM on the ambient environment.
REM
REM NOTE: never wrap a path in a parenthesised if-block here; "(x86)" in the
REM Visual Studio path closes the block early and the batch dies with a confusing
REM "was unexpected at this time".
REM
REM ASCII only (host ANSI codepage is 936).
setlocal

call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64
if errorlevel 1 exit /b 2

if not defined INCLUDE exit /b 3
if not defined LIB exit /b 4

set CARGO_HOME=%~dp0..\..\..\_toolchain\cargo
set RUSTUP_HOME=%~dp0..\..\..\_toolchain\rustup
REM Clear CC/CXX: inheriting a MinGW value from the GNU workspace build would make
REM cc-rs pick gcc for a MSVC target.
set CC=
set CXX=

cd /d "%~dp0..\..\apps\desktop\src-tauri"
"%CARGO_HOME%\bin\cargo.exe" +stable-x86_64-pc-windows-msvc %*
exit /b %errorlevel%

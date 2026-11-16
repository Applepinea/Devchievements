@echo off
REM Dump the MSVC developer environment so a PowerShell session can import it.
REM ASCII only (host ANSI codepage is 936).
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
if errorlevel 1 (
  echo VCVARS_FAILED
  exit /b 1
)
set

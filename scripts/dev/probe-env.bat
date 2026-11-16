@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
echo VCVARS_ERRORLEVEL=%errorlevel%
echo INCLUDE=[%INCLUDE%]
echo LIB=[%LIB%]
echo VSCMD_VER=[%VSCMD_VER%]
echo SDKINC=[%WindowsSdkDir%]

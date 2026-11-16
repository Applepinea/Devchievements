@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
echo TEMP=%TEMP%
echo TMP=%TMP%
where link.exe
where cl.exe
link.exe /? > nul 2>&1
echo LINK_USAGE_EXIT=%errorlevel%
echo probe > "%TEMP%\lnkprobe.tmp"
echo WRITE_EXIT=%errorlevel%
if exist "%TEMP%\lnkprobe.tmp" echo FILE_PRESENT=yes
del "%TEMP%\lnkprobe.tmp" >nul 2>&1

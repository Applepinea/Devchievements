@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
cd /d "C:\Users\csa\Documents\deepseek-harness\default-workspace\_toolchain\cargo\registry\src\127.0.0.1-2d8ad37583957fb8\libsqlite3-sys-0.30.1"
"C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\HostX64\x64\cl.exe" @"C:\Users\csa\Documents\deepseek-harness\default-workspace\_toolprobe\cl.rsp"
echo CL_EXIT=%errorlevel%

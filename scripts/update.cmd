@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0updater.ps1" %*
set "result=%errorlevel%"
if not "%~1"=="-Quiet" pause
exit /b %result%

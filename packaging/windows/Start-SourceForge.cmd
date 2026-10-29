@echo off
rem SourceForge portable launcher for Windows. Uses the built-in PowerShell
rem HttpListener to serve the bundled static app on loopback; no install needed.
setlocal
set PORT=%1
if "%PORT%"=="" set PORT=8787
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0Start-SourceForge.ps1" -Port %PORT%
if errorlevel 1 pause

@echo off
title Fake Printer (Rust)
cd /d "%~dp0"

if exist "%~dp0fake-printer.exe" (
  "%~dp0fake-printer.exe"
  exit /b %errorlevel%
)
if exist "%~dp0rust\target\release\fake-printer.exe" (
  "%~dp0rust\target\release\fake-printer.exe"
  exit /b %errorlevel%
)
echo [ERROR] fake-printer.exe not found. Build it first: cd rust ^&^& cargo build --release
pause
exit /b 1

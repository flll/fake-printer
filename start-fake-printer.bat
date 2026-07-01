@echo off
title Fake Printer
cd /d "%~dp0"

set "PY=%~dp0.venv\Scripts\python.exe"
if not exist "%PY%" (
  echo [ERROR] Not set up. Run: pwsh scripts\setup.ps1
  pause
  exit /b 1
)

"%PY%" "%~dp0scripts\dashboard.py"

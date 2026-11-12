@echo off
rem Creates the private .venv in this folder and installs requirements.txt into it (nothing global).
setlocal
cd /d "%~dp0"
if not exist "%~dp0tmp" mkdir "%~dp0tmp"
set "TEMP=%~dp0tmp"
set "TMP=%~dp0tmp"
if exist "D:\Caches\pip" set "PIP_CACHE_DIR=D:\Caches\pip"
where py >nul 2>nul && (py -3.12 -m venv .venv || py -3.13 -m venv .venv) || python -m venv .venv
".venv\Scripts\python.exe" -m pip install --disable-pip-version-check -r requirements.txt
endlocal

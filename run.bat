@echo off
rem Master-Craft GUI launcher. Keeps temp files and caches inside the app folder.
setlocal
cd /d "%~dp0"
if not exist "%~dp0tmp" mkdir "%~dp0tmp"
set "TEMP=%~dp0tmp"
set "TMP=%~dp0tmp"
set "NUMBA_CACHE_DIR=%~dp0tmp\numba"
set "MPLCONFIGDIR=%~dp0tmp\mpl"
if not exist "%~dp0.venv\Scripts\python.exe" (
  echo [Master-Craft] .venv not found. Run setup.bat first.
  pause
  exit /b 1
)
call "%~dp0.venv\Scripts\activate.bat"
python -m mastercraft %*
if errorlevel 1 pause
endlocal

@echo off
rem Master-Craft batch CLI. Example: cli.bat "D:\Suno\*.wav" --profile Pop --format wav24
setlocal
cd /d "%~dp0"
if not exist "%~dp0tmp" mkdir "%~dp0tmp"
set "TEMP=%~dp0tmp"
set "TMP=%~dp0tmp"
set "NUMBA_CACHE_DIR=%~dp0tmp\numba"
set "MPLCONFIGDIR=%~dp0tmp\mpl"
if not exist "%~dp0.venv\Scripts\python.exe" (
  echo [Master-Craft] .venv not found. Run setup.bat first.
  exit /b 1
)
"%~dp0.venv\Scripts\python.exe" -m mastercraft.cli %*
set "MC_RC=%errorlevel%"
endlocal & exit /b %MC_RC%

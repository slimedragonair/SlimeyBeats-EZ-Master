@echo off
setlocal
cd /d "%~dp0rust"
if not exist "target\release\slimeybeats-ez-master-live.exe" (
    cargo build --release --bins
    if errorlevel 1 exit /b 1
)
start "" "target\release\slimeybeats-ez-master-live.exe" %*

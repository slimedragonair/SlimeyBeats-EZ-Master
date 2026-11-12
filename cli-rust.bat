@echo off
setlocal
cd /d "%~dp0rust"
if not exist "target\release\slimeybeats-ez-master-cli.exe" (
    cargo build --release --bin slimeybeats-ez-master-cli
    if errorlevel 1 exit /b 1
)
"target\release\slimeybeats-ez-master-cli.exe" %*

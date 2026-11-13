@echo off
setlocal
cd /d %~dp0

where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo Cargo not found. Please install Rust from https://rustup.rs/
    pause
    exit /b 1
)

if not exist .env (
    echo Creating .env from .env.example...
    copy .env.example .env
)

if not exist Cargo.toml (
    echo Cargo.toml not found. Run this script from the project root.
    pause
    exit /b 1
)

echo Building remote-browser...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo Build failed.
    pause
    exit /b 1
)

echo Starting remote-browser in tray...
start "remote-browser" target\release\remote-browser.exe
exit

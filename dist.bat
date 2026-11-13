@echo off
setlocal
cd /d %~dp0

where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo Cargo not found. Install Rust from https://rustup.rs/
    pause
    exit /b 1
)

echo Building release...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo Build failed.
    pause
    exit /b 1
)

if not exist dist mkdir dist
copy /y target\release\remote-browser.exe dist\ >nul
if not exist dist\.env copy /y .env.example dist\.env >nul

if not exist dist\README.txt (
    echo Remote Browser > dist\README.txt
    echo Requirements: 64-bit Windows with Chrome or Edge installed. >> dist\README.txt
    echo Run remote-browser.exe, then open http://localhost:5001 on your phone. >> dist\README.txt
    echo Edit .env for PORT, VIEWPORT, HEADLESS. >> dist\README.txt
)

echo.
echo Packaged:
dir /b dist
echo Copy the dist folder to the other machine and run remote-browser.exe there.
pause

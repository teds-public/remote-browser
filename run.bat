@echo off
:: Start remote-browser: check Node, install deps, launch server.
setlocal
cd /d "%~dp0"
set "code=0"

if "%~1"=="" goto :run
if /i "%~1"=="help" goto :usage
echo Unknown command "%~1".
set "code=1"
goto :pause

:run
where node >nul 2>&1
if errorlevel 1 (
  echo [FAIL] Node.js not found. Install Node 20+ first.
  set "code=1"
  goto :pause
)

if not exist ".env" (
  echo [.env missing] Copying from .env.example...
  copy ".env.example" ".env" >nul
  if errorlevel 1 (
    echo [FAIL] Cannot create .env.
    set "code=1"
    goto :pause
  )
)

if not exist "node_modules" (
  echo [deps missing] Installing...
  call npm install --no-audit --no-fund
  if errorlevel 1 (
    echo [FAIL] npm install failed.
    set "code=1"
    goto :pause
  )
)

echo Starting remote-browser...
call npm start
if errorlevel 1 (
  echo [FAIL] Server exited with error.
  set "code=1"
  goto :pause
)
goto :done

:usage
echo Usage: %~nx0 [help]
echo Double-click to start the server.
goto :done

:done
set "code=%errorlevel%"

:pause
echo "%cmdcmdline%" | findstr /i /c:"/c" >nul
if errorlevel 1 exit /b %code%
if defined MSYSTEM exit /b %code%
pause
exit /b %code%

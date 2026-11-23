@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Waypoint Lab - local build and launch

echo ==============================================================
echo Waypoint Lab - build and launch locally
echo No GitHub Actions, no remote GPU jobs, no metered CI.
echo ==============================================================
echo.

where cargo >nul 2>nul
if errorlevel 1 goto :missing_rust
where npm >nul 2>nul
if errorlevel 1 goto :missing_node

if not exist "node_modules\@tauri-apps\cli" (
    echo Installing desktop build dependencies locally...
    call npm ci
    if errorlevel 1 goto :failed
)

echo Building native Waypoint Lab executable on this Windows PC...
call npm run dist
if errorlevel 1 goto :failed

if not exist "src-tauri\target\release\waypoint.exe" goto :missing_exe

echo.
echo Launching Waypoint Lab. Choose the Benchmarks tab and Run locally.
start "" "src-tauri\target\release\waypoint.exe"
exit /b 0

:missing_rust
echo Rust is not installed or cargo is not on PATH.
echo Install Rust 1.90+ from https://rustup.rs and the Visual Studio C++ Build Tools.
goto :failed
:missing_node
echo Node.js / npm is not installed.
echo Install Node.js 20+ from https://nodejs.org
goto :failed
:missing_exe
echo Build completed but Waypoint executable was not found.
goto :failed
:failed
echo.
echo Build or dependency check failed. No GitHub Actions were used.
echo See docs\LOCAL_BENCHMARKS.md for prerequisites and troubleshooting.
pause
exit /b 1

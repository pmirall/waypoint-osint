@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Waypoint Lab - build local Windows installer

echo ================================================================
echo BUILD LOCAL WINDOWS INSTALLER - Waypoint Lab
echo Runs on YOUR PC only.  Zero GitHub Actions.
echo ================================================================
echo.

where cargo >nul 2>nul
if errorlevel 1 goto :missing_rust
where npm >nul 2>nul
if errorlevel 1 goto :missing_node

if not exist "node_modules\@tauri-apps\cli" (
  echo Installing local Node/Tauri build dependencies...
  call npm ci
  if errorlevel 1 goto :failed
)

echo Building unsigned NSIS installer on this computer...
echo Local override disables Tauri updater signing artifacts.
call npm run installer -- --config src-tauri/tauri.local.conf.json
if errorlevel 1 goto :failed

if not exist "src-tauri\target\release\bundle\nsis" goto :failed
echo.
echo Local installer build completed.
echo The installer will not have a trusted commercial code-signing certificate.
echo Windows SmartScreen may warn about unsigned software.
explorer "%CD%\src-tauri\target\release\bundle\nsis"
exit /b 0

:missing_rust
echo Rust / Cargo not installed. Install Rust 1.90+ and Visual Studio C++ Build Tools.
goto :failed
:missing_node
echo Node.js 20+ and npm are required for local packaging.
goto :failed
:failed
echo.
echo Build failed or prerequisites are incomplete. No GitHub Actions were used.
echo See docs\LOCAL_BENCHMARKS.md for details.
pause
exit /b 1

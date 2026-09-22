@echo off
title FastPing MMO WAN Accelerator
cd /d "%~dp0"
echo ========================================================
echo   Demarrage de FastPing MMO Accelerator (GUI)...
echo   VPS configure: 72.61.111.131
echo ========================================================
cargo run --release -p accelerator-gui
pause

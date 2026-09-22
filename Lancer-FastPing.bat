@echo off
title FastPing MMO WAN Accelerator
cd /d "%~dp0"

:: Check for Administrator privileges
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo ========================================================
    echo   [!] Privileges Administrateur requis pour WinDivert.
    echo   [!] Demande d'elevation UAC en cours...
    echo ========================================================
    powershell -Command "Start-Process cmd -ArgumentList '/c cd /d \""%~dp0\"" && Lancer-FastPing.bat' -Verb RunAs"
    exit /b
)

echo ========================================================
echo   Demarrage de FastPing MMO Accelerator (GUI - ADMIN)...
echo   VPS configure: 72.61.111.131
echo ========================================================
cargo run --release -p accelerator-gui
pause

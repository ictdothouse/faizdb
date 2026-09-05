@echo off
title FaizDB Studio — Official Desktop & Edge Control Center
color 0A

echo ==============================================================================
echo    🔥 FaizDB Official Studio — Local Engine ^& Edge Robot Control Center
echo ==============================================================================
echo.
echo  [1/2] Starting FaizDB Native Server (Port 27018, 5433, 27017, 50051)...
start "FaizDB-Engine" /min .\faizdb.exe serve --pg-port 5433

echo  [2/2] Launching FaizDB Studio UI...
timeout /t 2 /nobreak >nul
start "" http://localhost:27020

echo.
echo  ==============================================================================
echo   FaizDB is now LIVE!
echo   - Studio Web App:       http://localhost:27020
echo   - Robot Control Center: http://localhost:27020/#robot
echo   - PostgreSQL Wire:      localhost:5433 (psql, DBeaver)
echo   - MongoDB Wire:         localhost:27017 (mongosh, Compass)
echo   - HTTP / REST API:      http://localhost:27018
echo  ==============================================================================
echo.
echo  Press any key to close this launcher (FaizDB will continue running in background).
pause >nul

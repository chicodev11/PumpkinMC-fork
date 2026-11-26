@echo off
rem Runs the server and restarts it after a crash or a watchdog stop.
rem A normal /stop (exit code 0) ends the loop.
rem Usage: scripts\run-server.bat [path\to\pumpkin.exe]   (run from the server folder)
setlocal
set BINARY=%~1
if "%BINARY%"=="" set BINARY=pumpkin.exe

:loop
echo [run-server] Starting %BINARY% at %date% %time%
"%BINARY%"
if %errorlevel%==0 (
    echo [run-server] Server stopped normally.
    exit /b 0
)
echo [run-server] Server exited with code %errorlevel% at %date% %time%
if not exist crash-reports mkdir crash-reports
echo %date% %time% exit code %errorlevel% >> crash-reports\restarts.log
echo [run-server] Restarting in 10 seconds (Ctrl+C to cancel)...
timeout /t 10 /nobreak >nul
goto loop

@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
echo %* | findstr /C:"--cli" >nul
if %ERRORLEVEL%==0 goto cli
start "" /B powershell.exe -NoProfile -ExecutionPolicy Bypass -STA -File "%SCRIPT_DIR%MultiZaloDesktop.ps1" %*
exit /b 0
:cli
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%MultiZaloDesktop.ps1" %*
exit /b %ERRORLEVEL%

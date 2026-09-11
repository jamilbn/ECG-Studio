@echo off
setlocal

set "CARGO_TARGET_DIR=%LOCALAPPDATA%\ECGStudio\target-release-app"

cargo build --release
if errorlevel 1 exit /b %errorlevel%

if not exist "dist" mkdir "dist"
copy /Y "%CARGO_TARGET_DIR%\release\ecg-studio.exe" "dist\ECGStudio.exe" >nul
if errorlevel 1 exit /b %errorlevel%

set "APP_ICON=%CARGO_TARGET_DIR%\app.ico"
powershell -NoProfile -ExecutionPolicy Bypass -File "tools\New-IcoFromPng.ps1" -SourcePath "assets\app-icon.png" -IconPath "%APP_ICON%" -Width 256 -Height 256
if errorlevel 1 exit /b %errorlevel%

powershell -NoProfile -ExecutionPolicy Bypass -File "tools\Set-ExeIcon.ps1" -ExePath "dist\ECGStudio.exe" -IconPath "%APP_ICON%"
if errorlevel 1 exit /b %errorlevel%

if exist "dist\ECGStudio-new.exe" del /f /q "dist\ECGStudio-new.exe"

echo dist\ECGStudio.exe

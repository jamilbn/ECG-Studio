@echo off
setlocal

pushd "%~dp0"

set "ISCC="
if defined INNO_ISCC if exist "%INNO_ISCC%" set "ISCC=%INNO_ISCC%"
if not defined ISCC if exist "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" set "ISCC=%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"
if not defined ISCC if exist "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
if not defined ISCC if exist "%ProgramFiles%\Inno Setup 6\ISCC.exe" set "ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe"
if not defined ISCC for /f "delims=" %%I in ('where.exe ISCC.exe 2^>nul') do if not defined ISCC set "ISCC=%%I"

if not defined ISCC (
    echo Inno Setup 6 nao encontrado.
    echo Instale com:
    echo   winget install --id JRSoftware.InnoSetup -e
    echo Depois rode novamente:
    echo   build_installer.bat
    popd
    exit /b 1
)

set "APP_VERSION=0.1.0"
for /f "tokens=2 delims== " %%V in ('findstr /R /C:"^version = " Cargo.toml') do set "APP_VERSION=%%~V"
set "APP_VERSION=%APP_VERSION:"=%"

call ".\build_release.bat"
if errorlevel 1 (
    popd
    exit /b %errorlevel%
)

if not exist "dist" mkdir "dist"
powershell -NoProfile -ExecutionPolicy Bypass -File "tools\New-IcoFromPng.ps1" -SourcePath "assets\app-icon.png" -IconPath "dist\ECGStudio.ico" -Width 256 -Height 256
if errorlevel 1 (
    popd
    exit /b %errorlevel%
)

"%ISCC%" /DAppVersion="%APP_VERSION%" "packaging\windows\ecg-studio.iss"
if errorlevel 1 (
    popd
    exit /b %errorlevel%
)

echo dist\ECGStudioSetup-%APP_VERSION%.exe
popd

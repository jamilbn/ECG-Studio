@echo off
setlocal EnableExtensions

cd /d "%~dp0"

set "MODE=%~1"
if "%MODE%"=="" set "MODE=apple-silicon"
if /I "%MODE%"=="--apple-silicon" set "MODE=apple-silicon"
if /I "%MODE%"=="arm64" set "MODE=apple-silicon"
if /I "%MODE%"=="aarch64" set "MODE=apple-silicon"
if /I "%MODE%"=="--universal" set "MODE=universal"
if /I "%MODE%"=="universal2" set "MODE=universal"
if /I "%MODE%"=="--intel" set "MODE=intel"
if /I "%MODE%"=="x64" set "MODE=intel"
if /I "%MODE%"=="x86_64" set "MODE=intel"
if /I "%MODE%"=="help" goto :usage
if /I "%MODE%"=="--help" goto :usage
if /I "%MODE%"=="-h" goto :usage

if /I not "%MODE%"=="apple-silicon" if /I not "%MODE%"=="universal" if /I not "%MODE%"=="intel" goto :usage

echo ECG Studio macOS cross-build from Windows
echo.
echo This is experimental. For release builds, prefer running build_macos.sh on macOS.
echo Requirements: Rust, cargo-zigbuild, Zig, and Rust macOS targets.
echo Universal builds also require llvm-lipo on PATH.
echo.

where cargo >nul 2>nul
if errorlevel 1 (
    echo Missing cargo.
    exit /b 1
)

where rustup >nul 2>nul
if errorlevel 1 (
    echo Missing rustup.
    exit /b 1
)

cargo zigbuild --version >nul 2>nul
if errorlevel 1 (
    echo Missing cargo-zigbuild. Install it with:
    echo cargo install cargo-zigbuild
    exit /b 1
)

where zig >nul 2>nul
if errorlevel 1 (
    echo Missing zig. Install Zig and make sure zig.exe is on PATH.
    exit /b 1
)

if /I "%MODE%"=="universal" (
    where llvm-lipo >nul 2>nul
    if errorlevel 1 (
        echo Missing llvm-lipo. Build apple-silicon or intel separately, or install LLVM.
        exit /b 1
    )
)

if not defined MACOSX_DEPLOYMENT_TARGET set "MACOSX_DEPLOYMENT_TARGET=12.0"
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%LOCALAPPDATA%\ECGStudio\target-macos-cross"

set "APP_VERSION=0.1.0"
for /f "tokens=3 delims= " %%A in ('findstr /b /c:"version = " Cargo.toml') do set "APP_VERSION=%%~A"

set "DIST_DIR=dist\macos-cross"
set "APP_DIR=%DIST_DIR%\ECG Studio.app"
set "BUNDLE_BIN=%APP_DIR%\Contents\MacOS\ecg-studio"
set "RESOURCES_DIR=%APP_DIR%\Contents\Resources"

if exist "%APP_DIR%" rmdir /s /q "%APP_DIR%"
mkdir "%APP_DIR%\Contents\MacOS"
if errorlevel 1 exit /b %errorlevel%
mkdir "%RESOURCES_DIR%"
if errorlevel 1 exit /b %errorlevel%

call :write_plist
if errorlevel 1 exit /b %errorlevel%

if exist "assets\app-icon.png" copy /Y "assets\app-icon.png" "%RESOURCES_DIR%\app-icon.png" >nul

if /I "%MODE%"=="universal" goto :build_universal
if /I "%MODE%"=="apple-silicon" goto :build_apple_silicon
if /I "%MODE%"=="intel" goto :build_intel

:build_apple_silicon
call :build_target aarch64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
call :copy_target_binary aarch64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
goto :done

:build_intel
call :build_target x86_64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
call :copy_target_binary x86_64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
goto :done

:build_universal
call :build_target aarch64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
call :build_target x86_64-apple-darwin
if errorlevel 1 exit /b %errorlevel%
llvm-lipo -create -output "%BUNDLE_BIN%" "%CARGO_TARGET_DIR%\aarch64-apple-darwin\release\ecg-studio" "%CARGO_TARGET_DIR%\x86_64-apple-darwin\release\ecg-studio"
if errorlevel 1 exit /b %errorlevel%
goto :done

:done
echo.
echo %APP_DIR%
echo.
echo After copying this .app to a Mac, run:
echo chmod +x "ECG Studio.app/Contents/MacOS/ecg-studio"
echo.
echo For signing, notarization, and the most reliable .app metadata, run build_macos.sh on macOS.
exit /b 0

:build_target
set "TARGET=%~1"
echo Building %TARGET%...
rustup target add %TARGET%
if errorlevel 1 exit /b %errorlevel%
cargo zigbuild --release --target %TARGET%
exit /b %errorlevel%

:copy_target_binary
set "TARGET=%~1"
set "SOURCE_BIN=%CARGO_TARGET_DIR%\%TARGET%\release\ecg-studio"
if not exist "%SOURCE_BIN%" (
    echo Expected binary not found: %SOURCE_BIN%
    exit /b 1
)
copy /Y "%SOURCE_BIN%" "%BUNDLE_BIN%" >nul
exit /b %errorlevel%

:write_plist
set "PLIST=%APP_DIR%\Contents\Info.plist"
> "%PLIST%" echo ^<?xml version="1.0" encoding="UTF-8"?^>
>> "%PLIST%" echo ^<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"^>
>> "%PLIST%" echo ^<plist version="1.0"^>
>> "%PLIST%" echo ^<dict^>
>> "%PLIST%" echo     ^<key^>CFBundleName^</key^>
>> "%PLIST%" echo     ^<string^>ECG Studio^</string^>
>> "%PLIST%" echo     ^<key^>CFBundleDisplayName^</key^>
>> "%PLIST%" echo     ^<string^>ECG Studio^</string^>
>> "%PLIST%" echo     ^<key^>CFBundleExecutable^</key^>
>> "%PLIST%" echo     ^<string^>ecg-studio^</string^>
>> "%PLIST%" echo     ^<key^>CFBundleIdentifier^</key^>
>> "%PLIST%" echo     ^<string^>com.ecgstudio.app^</string^>
>> "%PLIST%" echo     ^<key^>CFBundleVersion^</key^>
>> "%PLIST%" echo     ^<string^>%APP_VERSION%^</string^>
>> "%PLIST%" echo     ^<key^>CFBundleShortVersionString^</key^>
>> "%PLIST%" echo     ^<string^>%APP_VERSION%^</string^>
>> "%PLIST%" echo     ^<key^>CFBundlePackageType^</key^>
>> "%PLIST%" echo     ^<string^>APPL^</string^>
>> "%PLIST%" echo     ^<key^>LSMinimumSystemVersion^</key^>
>> "%PLIST%" echo     ^<string^>%MACOSX_DEPLOYMENT_TARGET%^</string^>
>> "%PLIST%" echo     ^<key^>NSHighResolutionCapable^</key^>
>> "%PLIST%" echo     ^<true/^>
>> "%PLIST%" echo     ^<key^>CFBundleDocumentTypes^</key^>
>> "%PLIST%" echo     ^<array^>
>> "%PLIST%" echo         ^<dict^>
>> "%PLIST%" echo             ^<key^>CFBundleTypeName^</key^>
>> "%PLIST%" echo             ^<string^>ECG document^</string^>
>> "%PLIST%" echo             ^<key^>CFBundleTypeRole^</key^>
>> "%PLIST%" echo             ^<string^>Viewer^</string^>
>> "%PLIST%" echo             ^<key^>LSHandlerRank^</key^>
>> "%PLIST%" echo             ^<string^>Alternate^</string^>
>> "%PLIST%" echo             ^<key^>LSItemContentTypes^</key^>
>> "%PLIST%" echo             ^<array^>
>> "%PLIST%" echo                 ^<string^>com.ecgstudio.ecg^</string^>
>> "%PLIST%" echo                 ^<string^>com.ecgstudio.dicom-ecg^</string^>
>> "%PLIST%" echo                 ^<string^>public.xml^</string^>
>> "%PLIST%" echo             ^</array^>
>> "%PLIST%" echo             ^<key^>CFBundleTypeExtensions^</key^>
>> "%PLIST%" echo             ^<array^>
>> "%PLIST%" echo                 ^<string^>xml^</string^>
>> "%PLIST%" echo                 ^<string^>aecg^</string^>
>> "%PLIST%" echo                 ^<string^>hl7^</string^>
>> "%PLIST%" echo                 ^<string^>c8k^</string^>
>> "%PLIST%" echo                 ^<string^>ecg^</string^>
>> "%PLIST%" echo                 ^<string^>dcm^</string^>
>> "%PLIST%" echo                 ^<string^>dicom^</string^>
>> "%PLIST%" echo             ^</array^>
>> "%PLIST%" echo         ^</dict^>
>> "%PLIST%" echo     ^</array^>
>> "%PLIST%" echo     ^<key^>UTExportedTypeDeclarations^</key^>
>> "%PLIST%" echo     ^<array^>
>> "%PLIST%" echo         ^<dict^>
>> "%PLIST%" echo             ^<key^>UTTypeIdentifier^</key^>
>> "%PLIST%" echo             ^<string^>com.ecgstudio.ecg^</string^>
>> "%PLIST%" echo             ^<key^>UTTypeDescription^</key^>
>> "%PLIST%" echo             ^<string^>ECG document^</string^>
>> "%PLIST%" echo             ^<key^>UTTypeConformsTo^</key^>
>> "%PLIST%" echo             ^<array^>
>> "%PLIST%" echo                 ^<string^>public.data^</string^>
>> "%PLIST%" echo             ^</array^>
>> "%PLIST%" echo             ^<key^>UTTypeTagSpecification^</key^>
>> "%PLIST%" echo             ^<dict^>
>> "%PLIST%" echo                 ^<key^>public.filename-extension^</key^>
>> "%PLIST%" echo                 ^<array^>
>> "%PLIST%" echo                     ^<string^>aecg^</string^>
>> "%PLIST%" echo                     ^<string^>hl7^</string^>
>> "%PLIST%" echo                     ^<string^>c8k^</string^>
>> "%PLIST%" echo                     ^<string^>ecg^</string^>
>> "%PLIST%" echo                 ^</array^>
>> "%PLIST%" echo             ^</dict^>
>> "%PLIST%" echo         ^</dict^>
>> "%PLIST%" echo         ^<dict^>
>> "%PLIST%" echo             ^<key^>UTTypeIdentifier^</key^>
>> "%PLIST%" echo             ^<string^>com.ecgstudio.dicom-ecg^</string^>
>> "%PLIST%" echo             ^<key^>UTTypeDescription^</key^>
>> "%PLIST%" echo             ^<string^>DICOM ECG document^</string^>
>> "%PLIST%" echo             ^<key^>UTTypeConformsTo^</key^>
>> "%PLIST%" echo             ^<array^>
>> "%PLIST%" echo                 ^<string^>public.data^</string^>
>> "%PLIST%" echo             ^</array^>
>> "%PLIST%" echo             ^<key^>UTTypeTagSpecification^</key^>
>> "%PLIST%" echo             ^<dict^>
>> "%PLIST%" echo                 ^<key^>public.filename-extension^</key^>
>> "%PLIST%" echo                 ^<array^>
>> "%PLIST%" echo                     ^<string^>dcm^</string^>
>> "%PLIST%" echo                     ^<string^>dicom^</string^>
>> "%PLIST%" echo                 ^</array^>
>> "%PLIST%" echo             ^</dict^>
>> "%PLIST%" echo         ^</dict^>
>> "%PLIST%" echo     ^</array^>
>> "%PLIST%" echo ^</dict^>
>> "%PLIST%" echo ^</plist^>
exit /b %errorlevel%

:usage
echo Usage: build_macos_from_windows.bat [apple-silicon^|universal^|intel]
echo.
echo This script is only for experimental Windows cross-builds.
echo The reliable path is: ./build_macos.sh universal  # on macOS
exit /b 2

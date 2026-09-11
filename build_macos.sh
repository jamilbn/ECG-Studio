#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ECG Studio"
BINARY_NAME="ecg-studio"
BUNDLE_ID="${BUNDLE_ID:-com.ecgstudio.app}"
MODE="${1:-apple-silicon}"

usage() {
    cat <<'EOF'
Usage: ./build_macos.sh [apple-silicon|universal|intel|native]

apple-silicon  Build arm64 only. Good for M-series Macs.
universal      Build Universal 2, arm64 + x86_64.
intel          Build x86_64 only.
native         Build only for the current Mac architecture.

Optional environment:
  BUNDLE_ID=com.example.ecgstudio
  MACOSX_DEPLOYMENT_TARGET=12.0
  CARGO_TARGET_DIR=/path/to/target
EOF
}

case "$MODE" in
    --help|-h|help)
        usage
        exit 0
        ;;
    --apple-silicon|arm64|aarch64)
        MODE="apple-silicon"
        ;;
    --universal|universal2)
        MODE="universal"
        ;;
    --intel|x64|x86_64)
        MODE="intel"
        ;;
    --native)
        MODE="native"
        ;;
esac

if [[ "$MODE" != "apple-silicon" && "$MODE" != "universal" && "$MODE" != "intel" && "$MODE" != "native" ]]; then
    usage
    exit 2
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This script must run on macOS. Use build_macos_from_windows.bat only for experimental cross-builds."
    exit 1
fi

for tool in cargo rustup xcrun xcode-select; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Missing required tool: $tool"
        exit 1
    fi
done

if ! xcode-select -p >/dev/null 2>&1; then
    echo "Xcode Command Line Tools are not configured. Run: xcode-select --install"
    exit 1
fi

if ! xcrun --find clang >/dev/null 2>&1; then
    echo "clang was not found through xcrun. Run: xcode-select --install"
    exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

APP_VERSION="$(awk -F '"' '/^version =/{ print $2; exit }' Cargo.toml)"
APP_VERSION="${APP_VERSION:-0.1.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/ECGStudio/target-macos}"

DIST_DIR="$ROOT_DIR/dist/macos"
APP_DIR="$DIST_DIR/$APP_NAME.app"
BUNDLE_BIN="$APP_DIR/Contents/MacOS/$BINARY_NAME"
RESOURCES_DIR="$APP_DIR/Contents/Resources"

case "$MODE" in
    apple-silicon)
        TARGETS=("aarch64-apple-darwin")
        ;;
    intel)
        TARGETS=("x86_64-apple-darwin")
        ;;
    universal)
        TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
        ;;
    native)
        case "$(uname -m)" in
            arm64)
                TARGETS=("aarch64-apple-darwin")
                ;;
            x86_64)
                TARGETS=("x86_64-apple-darwin")
                ;;
            *)
                echo "Unsupported Mac architecture: $(uname -m)"
                exit 1
                ;;
        esac
        ;;
esac

build_target() {
    local target="$1"
    echo "Building $target..."
    rustup target add "$target"
    cargo build --release --target "$target"
}

write_info_plist() {
    cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>$BINARY_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>
    <string>$APP_VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$APP_VERSION</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>ECG document</string>
            <key>CFBundleTypeRole</key>
            <string>Viewer</string>
            <key>LSHandlerRank</key>
            <string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>com.ecgstudio.ecg</string>
                <string>com.ecgstudio.dicom-ecg</string>
                <string>public.xml</string>
            </array>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>xml</string>
                <string>aecg</string>
                <string>hl7</string>
                <string>c8k</string>
                <string>ecg</string>
                <string>dcm</string>
                <string>dicom</string>
            </array>
        </dict>
    </array>
    <key>UTExportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key>
            <string>com.ecgstudio.ecg</string>
            <key>UTTypeDescription</key>
            <string>ECG document</string>
            <key>UTTypeConformsTo</key>
            <array>
                <string>public.data</string>
            </array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array>
                    <string>aecg</string>
                    <string>hl7</string>
                    <string>c8k</string>
                    <string>ecg</string>
                </array>
            </dict>
        </dict>
        <dict>
            <key>UTTypeIdentifier</key>
            <string>com.ecgstudio.dicom-ecg</string>
            <key>UTTypeDescription</key>
            <string>DICOM ECG document</string>
            <key>UTTypeConformsTo</key>
            <array>
                <string>public.data</string>
            </array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array>
                    <string>dcm</string>
                    <string>dicom</string>
                </array>
            </dict>
        </dict>
    </array>
</dict>
</plist>
PLIST
}

make_icon() {
    local source_icon="$ROOT_DIR/assets/app-icon.png"
    local iconset="$DIST_DIR/AppIcon.iconset"

    if [[ ! -f "$source_icon" ]]; then
        return
    fi

    cp "$source_icon" "$RESOURCES_DIR/app-icon.png"

    if ! command -v sips >/dev/null 2>&1 || ! xcrun --find iconutil >/dev/null 2>&1; then
        echo "sips/iconutil not available; app bundle will include PNG but no ICNS icon."
        return
    fi

    rm -rf "$iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        sips -z "$size" "$size" "$source_icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
        retina_size=$((size * 2))
        sips -z "$retina_size" "$retina_size" "$source_icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done

    xcrun iconutil -c icns "$iconset" -o "$RESOURCES_DIR/AppIcon.icns"
    rm -rf "$iconset"
}

for target in "${TARGETS[@]}"; do
    build_target "$target"
done

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$RESOURCES_DIR"
write_info_plist
make_icon

if [[ "$MODE" == "universal" ]]; then
    xcrun lipo -create \
        "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/$BINARY_NAME" \
        "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/$BINARY_NAME" \
        -output "$BUNDLE_BIN"
else
    cp "$CARGO_TARGET_DIR/${TARGETS[0]}/release/$BINARY_NAME" "$BUNDLE_BIN"
fi

chmod +x "$BUNDLE_BIN"

if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$APP_DIR" >/dev/null
fi

ZIP_PATH="$DIST_DIR/ECG-Studio-macos-$MODE.zip"
if command -v ditto >/dev/null 2>&1; then
    ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"
fi

echo "$APP_DIR"
if [[ -f "$ZIP_PATH" ]]; then
    echo "$ZIP_PATH"
fi

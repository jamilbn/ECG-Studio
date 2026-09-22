#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This script must run on Linux."
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

APP_VERSION="$(awk -F '"' '/^version =/{ print $2; exit }' Cargo.toml)"
APP_VERSION="${APP_VERSION:-0.1.0}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ECGStudio/target-linux}"

cargo build --release

DIST_DIR="$ROOT_DIR/dist/linux"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

cp "$CARGO_TARGET_DIR/release/ecg-studio" "$DIST_DIR/ecg-studio"
chmod 755 "$DIST_DIR/ecg-studio"
cp "$ROOT_DIR/assets/app-icon.png" "$DIST_DIR/ecg-studio.png"
cp "$ROOT_DIR/packaging/linux/ecg-studio.desktop" "$DIST_DIR/ecg-studio.desktop"
cp "$ROOT_DIR/packaging/linux/ecg-studio-mime.xml" "$DIST_DIR/ecg-studio-mime.xml"
cp "$ROOT_DIR/packaging/linux/99-ecg-studio.rules" "$DIST_DIR/99-ecg-studio.rules"
cp "$ROOT_DIR/packaging/linux/install.sh" "$DIST_DIR/install.sh"
cp "$ROOT_DIR/packaging/linux/uninstall.sh" "$DIST_DIR/uninstall.sh"
chmod 755 "$DIST_DIR/install.sh" "$DIST_DIR/uninstall.sh"

ARCHIVE="$ROOT_DIR/dist/ECGStudio-linux-$APP_VERSION.tar.gz"
tar -C "$DIST_DIR" -czf "$ARCHIVE" .

echo "$DIST_DIR"
echo "$ARCHIVE"

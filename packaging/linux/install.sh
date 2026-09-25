#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ -f "$SCRIPT_DIR/ecg-studio" ]]; then
  BIN_SRC="$SCRIPT_DIR/ecg-studio"
elif [[ -n "${CARGO_TARGET_DIR:-}" && -f "$CARGO_TARGET_DIR/release/ecg-studio" ]]; then
  BIN_SRC="$CARGO_TARGET_DIR/release/ecg-studio"
elif [[ -f "$ROOT_DIR/target/release/ecg-studio" ]]; then
  BIN_SRC="$ROOT_DIR/target/release/ecg-studio"
else
  echo "ecg-studio binary not found. Build with ./build_linux.sh or cargo build --release." >&2
  exit 1
fi

if [[ -f "$SCRIPT_DIR/ecg-studio.png" ]]; then
  ICON_SRC="$SCRIPT_DIR/ecg-studio.png"
else
  ICON_SRC="$ROOT_DIR/assets/app-icon.png"
fi

DESKTOP_SRC="$SCRIPT_DIR/ecg-studio.desktop"
MIME_SRC="$SCRIPT_DIR/ecg-studio-mime.xml"
UDEV_EARLY_SRC="$SCRIPT_DIR/40-ecg-studio.rules"
UDEV_SRC="$SCRIPT_DIR/99-ecg-studio.rules"

if [[ "$(id -u)" -eq 0 ]]; then
  PREFIX="${PREFIX:-/usr/local}"
else
  PREFIX="${PREFIX:-$HOME/.local}"
fi

BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
ICON_DIR="$PREFIX/share/icons/hicolor/256x256/apps"
MIME_DIR="$PREFIX/share/mime/packages"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR" "$MIME_DIR"
install -m 755 "$BIN_SRC" "$BIN_DIR/ecg-studio"
install -m 644 "$ICON_SRC" "$ICON_DIR/ecg-studio.png"
install -m 644 "$MIME_SRC" "$MIME_DIR/ecg-studio.xml"

sed "s|^Exec=.*|Exec=$BIN_DIR/ecg-studio %f|" "$DESKTOP_SRC" > "$APP_DIR/ecg-studio.desktop"
chmod 644 "$APP_DIR/ecg-studio.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi
if command -v update-mime-database >/dev/null 2>&1 && [[ -d "$PREFIX/share/mime" ]]; then
  update-mime-database "$PREFIX/share/mime" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1 && [[ -d "$PREFIX/share/icons/hicolor" ]]; then
  gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Installed ECG Studio to $BIN_DIR/ecg-studio"
echo "Desktop entry: $APP_DIR/ecg-studio.desktop"
echo "File associations: $MIME_DIR/ecg-studio.xml"

if [[ "$(id -u)" -eq 0 ]]; then
  install -m 644 "$UDEV_EARLY_SRC" /etc/udev/rules.d/40-ecg-studio.rules
  install -m 644 "$UDEV_SRC" /etc/udev/rules.d/99-ecg-studio.rules
  if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger >/dev/null 2>&1 || true
  fi
  echo "Installed live USB udev rules to /etc/udev/rules.d/40-ecg-studio.rules and 99-ecg-studio.rules"
else
  echo "Live USB capture needs device permissions."
  echo "Re-run with sudo to install udev rules, or copy:"
  echo "  sudo install -m 644 \"$UDEV_EARLY_SRC\" /etc/udev/rules.d/40-ecg-studio.rules"
  echo "  sudo install -m 644 \"$UDEV_SRC\" /etc/udev/rules.d/99-ecg-studio.rules"
  echo "  sudo udevadm control --reload-rules && sudo udevadm trigger"
fi

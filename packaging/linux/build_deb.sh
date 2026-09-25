#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This script must run on Linux."
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

APP_VERSION="$(awk -F '"' '/^version =/{ print $2; exit }' Cargo.toml)"
APP_VERSION="${APP_VERSION:-0.1.0}"
ARCH="$(dpkg --print-architecture)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ECGStudio/target-linux}"

BIN_SRC="${1:-$CARGO_TARGET_DIR/release/ecg-studio}"
if [[ ! -f "$BIN_SRC" ]]; then
  echo "ecg-studio binary not found at $BIN_SRC" >&2
  exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p \
  "$STAGE/DEBIAN" \
  "$STAGE/usr/bin" \
  "$STAGE/usr/share/applications" \
  "$STAGE/usr/share/icons/hicolor/256x256/apps" \
  "$STAGE/usr/share/mime/packages" \
  "$STAGE/etc/udev/rules.d"

install -m 755 "$BIN_SRC" "$STAGE/usr/bin/ecg-studio"
install -m 644 "$ROOT_DIR/assets/app-icon.png" \
  "$STAGE/usr/share/icons/hicolor/256x256/apps/ecg-studio.png"
install -m 644 "$ROOT_DIR/packaging/linux/ecg-studio.desktop" \
  "$STAGE/usr/share/applications/ecg-studio.desktop"
install -m 644 "$ROOT_DIR/packaging/linux/ecg-studio-mime.xml" \
  "$STAGE/usr/share/mime/packages/ecg-studio.xml"
install -m 644 "$ROOT_DIR/packaging/linux/40-ecg-studio.rules" \
  "$STAGE/etc/udev/rules.d/40-ecg-studio.rules"
install -m 644 "$ROOT_DIR/packaging/linux/99-ecg-studio.rules" \
  "$STAGE/etc/udev/rules.d/99-ecg-studio.rules"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: ecg-studio
Version: ${APP_VERSION}
Section: science
Priority: optional
Architecture: ${ARCH}
Maintainer: jamilbn <jamilbn@users.noreply.github.com>
Depends: libc6, libdbus-1-3, libegl1, libfontconfig1, libx11-6, libxcb1, libxcursor1, libxkbcommon0, libxrandr2, libxi6, libwayland-client0, libwayland-cursor0, libwayland-egl1
Description: Open, preview, print and export ECG files
 Desktop app for reading ECG files, filtering the signal, previewing a page,
 and exporting a text summary.
EOF

cat > "$STAGE/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
  fi
  if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database /usr/share/mime >/dev/null 2>&1 || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || true
  fi
  if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger >/dev/null 2>&1 || true
  fi
fi
EOF

cat > "$STAGE/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "remove" ] && command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  udevadm trigger >/dev/null 2>&1 || true
fi
EOF

chmod 755 "$STAGE/DEBIAN/postinst" "$STAGE/DEBIAN/prerm"

mkdir -p "$ROOT_DIR/dist"
DEB="$ROOT_DIR/dist/ecg-studio_${APP_VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$STAGE" "$DEB"
echo "$DEB"

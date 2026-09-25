#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" -eq 0 ]]; then
  PREFIX="${PREFIX:-/usr/local}"
else
  PREFIX="${PREFIX:-$HOME/.local}"
fi

rm -f "$PREFIX/bin/ecg-studio"
rm -f "$PREFIX/share/applications/ecg-studio.desktop"
rm -f "$PREFIX/share/icons/hicolor/256x256/apps/ecg-studio.png"
rm -f "$PREFIX/share/mime/packages/ecg-studio.xml"

if [[ "$(id -u)" -eq 0 ]]; then
  rm -f /etc/udev/rules.d/40-ecg-studio.rules /etc/udev/rules.d/99-ecg-studio.rules
  if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || true
    udevadm trigger >/dev/null 2>&1 || true
  fi
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
fi
if command -v update-mime-database >/dev/null 2>&1 && [[ -d "$PREFIX/share/mime" ]]; then
  update-mime-database "$PREFIX/share/mime" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1 && [[ -d "$PREFIX/share/icons/hicolor" ]]; then
  gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Removed ECG Studio from $PREFIX"

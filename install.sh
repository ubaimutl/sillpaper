#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
APP_DIR="${PREFIX}/share/applications"
ICON_DIR="${PREFIX}/share/icons/hicolor/scalable/apps"
METAINFO_DIR="${PREFIX}/share/metainfo"

echo "Building Sillpaper (release)..."
cargo build --release --locked --manifest-path "${ROOT_DIR}/Cargo.toml" --bin sillpaper

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}" "${METAINFO_DIR}"

install -Dm755 "${ROOT_DIR}/target/release/sillpaper" "${BIN_DIR}/sillpaper"
install -Dm644 "${ROOT_DIR}/assets/dev.ubai.Sillpaper.svg" \
    "${ICON_DIR}/dev.ubai.Sillpaper.svg"
install -Dm644 "${ROOT_DIR}/assets/dev.ubai.Sillpaper.desktop" \
    "${APP_DIR}/dev.ubai.Sillpaper.desktop"
install -Dm644 "${ROOT_DIR}/assets/dev.ubai.Sillpaper.metainfo.xml" \
    "${METAINFO_DIR}/dev.ubai.Sillpaper.metainfo.xml"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APP_DIR}" || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${PREFIX}/share/icons/hicolor" || true
fi

echo "Installed Sillpaper:"
echo "  Binary  : ${BIN_DIR}/sillpaper"
echo "  Icon    : ${ICON_DIR}/dev.ubai.Sillpaper.svg"
echo "  Desktop : ${APP_DIR}/dev.ubai.Sillpaper.desktop"
echo "  Metainfo: ${METAINFO_DIR}/dev.ubai.Sillpaper.metainfo.xml"

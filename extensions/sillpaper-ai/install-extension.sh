#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
EXTENSIONS_DIR="${SILLPAPER_EXTENSIONS_DIR:-${STILLWRITE_EXTENSIONS_DIR:-}}"
if [[ -z "$EXTENSIONS_DIR" ]]; then
    EXTENSIONS_DIR="$CONFIG_DIR/sillpaper/extensions"
    if [[ ! -e "$EXTENSIONS_DIR" && -d "$CONFIG_DIR/stillwrite/extensions" ]]; then
        EXTENSIONS_DIR="$CONFIG_DIR/stillwrite/extensions"
    fi
fi
EXT_DIR="$EXTENSIONS_DIR/sillpaper-ai"

echo "Building standalone Rust extension sillpaper-ai (release)..."
cargo build --release --locked --manifest-path "$SCRIPT_DIR/Cargo.toml"

mkdir -p "$EXT_DIR"
cp "$SCRIPT_DIR/manifest.json" "$EXT_DIR/manifest.json"
install -m 755 "$SCRIPT_DIR/target/release/sillpaper-ai" "$EXT_DIR/sillpaper-ai"

echo "Installed native Rust Sillpaper AI extension into:"
echo "  $EXT_DIR"
echo "Open Sillpaper → Settings → Extensions (External) to enable and configure."

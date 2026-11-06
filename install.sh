#!/bin/sh
# Build ymp and install it somewhere on PATH.
#   ./install.sh              -> ~/.local/bin
#   YMP_PREFIX=/usr/local ./install.sh
set -eu

cd "$(dirname "$0")"
DEST="${YMP_INSTALL_DIR:-$HOME/.local/bin}"

cargo build --release

mkdir -p "$DEST"
cp target/release/ymp "$DEST/ymp"
chmod +x "$DEST/ymp"

# macOS attaches com.apple.provenance to freshly copied binaries in some
# directories, and the kernel then kills them on launch (exit 137 / "Killed: 9").
if command -v xattr >/dev/null 2>&1; then
    xattr -c "$DEST/ymp" 2>/dev/null || true
fi

printf 'installed %s -> %s\n' "$("$DEST/ymp" -v)" "$DEST/ymp"

case ":$PATH:" in
    *":$DEST:"*) ;;
    *) printf 'note: %s is not on PATH, add it to your shell profile\n' "$DEST" ;;
esac

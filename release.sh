#!/bin/sh
# Build and package a release tarball for the machine you are on.
#   ./release.sh        -> dist/ymp-<version>-<target>.tar.gz (+ sha256)
# The tarball holds the binary, README, LICENSE and a short INSTALL.txt, which is
# what a GitHub release wants. It is not universal: run this on each
# architecture you intend to ship.
set -eu
cd "$(dirname "$0")"

NAME=ymp
VER=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
TARGET=$(rustc -vV | sed -n 's/^host: //p')

cargo build --release

STAGE="dist/$NAME-$VER-$TARGET"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "target/release/$NAME" README.md LICENSE "$STAGE/"

cat > "$STAGE/INSTALL.txt" <<TXT
$NAME $VER ($TARGET)

  mkdir -p ~/.local/bin
  cp $NAME ~/.local/bin/
  chmod +x ~/.local/bin/$NAME
  xattr -c ~/.local/bin/$NAME     # macOS: clear the quarantine flag from the download
  $NAME -v

needs: a terminal with the kitty or iTerm2 graphics protocol (falls back to half blocks)
需要：支持 Kitty 或 iTerm2 图形协议的终端（没有就退回半块字符画）

Inside the UI press ? for the key map.
TXT

tar -czf "dist/$NAME-$VER-$TARGET.tar.gz" -C dist "$NAME-$VER-$TARGET"
rm -rf "$STAGE"

ls -lh "dist/$NAME-$VER-$TARGET.tar.gz"
shasum -a 256 "dist/$NAME-$VER-$TARGET.tar.gz"

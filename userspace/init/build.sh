#!/usr/bin/env bash
# userspace/init/build.sh — standalone build script for rustos-init.
#
# Usage:
#   ./build.sh               # debug build
#   ./build.sh --release     # release build (default for xtask)
#   ./build.sh --pack        # build + pack into ../../initramfs.cpio
#
# Requires:
#   rustup target add x86_64-unknown-linux-musl
#   (cargo xtask build-init handles this automatically)

set -euo pipefail

RELEASE=""
PACK=false

for arg in "$@"; do
    case "$arg" in
        --release) RELEASE="--release" ;;
        --pack)    PACK=true ;;
        *) echo "[build.sh] unknown arg: $arg" >&2; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")")" && pwd""
cd "$SCRIPT_DIR"

echo "[build.sh] adding musl target..."
rustup target add x86_64-unknown-linux-musl 2>/dev/null || true

echo "[build.sh] building rustos-init $RELEASE..."
cargo build $RELEASE

PROFILE_DIR="debug"
[[ -n "$RELEASE" ]] && PROFILE_DIR="release"

INIT_BIN="$SCRIPT_DIR/target/x86_64-unknown-linux-musl/$PROFILE_DIR/init"
echo "[build.sh] init binary: $INIT_BIN"
ls -lh "$INIT_BIN"
file "$INIT_BIN"

if $PACK; then
    STAGING="$(mktemp -d)"
    echo "[build.sh] staging initramfs at $STAGING..."
    install -D -m 755 "$INIT_BIN" "$STAGING/init"
    mkdir -p "$STAGING"/{bin,dev,proc,sys,tmp,etc}
    echo 'NAME=RustOS' > "$STAGING/etc/os-release"
    CPIO_OUT="$(realpath "$SCRIPT_DIR/../../initramfs.cpio")"
    (cd "$STAGING" && find . | sort | cpio --create --format=newc --quiet > "$CPIO_OUT")
    rm -rf "$STAGING"
    echo "[build.sh] packed initramfs: $CPIO_OUT"
    ls -lh "$CPIO_OUT"
fi

#!/usr/bin/env bash
# scripts/ci/build-userspace.sh
#
# Builds initramfs userspace for the given ARCH through RustOS's canonical
# Cargo/xtask automation.
#
# Usage:
#   ARCH=aarch64 ./scripts/ci/build-userspace.sh
#
# Environment variables:
#   ARCH          x86_64 | aarch64 | riscv64  (default: x86_64)

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCH="${ARCH:-x86_64}"

case "$ARCH" in
  x86_64|aarch64|riscv64) ;;
  *)
    echo "[userspace] ERROR: unsupported ARCH='${ARCH}'" >&2
    echo "[userspace] Valid architectures: x86_64 aarch64 riscv64" >&2
    exit 1
    ;;
esac

echo "[userspace] Building initramfs userspace for ARCH=${ARCH} via cargo xtask..."
cd "$ROOT_DIR"
cargo xtask build-init --arch "$ARCH"
echo "[userspace] Done. Output: ${ROOT_DIR}/initramfs.cpio"

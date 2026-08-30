#!/usr/bin/env bash
# tools/build_userspace.sh
#
# Compatibility wrapper for the Cargo/xtask userspace build.
#
# Usage:
#   ./tools/build_userspace.sh              # aarch64 (default)
#   ./tools/build_userspace.sh x86_64       # x86_64
#   ./tools/build_userspace.sh riscv64      # RISC-V 64
#
# Output:
#   initramfs.cpio   (CPIO newc archive — pass to QEMU as -initrd)

set -euo pipefail

ARCH=${1:-aarch64}
VALID_ARCHS=("aarch64" "x86_64" "riscv64")

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ ! " ${VALID_ARCHS[*]} " =~ " ${ARCH} " ]]; then
  echo "[build_userspace] ERROR: unsupported ARCH='$ARCH'" >&2
  echo "[build_userspace] Valid architectures: ${VALID_ARCHS[*]}" >&2
  exit 1
fi

echo "[build_userspace] Building initramfs userspace for ARCH=${ARCH} via cargo xtask..."
cd "$REPO_ROOT"
cargo xtask build-init --arch "$ARCH"
echo "[build_userspace] Done. Output: ${REPO_ROOT}/initramfs.cpio"

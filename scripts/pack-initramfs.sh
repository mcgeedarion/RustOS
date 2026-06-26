#!/usr/bin/env bash
# scripts/pack-initramfs.sh
#
# Pack a minimal initramfs CPIO archive for RustOS.
#
# Usage:
#   ./scripts/pack-initramfs.sh [STAGING_DIR [OUTPUT]]
#
# Defaults:
#   STAGING_DIR  build/initramfs-staging
#   OUTPUT       build/initramfs.cpio
#
# The script:
#   1. Creates the staging layout (bin, dev, etc, proc, run, sys, tmp).
#   2. Copies the pre-built init binary as ./init at mode 0755.
#   3. Writes a minimal /etc/hostname.
#   4. Packs everything as an UNCOMPRESSED CPIO newc archive.
#
# Why uncompressed?
#   RustOS does not yet include a decompressor.  QEMU's -initrd flag and
#   the UEFI boot stub both call set_initramfs_range() with the raw
#   physical address and byte length of whatever memory region the firmware
#   hands them.  Compressed formats (gzip, lz4, zstd) would silently fail
#   the `070701` magic check in src/init/initramfs/mod.rs and the kernel
#   would loop printing the WARN message instead of finding /init.
#
# Requires: cpio, find, install (coreutils)

set -euo pipefail

STAGING="${1:-build/initramfs-staging}"
OUTPUT="${2:-build/initramfs.cpio}"

INIT_BIN="build/userspace/init"

if [[ ! -f "$INIT_BIN" ]]; then
    echo "error: $INIT_BIN not found." >&2
    echo "       Build userspace first:" >&2
    echo "         cargo xtask mkinitramfs --arch x86_64" >&2
    echo "       or:  make -C userspace" >&2
    exit 1
fi

if ! command -v cpio >/dev/null 2>&1; then
    echo "error: 'cpio' not found on PATH." >&2
    echo "       Install with: sudo apt install cpio" >&2
    exit 1
fi

# ── Staging layout ────────────────────────────────────────────────────────────
rm -rf "$STAGING"
mkdir -p \
    "${STAGING}/bin" \
    "${STAGING}/dev" \
    "${STAGING}/etc" \
    "${STAGING}/proc" \
    "${STAGING}/run" \
    "${STAGING}/sys" \
    "${STAGING}/tmp"

# /init — the PID 1 binary, must be statically linked ELF
install -m 0755 "$INIT_BIN" "${STAGING}/init"

# Minimal /etc/hostname so init can identify the system
echo "rustos" > "${STAGING}/etc/hostname"

# /etc/os-release for completeness
cat > "${STAGING}/etc/os-release" <<'EOF'
NAME=RustOS
ID=rustos
VERSION=0.1.0
PRETTY_NAME="RustOS 0.1.0"
EOF

# ── Pack ──────────────────────────────────────────────────────────────────────
mkdir -p "$(dirname "$OUTPUT")"

# sort guarantees deterministic archive ordering across filesystems
(
    cd "$STAGING"
    find . | sort | cpio --quiet -o -H newc
) > "$OUTPUT"

SIZE=$(wc -c < "$OUTPUT")
echo "initramfs: packed ${SIZE} bytes → ${OUTPUT}"
echo "initramfs: contents:"
cpio -itv < "$OUTPUT" 2>/dev/null | sed 's/^/  /'

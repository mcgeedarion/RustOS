#!/usr/bin/env bash
# scripts/riscv-uefi-boot.sh
# Boot RustOS under RISC-V UEFI (EDK2/OvmfPkg RiscVVirt) via QEMU virt machine.
#
# Prerequisites:
#   - QEMU >= 8.1  (qemu-system-riscv64)
#   - EDK2 built with: build -a RISCV64 --buildtarget RELEASE \
#         -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5
#   - RISCV_VIRT_CODE.fd and RISCV_VIRT_VARS.fd produced by that build
#   - A disk image with a FAT32 EFI System Partition containing:
#       EFI/BOOT/BOOTRISCV64.EFI  (your UEFI bootloader / kernel stub)
#
# Usage:
#   ./scripts/riscv-uefi-boot.sh [DISK_IMAGE]

set -euo pipefail

# ─── Config ───────────────────────────────────────────────────────────────────
QEMU=${QEMU:-qemu-system-riscv64}
FW_DIR=${FW_DIR:-$(dirname "$0")/../firmware/riscv64}
CODE_FD="${FW_DIR}/RISCV_VIRT_CODE.fd"
VARS_FD="${FW_DIR}/RISCV_VIRT_VARS.fd"
DISK=${1:-${FW_DIR}/../disk/rustos-riscv64.img}
MEM=${MEM:-4096}
SMP=${SMP:-2}
# ──────────────────────────────────────────────────────────────────────────────

# 1. Verify QEMU version >= 8.1
QEMU_VER=$("${QEMU}" --version | head -1 | grep -oP '\d+\.\d+' | head -1)
QEMU_MAJOR=$(echo "${QEMU_VER}" | cut -d. -f1)
QEMU_MINOR=$(echo "${QEMU_VER}" | cut -d. -f2)
if [[ "${QEMU_MAJOR}" -lt 8 ]] || ([[ "${QEMU_MAJOR}" -eq 8 ]] && [[ "${QEMU_MINOR}" -lt 1 ]]); then
    echo "ERROR: QEMU ${QEMU_VER} is too old. EDK2 RiscVVirt requires QEMU >= 8.1."
    exit 1
fi
echo "[ok] QEMU ${QEMU_VER}"

# 2. Verify pflash firmware images exist
for f in "${CODE_FD}" "${VARS_FD}"; do
    if [[ ! -f "${f}" ]]; then
        echo "ERROR: Missing firmware image: ${f}"
        echo "  Build EDK2: build -a RISCV64 --buildtarget RELEASE \\"
        echo "    -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5"
        exit 1
    fi
done
echo "[ok] pflash images: ${CODE_FD}, ${VARS_FD}"

# 3. Pad VARS image to 32 MiB (QEMU pflash requirement)
VARS_SIZE=$(stat -c%s "${VARS_FD}" 2>/dev/null || stat -f%z "${VARS_FD}")
if [[ "${VARS_SIZE}" -lt $((32 * 1024 * 1024)) ]]; then
    truncate -s 32M "${VARS_FD}"
    echo "[ok] VARS image padded to 32 MiB"
fi

# 4. Verify disk image exists and has an ESP
if [[ ! -f "${DISK}" ]]; then
    echo "ERROR: Disk image not found: ${DISK}"
    echo "  Create one with a FAT32 ESP containing EFI/BOOT/BOOTRISCV64.EFI"
    exit 1
fi
echo "[ok] disk image: ${DISK}"

# 5. Boot via qemu-system-riscv64 -M virt with pflash firmware
echo "[boot] Launching RISC-V UEFI..."
exec "${QEMU}" \
    -M virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
    -m "${MEM}" \
    -smp "${SMP}" \
    -cpu rv64 \
    -serial mon:stdio \
    -display none \
    -blockdev node-name=pflash0,driver=file,read-only=on,filename="${CODE_FD}" \
    -blockdev node-name=pflash1,driver=file,filename="${VARS_FD}" \
    -drive file="${DISK}",format=raw,if=virtio,id=hd0 \
    "$@"

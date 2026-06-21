# RISC-V UEFI Boot via EDK2 (QEMU virt)

This document describes the full workflow for booting RustOS under a RISC-V UEFI
environment using EDK2's `OvmfPkg/RiscVVirt` target and QEMU's `virt` machine.

## Firmware chain

```
QEMU virt machine
  └─ OpenSBI (M-mode)        ← provided by QEMU internally
       └─ EDK2 RiscVVirt (S-mode UEFI firmware)
            └─ UEFI Boot Manager
                 └─ EFI/BOOT/BOOTRISCV64.EFI  (your bootloader / kernel stub)
                      └─ RustOS kernel
```

## Step 1 — Verify QEMU ≥ 8.1

The EDK2 `RiscVVirt` target uses a two-pflash layout that requires QEMU 8.1 or
newer (or commit `7efd65423a`).

```bash
qemu-system-riscv64 --version
```

Expected output (minimum):

```
QEMU emulator version 8.1.x
```

## Step 2 — Build EDK2 with `RiscVVirtQemu.dsc`

```bash
# Clone EDK2
git clone --recurse-submodule https://github.com/tianocore/edk2.git
cd edk2

# Set up environment
export WORKSPACE=$(pwd)
export PACKAGES_PATH=$WORKSPACE
export GCC5_RISCV64_PREFIX=riscv64-linux-gnu-
export EDK_TOOLS_PATH=$WORKSPACE/BaseTools
source edksetup.sh --reconfig
make -C BaseTools
source edksetup.sh BaseTools

# Build RISC-V virt target
build -a RISCV64 --buildtarget RELEASE \
    -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc \
    -t GCC5
```

For Clang/LLVM, replace `-t GCC5` with `-t CLANGDWARF`.

The build outputs two pflash images:

```
Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd   # read-only firmware code
Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd   # writable NVRAM/vars store
```

Copy or symlink them to `firmware/riscv64/` in this repo for use by the launch
script.

## Step 3 — Use pflash CODE and VARS images

Both images are passed to QEMU as named block devices attached to `pflash0` and
`pflash1` of the `virt` machine. The CODE image is read-only; the VARS image
holds persistent UEFI variable storage across reboots.

```bash
truncate -s 32M firmware/riscv64/RISCV_VIRT_CODE.fd
truncate -s 32M firmware/riscv64/RISCV_VIRT_VARS.fd
```

> **Note:** Both pflash regions must be exactly 32 MiB for the QEMU virt
> machine. The `scripts/riscv-uefi-boot.sh` script handles this automatically.

## Step 4 — Put a RISC-V EFI binary on the ESP

Create a disk image with a FAT32 EFI System Partition. The UEFI fallback boot
path for RISC-V is:

```
\EFI\BOOT\BOOTRISCV64.EFI
```

Example using `mtools` / `dd`:

```bash
# Create a 256 MiB raw disk image
dd if=/dev/zero of=disk/rustos-riscv64.img bs=1M count=256

# Partition: GPT with one ESP
sgdisk -n 1:2048:+200M -t 1:ef00 -c 1:"EFI System" disk/rustos-riscv64.img

# Format ESP as FAT32 and install the EFI binary
mformat  -i disk/rustos-riscv64.img@@1M -F ::
mmd     -i disk/rustos-riscv64.img@@1M ::/EFI
mmd     -i disk/rustos-riscv64.img@@1M ::/EFI/BOOT
mcopy   -i disk/rustos-riscv64.img@@1M \
    target/riscv64/release/rustos.efi \
    ::/EFI/BOOT/BOOTRISCV64.EFI
```

## Step 5 — Boot under `qemu-system-riscv64 -M virt`

Use the provided script:

```bash
chmod +x scripts/riscv-uefi-boot.sh
./scripts/riscv-uefi-boot.sh disk/rustos-riscv64.img
```

Or launch QEMU directly:

```bash
qemu-system-riscv64 \
    -M virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
    -m 4096 -smp 2 \
    -cpu rv64 \
    -serial mon:stdio \
    -display none \
    -blockdev node-name=pflash0,driver=file,read-only=on,filename=firmware/riscv64/RISCV_VIRT_CODE.fd \
    -blockdev node-name=pflash1,driver=file,filename=firmware/riscv64/RISCV_VIRT_VARS.fd \
    -drive file=disk/rustos-riscv64.img,format=raw,if=virtio
```

> Do **not** pass the kernel via `-kernel`. That bypasses the UEFI firmware
> entirely. The EFI binary on the ESP is the correct entry point.

## Troubleshooting "Unsupported" errors

| Symptom | Likely cause | Fix |
|---|---|---|
| `Unsupported` at firmware init | QEMU < 8.1 | Upgrade QEMU |
| `map: No mapping found` in UEFI Shell | No ESP or wrong format | Reformat partition as FAT32, correct partition type GUID |
| Falls to UEFI Shell, no auto boot | `BOOTRISCV64.EFI` missing or wrong path | Check `\EFI\BOOT\BOOTRISCV64.EFI` on the ESP |
| `InvalidateDataCacheRange: RISC-V unsupported` | Wrong platform EDK2 build | Use `RiscVVirtQemu.dsc`, not a board-specific DSC |
| Variable store corruption | VARS image not writable | Make sure `RISCV_VIRT_VARS.fd` is not read-only |

## References

- [EDK2 OvmfPkg RiscVVirt README](https://github.com/tianocore/edk2/blob/master/OvmfPkg/RiscVVirt/README.md)
- [RISC-V UEFI EDK2 Docs](https://github.com/riscv-admin/riscv-uefi-edk2-docs)
- [RISC-V UEFI Protocol Specification](https://docs.riscv.org/reference/platform-software/uefi/_attachments/RISCV_UEFI_PROTOCOL-spec.pdf)
- [QEMU virt machine docs](https://www.qemu.org/docs/master/system/riscv/virt.html)

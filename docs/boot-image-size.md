# Boot image size

The x86_64 size workflow measures the entire `boot-x86_64.img` container after a release-boot build. It does not directly measure kernel code size, heap use, resident memory, or initramfs payload.

## Current image contract

xtask's partitioned FAT-image builder fixes `DISK_SECTORS` at 131,072 with 512-byte sectors, producing a 64 MiB container with GPT/ESP structures. `.github/workflows/image-size.yml` sets `IMAGE_SIZE_LIMIT_BYTES` to 67,108,864, the same container capacity.

Because the image is fixed-size, unchanged container bytes cannot demonstrate a smaller kernel. The builder separately rejects an EFI payload that will not fit the data-cluster allocation.

```bash
cargo xtask image --arch x86_64 --profile release-boot
stat -c '%n %s' boot-x86_64.img
stat -c '%n %s' target/esp/x86_64/EFI/BOOT/BOOTX64.EFI
```

These are measurement commands, not results from this documentation audit. Release-boot selects the minimal graph through xtask, so its payload size is not the size of the full OS.

## What to report

- **Container size**: Total `.img` bytes and configured capacity.
- **EFI payload**: Actual `BOOT*.EFI` or `rustos.efi` bytes from the same build.
- **Initramfs**: Archive size and whether it is embedded/staged for that profile.
- **Sections**: Code, readonly data, writable data, and zero-fill sections from a tool compatible with the artifact format.
- **Build identity**: Revision, target, features, profile, compiler, link/strip settings, and input archive fingerprint.

The workflow uploads `image-size.json` and build output for trend inspection. A smaller payload is only an optimization result after the same required behavior still passes.

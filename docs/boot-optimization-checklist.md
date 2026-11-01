# Boot optimization checklist

This checklist is a measurement workflow, not a list of completed optimizations. It applies after the selected profile compiles and its required behavior passes.

## Establish comparable evidence

- **Revision and profile**: Pin source, architecture, boot mode, feature set, compiler, and Cargo profile.
- **Runtime environment**: Record firmware, QEMU version/machine/CPU/acceleration, memory, disk layout, and host details.
- **Correctness**: Require the matching minimal or full execution gate before accepting a faster result.
- **Timing**: Validate single-boot markers, identify the counter/frequency, and keep raw ticks distinct from converted time.
- **Size**: Measure EFI payload separately from the fixed 64 MiB image container.
- **Repeatability**: Compare multiple equivalent runs and keep diagnostic/instrumentation differences visible.

## Inspect before changing

- **Initialization order**: Identify real dependencies before deferring a service or enabling interrupts earlier.
- **Logging**: Measure serial-output cost; do not remove markers or failure diagnostics needed by CI.
- **Allocation**: Distinguish slim bump heaps from the full reclaiming heap and PMM boot metadata.
- **Loading**: Separate CPIO/ELF inspection from actual mappings, scheduler activation, and user-mode transfer.
- **Linking**: Inspect sections and retained data before attributing size changes to LTO or strip settings.

## Current blockers

The full x86_64 graph fails compilation, RISC-V slim checks fail, and the documentation audit ran no QEMU timing experiment. Guest performance optimization is therefore not an achieved outcome of this refresh.

`boot-regression.sh` and the current timing parser do not share a generated JSON schema, and the image-size gate measures a fixed-capacity container. Resolve those evidence boundaries before presenting automated regression thresholds as operational guarantees.

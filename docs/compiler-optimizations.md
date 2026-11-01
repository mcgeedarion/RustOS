# Compiler and profile settings

This page records manifest settings at `08a14f6`, not benchmarked speedups. Compiler optimization, feature selection, artifact format, and runtime behavior are separate controls.

## Root profiles

| Profile | Manifest settings |
| --- | --- |
| `dev` | `opt-level = 0` |
| `release` | `opt-level = 3`, fat LTO, one codegen unit, `strip = false` |
| `release-boot` | Inherits release; `opt-level = "z"`, fat LTO, one codegen unit, `debug = 0`, debug assertions disabled, `strip = "debuginfo"` |
| `release-pgo-generate` | Inherits release; level 3, fat LTO, one codegen unit, overflow checks disabled |

`strip = "debuginfo"` is not the same as removing all symbol names. The release-pgo-generate profile's name does not enable LLVM profile instrumentation by itself.

## xtask versus raw Cargo

xtask supports dev/debug, release, and release-boot profile selectors. For release-boot it also applies `--no-default-features --features release-boot,boot_minimal`, producing the minimal graph.

```bash
cargo xtask build --arch x86_64 --profile release-boot
cargo xtask build --arch x86_64 --profile release --features boot_minimal
```

Raw Cargo `--profile release-boot` does not reproduce that feature selection automatically. Cargo features are additive; an empty marker feature does not exclude other debug/test features.

## PGO, instrumentation, and counter evidence

No automatically connected PGO collection/merge/use pipeline is established by the root manifest. A real PGO experiment needs explicit compiler instrumentation, representative guest execution, collected profile data, and a compatible rebuild using that data.

Likewise, ftrace hook source does not insert compiler instrumentation by itself, and syscall tracing adds measurement behavior that changes the workload. Keep instrumented diagnostic builds separate from uninstrumented comparison images.

## Optimization policy

Fix correctness and profile identity before optimizing. Report same-workload, same-target measurements and preserve build commands, rather than copying historical percentage or boot-time claims into the current docs.

Do not enable host-specific instructions indiscriminately for portable firmware images. The architecture boundary, target JSONs, Cargo flags, firmware loader, and actual CPU model must agree.

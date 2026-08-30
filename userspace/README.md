# RustOS Userspace

This directory contains early userspace binaries and support crates for RustOS. Most programs are small `no_std` Rust targets used for boot and syscall validation.

## Architecture

All userspace binaries are built as a Cargo workspace with cross-compilation support for:
- `x86_64-unknown-none`
- `aarch64-unknown-none`

## Components

| Component | Description | Status |
|-----------|-------------|--------|
| `init` | Early PID 1 experiment for initramfs handoff | Partial |
| `hello` | Minimal userspace hello-world binary | Partial |
| `smoke` | Rust syscall smoke-test binary | Partial |
| `kmtest` | Userspace launcher/companion for kernel tests | Experimental |
| `libc-shim` | Small POSIX-style FFI shim for userspace experiments | Partial |
| `drivers/virtio_net` | Userspace virtio-net experiment | Experimental |

## Building

### Via xtask (recommended)

```bash
# Build all userspace binaries for x86_64
cargo xtask build-init --arch x86_64

# Build for AArch64
cargo xtask build-init --arch aarch64
```

### Direct Cargo build

```bash
cd userspace

# Build for x86_64
cargo build --release --target x86_64-unknown-none

# Build for AArch64
cargo build --release --target aarch64-unknown-none
```

## Output Locations

After building, binaries are located at:
```
userspace/target/<target>/release/
├── init        # PID 1 init process
├── hello       # Hello world binary
├── smoke       # Smoke test binary
└── kmtest      # Kernel test runner
```

## Running Tests

The `smoke` binary performs comprehensive syscall testing:
- write to stdout/stderr
- open/close /dev/null and /dev/zero
- fork/wait with exit status verification
- errno handling for invalid operations

Run in QEMU:
```bash
cargo xtask run --arch x86_64 --initrd
```

## Migration from C

This workspace replaces the previous C-based Makefile build system. All binaries have been rewritten in pure Rust with:
- No libc dependency
- Direct syscall wrappers using inline assembly
- Support for x86_64, AArch64, and RISC-V 64
- Proper panic handlers for no_std environments

## Adding New Binaries

1. Create a new directory under `userspace/<name>/`
2. Add `Cargo.toml`:
```toml
[package]
name = "rustos-<name>"
version.workspace = true
edition.workspace = true

[[bin]]
name = "<name>"
path = "src/main.rs"

[dependencies]
```

3. Add to workspace members in `userspace/Cargo.toml`
4. Implement `#![no_std]` binary with `_start` entry point

## License

MIT OR Apache-2.0

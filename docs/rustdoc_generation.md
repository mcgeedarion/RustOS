# RustOS API Documentation Generation

## Generating rustdoc API Documentation

This document describes how to generate comprehensive API documentation for the RustOS kernel.

### Prerequisites

```bash
# Ensure you have the Rust toolchain installed
rustup install nightly
rustup component add rust-docs
```

### Generate Documentation

#### Standard Documentation

```bash
# Generate documentation for all crates
cargo doc --all-features --no-deps

# Open in browser (Linux)
xdg-open target/doc/rustos_kernel/index.html

# Open in browser (macOS)
open target/doc/rustos_kernel/index.html
```

#### With Private Items

```bash
# Include private items for kernel developers
cargo doc --all-features --document-private-items --no-deps
```

#### Nightly Features

```bash
# Use nightly for unstable features
cargo +nightly doc --all-features --no-deps \
    -Z unstable-options \
    --enable-index-page
```

### Documentation Structure

The generated documentation includes:

1. **Kernel Core** (`src/`)
   - Architecture-specific implementations
   - Memory management
   - Process scheduler
   - System call interface

2. **Filesystem Drivers** (`src/fs/`)
   - VFS layer
   - ext4/ext2 driver
   - FAT32 driver
   - tmpfs/ramfs
   - JBD2 journaling

3. **SMP Subsystem** (`src/smp/`)
   - CPU hotplug state machine
   - Per-CPU data structures
   - IPI handling

4. **Device Drivers** (`src/drivers/`)
   - VirtIO drivers
   - PCIe subsystem
   - ACPI integration

5. **IPC Mechanisms** (`src/ipc/`)
   - Message queues
   - Pipes
   - Shared memory

### Custom Documentation Themes

Create a custom theme in `docs/rustdoc-theme.css`:

```css
/* Custom RustOS documentation theme */
.sidebar-logo {
    content: url("logo.png");
}

.docblock {
    background-color: #f8f9fa;
}
```

Apply with:
```bash
cargo doc --theme-files docs/rustdoc-theme.css
```

### Documentation Tests

Run doctests to ensure examples compile:

```bash
cargo test --doc --all-features
```

### Continuous Documentation

Documentation is automatically generated and deployed on every merge to main via GitHub Actions.

See `.github/workflows/docs.yml` for configuration.

### Viewing Documentation Locally

After generating docs, serve them locally:

```bash
# Using Python
cd target/doc
python3 -m http.server 8000

# Then open http://localhost:8000/rustos_kernel/
```

### Documentation Guidelines

When adding new modules or functions:

1. **Module-level docs**: Explain purpose and architecture
2. **Function docs**: Document parameters, return values, errors
3. **Examples**: Include usage examples in code blocks
4. **Safety**: Mark unsafe functions and explain invariants
5. **Panics**: Document panic conditions

Example:
```rust
/// Bring a CPU online through the hotplug state machine.
///
/// Transitions the CPU through states:
/// `Offline → Starting → Online`
///
/// # Arguments
/// * `cpu_id` - Logical CPU ID to bring online
///
/// # Returns
/// * `Ok(())` - CPU successfully brought online
/// * `Err(HotplugError)` - Operation failed
///
/// # Safety
/// Caller must ensure the CPU is physically present
///
/// # Example
/// ```rust,no_run
/// use rustos_kernel::smp::hotplug::cpu_up;
///
/// unsafe {
///     cpu_up(2).expect("Failed to bring up CPU 2");
/// }
/// ```
pub unsafe fn cpu_up(cpu_id: u32) -> HotplugResult<()> {
    // ...
}
```

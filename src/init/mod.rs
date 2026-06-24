//! Early-boot and init-process support.
//!
//! | Module           | Role                                                         |
//! |------------------|--------------------------------------------------------------|
//! | `boot_info`      | Parsed multiboot2 / UEFI / SBI boot information.            |
//! | `crt`            | C runtime stub: sets up the initial stack frame and calls   |
//! |                  | `main` for user-space ELF binaries.                          |
//! | `initramfs`      | CPIO initramfs parser and in-memory VFS mount.               |
//! |                  | `load()` / `file()` are called during boot before the       |
//! |                  | scheduler starts.  Always compiled in; the `userspace_boot`  |
//! |                  | feature path depends on it to locate /init.                  |
//! | `loader`         | High-level ELF loader: maps PT_LOAD segments into a fresh   |
//! |                  | address space and sets up the auxiliary vector.              |
//! | `schemes`        | Registers all built-in kernel schemes into `SCHEME_TABLE`.  |
//! |                  | Called from `kernel_main` after NIC + DHCP init.            |
//! | `service_manager`| Manages long-running kernel services and their restarts.    |

pub mod boot_info;

// `initramfs` must compile under every feature flag because
// `userspace_boot::enter()` calls `initramfs::load()` and `file("/init")`
// directly.  The heavier subsystems (crt, loader, schemes, service_manager)
// remain gated to keep the boot_minimal and userspace_boot images slim.
pub mod initramfs;

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod crt;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod loader;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod schemes;
#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
pub mod service_manager;

//! Kernel → userspace handoff for the `userspace_boot` milestone.
//!
//! Boot sequence
//! -------------
//! 1. Arch-specific early console init.
//! 2. [`crate::fs::initramfs::mount_initramfs`] — parse the CPIO archive and populate the initial
//!    VFS ramfs tree so that PID 1 and its children can call `open(2)` on any file that was packed
//!    into the archive.
//! 3. Locate `/init` in the raw archive bytes via [`crate::init::initramfs::load`] +
//!    [`InitramfsHandle::file`].  This is a zero-copy `&[u8]` slice — no heap allocation.
//! 4. [`crate::proc::exec::spawn_user_process_from_bytes`] — parse the ELF, build the page table,
//!    allocate a PCB (PID 1, PPID 0), and enqueue it on the run-queue.
//! 5. The boot CPU enters its idle loop; the scheduler dispatches PID 1 on the next tick.
//!
//! ## Initramfs format: CPIO newc (SVR4, uncompressed)
//!
//! **Why CPIO newc?**
//!
//! * It is the Linux kernel's own initramfs wire format; every boot loader and QEMU `-initrd` flag
//!   already understands it.
//! * The 110-byte ASCII-hex header is trivially parseable in `no_std` without heap allocation — see
//!   [`crate::init::initramfs`].
//! * Sequential layout matches the streaming [`CpioIter`] perfectly; no random-access index is
//!   required.
//!
//! **Why uncompressed?**
//!
//! The kernel does not yet include a decompressor.  `cargo xtask build-init`
//! and `scripts/pack-initramfs.sh` both emit uncompressed archives, and QEMU
//! passes the raw bytes to `set_initramfs_range()` via `LoadFile2` (x86-64
//! UEFI) or the FDT `/chosen` node (AArch64).
//!
//! ## Missing `/init`
//!
//! If `/init` is absent the kernel calls [`panic!`] with a descriptive,
//! actionable message rather than spinning silently.  This satisfies the Phase
//! 2 acceptance criterion: *"Missing /init produces a descriptive panic, not a
//! silent hang."*

use crate::init::boot_info::BootInfo;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// Early bump allocator
// ---------------------------------------------------------------------------
// A simple lock-free bump allocator that satisfies GlobalAlloc for the
// userspace_boot feature path.  dealloc is a no-op; the entire heap lives
// for the duration of the boot sequence.  Once the full PMM / slab allocator
// is compiled in this crate section can be removed.

struct EarlyBumpAllocator;

const HEAP_SIZE: usize = 1024 * 1024; // 1 MiB

#[repr(align(16))]
struct AlignedHeap([u8; HEAP_SIZE]);

static mut HEAP: AlignedHeap = AlignedHeap([0; HEAP_SIZE]);
static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for EarlyBumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(1);
        let size = layout.size();
        let mut current = HEAP_NEXT.load(Ordering::Relaxed);
        loop {
            let aligned = (current + align - 1) & !(align - 1);
            let Some(next) = aligned.checked_add(size) else {
                return core::ptr::null_mut();
            };
            if next > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            match HEAP_NEXT.compare_exchange(current, next, Ordering::SeqCst, Ordering::Relaxed) {
                Ok(_) => {
                    let base = core::ptr::addr_of_mut!(HEAP.0) as *mut u8;
                    return base.add(aligned);
                },
                Err(observed) => current = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static GLOBAL_ALLOCATOR: EarlyBumpAllocator = EarlyBumpAllocator;

// ---------------------------------------------------------------------------
// Architecture abstraction
// ---------------------------------------------------------------------------

/// Architecture-specific hooks required by the userspace-boot milestone.
///
/// Implement this trait in each arch's boot stub and pass the marker type
/// to [`enter`].
pub trait UserspaceBootArch {
    /// Human-readable architecture name, e.g. `"x86_64"`.
    const NAME: &'static str;

    /// Bring up the earliest serial console so `serial_println!` works.
    fn early_console_init();

    /// Execute one architecture-specific idle instruction (`hlt`, `wfi`, …).
    fn idle_once();
}

// ---------------------------------------------------------------------------
// Boot entry point
// ---------------------------------------------------------------------------

/// Kernel entry point for the `userspace_boot` milestone.
///
/// Called from the arch-specific boot stub after the MMU, interrupt
/// controllers, and the physical memory manager are operational.
/// **Never returns** — either the scheduler takes over after PID 1 is
/// enqueued, or the kernel panics with a diagnostic message.
pub fn enter<A: UserspaceBootArch>(boot_info: &'static BootInfo) -> ! {
    A::early_console_init();

    // ── Milestone 1: kernel entry ────────────────────────────────────────────
    crate::boot_mark!("BOOT_ENTRY");

    crate::serial_println!("rustos: userspace_boot milestone — arch={}", A::NAME);
    let _ = boot_info; // reserved for future use (memory map, ACPI tables, …)

    // ── Milestone 2: MMU/paging enabled ─────────────────────────────────────
    // The arch stub enables the MMU before calling enter(); we record it
    // here as the first common-path observation after virtual memory is live.
    crate::boot_mark!("BOOT_MMU_ON");

    // ── Step 1: parse the CPIO archive and populate the VFS ramfs tree ──────
    //
    // After this call every file packed into the initramfs is reachable
    // through the VFS so that PID 1 and its children can open them with
    // normal syscalls.
    crate::fs::initramfs::mount_initramfs();

    // ── Milestone 3: initramfs fully parsed and mapped ───────────────────────
    crate::boot_mark!("BOOT_INITRAMFS_LOADED");

    // ── Step 2: locate /init in the raw archive bytes ───────────────────────
    //
    // We call `load()` + `file()` directly on the raw CPIO bytes rather than
    // going through the VFS.  This avoids a second allocation round-trip and
    // means we don't depend on the VFS being fully operational at this point.
    let ram = crate::init::initramfs::load();

    let init_elf: &[u8] = match ram.file("/init") {
        Some(bytes) => {
            crate::serial_println!("initramfs: found /init ({} bytes)", bytes.len());
            bytes
        },
        None => {
            // Acceptance criterion: descriptive panic, not a silent hang.
            panic!(
                "kernel: /init not found in the initramfs.\n\
                 \n\
                 To fix:\n\
                   1. Rebuild the initramfs:  cargo xtask build-init\n\
                   2. Verify that userspace/init was compiled successfully.\n\
                   3. Ensure the CPIO archive contains `./init` at the root\n\
                      with execute permission (mode 0755).\n\
                 \n\
                 Boot cannot continue without PID 1."
            )
        },
    };

    // ── Step 3: exec /init as PID 1 ─────────────────────────────────────────
    //
    // `spawn_user_process_from_bytes` parses the ELF, maps PT_LOAD segments
    // into a new page table, builds the auxv / argv stack frame, allocates a
    // PCB with pid = alloc_pid() (first call → PID 1, ppid = 0), and enqueues
    // the thread on the global run-queue.
    crate::serial_println!("userspace: spawning /init as PID 1");

    let spawned =
        crate::proc::exec::spawn_user_process_from_bytes("/init", init_elf, &["/init"], &[]);

    if !spawned {
        panic!(
            "kernel: failed to exec /init — ELF load error.\n\
             \n\
             Checklist:\n\
               • /init must be a statically-linked ELF64 (no dynamic loader).\n\
               • Target architecture must match the running kernel.\n\
               • /init must have execute permission (mode 0755) in the\n\
                 CPIO archive.\n\
               • Rebuild with:  cargo xtask build-init --arch <arch>"
        );
    }

    crate::serial_println!("init: PID 1 created from /init");

    // ── Milestone 4: /init first instruction (execve called) ─────────────────
    // spawn_user_process_from_bytes has enqueued the PID 1 thread; the next
    // scheduler tick will deliver execution to /init's entry point.  We record
    // the marker here — immediately before parking the boot CPU — as the
    // closest observable proxy for "execve returned / first userspace tick".
    crate::boot_mark!("BOOT_INIT_EXEC");

    crate::serial_println!("userspace: PID 1 accepted; waiting for userspace sentinel");

    // ── Step 4: become the idle thread ──────────────────────────────────────
    //
    // The boot CPU parks itself here.  The scheduler will dispatch PID 1 on
    // its first tick.  We never return from this loop.
    loop {
        A::idle_once();
    }
}

//! Kernel panic handler and global allocator error handler.
//!
//! # Output format
//!
//! Every kernel panic emits a structured, CI-parseable block on the serial
//! console.  The format is intentionally line-oriented so CI tooling can
//! classify failures with simple `grep` rules:
//!
//! ```text
//! KERNEL PANIC: <message>
//! PANIC_LOC: <file>:<line>:<col>
//! PANIC_ARCH: <x86_64|aarch64|riscv64>
//! PANIC_TASK: <name> PID=<pid>          (omitted if no task context)
//! PANIC_FAULT_ADDR: <0xHEX>             (set by fault injectors; 0x0 otherwise)
//! --- REGISTER DUMP ---
//! ... (arch-specific, from debug::oops)
//! --- BACKTRACE ---
//! ... (frame-pointer walk)
//! --- END PANIC ---
//! ```
//!
//! OOM panics use `OOM:` prefix so CI can distinguish them from logic faults.

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
use crate::arch::{
    api::{Cpu, Interrupts, Serial},
    Arch,
};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

// ---------------------------------------------------------------------------
// Global faulting-address slot
// ---------------------------------------------------------------------------
// Trap handlers (page-fault, GPF, etc.) write the faulting address here
// *before* calling into the panic path so the formatter can emit it even
// when no TrapFrame is available (bare Rust panics).

static FAULT_ADDR: AtomicUsize = AtomicUsize::new(0);

/// Record the faulting address for the next panic emission.
/// Call this from architecture trap handlers prior to `panic!()`.
#[inline]
pub fn set_fault_addr(addr: usize) {
    FAULT_ADDR.store(addr, Ordering::Relaxed);
}

/// Return and clear the last recorded faulting address.
#[inline]
fn take_fault_addr() -> usize {
    FAULT_ADDR.swap(0, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Halt helpers
// ---------------------------------------------------------------------------

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
#[cold]
#[inline(never)]
fn halt_loop() -> ! {
    loop {
        Arch::halt();
    }
}

#[cfg(any(feature = "boot_minimal", feature = "userspace_boot"))]
#[cold]
#[inline(never)]
fn halt_loop() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

// ---------------------------------------------------------------------------
// Panic handler
// ---------------------------------------------------------------------------

#[panic_handler]
#[cold]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // 1. Stop all other cores and disable local interrupts.
    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    {
        Arch::disable();
        crate::smp::ipi::halt_all_except_self();
    }

    // 2. Structured header — CI grep anchors.
    let msg = info.message();
    serial_write(b"\r\n");
    serial_write(b"KERNEL PANIC: ");
    let _ = write!(ArchSerialWriter, "{}", msg);
    serial_write(b"\r\n");

    if let Some(loc) = info.location() {
        serial_write(b"PANIC_LOC: ");
        serial_write(loc.file().as_bytes());
        serial_write(b":");
        serial_u64(loc.line() as u64);
        serial_write(b":");
        serial_u64(loc.column() as u64);
        serial_write(b"\r\n");
    } else {
        serial_write(b"PANIC_LOC: unknown\r\n");
    }

    serial_write(b"PANIC_ARCH: ");
    serial_write(crate::arch::api::name().as_bytes());
    serial_write(b"\r\n");

    // 3. Current task name + PID (best-effort — may not be available early).
    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    emit_task_context();

    // 4. Faulting address (written by trap handlers via set_fault_addr).
    let fault = take_fault_addr();
    serial_write(b"PANIC_FAULT_ADDR: 0x");
    serial_hex(fault as u64);
    serial_write(b"\r\n");

    // 5. Full register dump + backtrace via oops module (debug feature).
    #[cfg(feature = "debug")]
    crate::debug::oops::oops(None);

    // Without debug feature emit a minimal backtrace header so CI at least
    // sees the section boundary.
    #[cfg(not(feature = "debug"))]
    {
        serial_write(b"--- REGISTER DUMP ---\r\n");
        serial_write(b"  (build without --features debug for full dump)\r\n");
        serial_write(b"--- BACKTRACE ---\r\n");
        serial_write(b"  (build without --features debug for backtrace)\r\n");
    }

    serial_write(b"--- END PANIC ---\r\n");
    halt_loop()
}

// ---------------------------------------------------------------------------
// OOM handler
// ---------------------------------------------------------------------------

#[alloc_error_handler]
#[cold]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    Arch::disable();

    serial_write(b"\r\nOOM: kernel allocation failure\r\n");
    serial_write(b"OOM_SIZE: ");
    serial_u64(layout.size() as u64);
    serial_write(b"\r\nOOM_ALIGN: ");
    serial_u64(layout.align() as u64);
    serial_write(b"\r\nOOM_ARCH: ");
    serial_write(crate::arch::api::name().as_bytes());
    serial_write(b"\r\n--- END OOM ---\r\n");
    halt_loop()
}

// ---------------------------------------------------------------------------
// Task context emission (requires proc subsystem)
// ---------------------------------------------------------------------------

#[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
fn emit_task_context() {
    // Try to read the current task name and PID from the scheduler.
    // `current_task_info()` returns None if called before the scheduler
    // is initialised (early boot panics).
    if let Some((name, pid)) = crate::proc::current_task_info() {
        serial_write(b"PANIC_TASK: ");
        serial_write(name.as_bytes());
        serial_write(b" PID=");
        serial_u64(pid as u64);
        serial_write(b"\r\n");
    } else {
        serial_write(b"PANIC_TASK: (pre-scheduler)\r\n");
    }
}

// ---------------------------------------------------------------------------
// Serial I/O helpers (alloc-free, works before heap is up)
// ---------------------------------------------------------------------------

struct ArchSerialWriter;
impl Write for ArchSerialWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        serial_write(s.as_bytes());
        Ok(())
    }
}

#[inline]
fn serial_write(bytes: &[u8]) {
    for &b in bytes {
        #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
        Arch::serial_putc(b);
        #[cfg(any(feature = "boot_minimal", feature = "userspace_boot"))]
        unsafe {
            crate::arch::console::early_putchar(b);
        }
    }
}

/// Write a decimal u64 without allocation.
fn serial_u64(mut n: u64) {
    if n == 0 {
        serial_write(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    serial_write(&buf[i..]);
}

/// Write a hex u64 without allocation (no leading `0x` — caller adds it).
fn serial_hex(mut n: u64) {
    const HEX: &[u8] = b"0123456789abcdef";
    if n == 0 {
        serial_write(b"0");
        return;
    }
    let mut buf = [0u8; 16];
    let mut i = 16usize;
    while n > 0 {
        i -= 1;
        buf[i] = HEX[(n & 0xf) as usize];
        n >>= 4;
    }
    serial_write(&buf[i..]);
}

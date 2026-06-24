//! Structured panic/oops formatter — register dump, frame-pointer backtrace,
//! optional trace ring drain, and faulting-address annotation.
//!
//! # Serial output contract
//!
//! Every section begins and ends with a line that CI tooling can anchor on:
//!
//! ```text
//! --- REGISTER DUMP ---
//! OOPS_REG_<NAME>: 0x<HEX>   (one register per line)
//! --- BACKTRACE ---
//! OOPS_FRAME #<N>: 0x<HEX>   (one frame per line; N is zero-based)
//! --- TRACE RING ---           (only when --features trace)
//! --- END OOPS ---
//! ```
//!
//! The `OOPS_REG_` and `OOPS_FRAME` prefixes are stable; CI can grep
//! for them independently of architecture or build variant.
//!
//! # Register dump
//!
//! Pass an [`AnyTrapFrame`] variant from the trap handler for a full register
//! dump.  For bare Rust panics (no TrapFrame), pass `None` — the current
//! frame-pointer register is snapshotted via inline asm for the backtrace.
//!
//! # Backtrace
//!
//! Requires frame pointers.  In `Cargo.toml` add to every `[profile.*]`:
//! ```toml
//! force-frame-pointers = true
//! ```

use crate::arch::hal::serial_write;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::trap::TrapFrame as AnyTrapFrame;
#[cfg(target_arch = "riscv64")]
use crate::arch::riscv64::trap::TrapFrame as AnyTrapFrame;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::trap::TrapFrame as AnyTrapFrame;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Emit a full oops report: register dump (if a trap frame is available),
/// frame-pointer backtrace, and (if built with `--features trace`) the
/// contents of the trace ring-buffer.
///
/// Called from `kernel/panic.rs` immediately after the structured panic header
/// has been written to the console.  Also callable directly from fault
/// handlers that have a `TrapFrame` in hand.
pub fn oops(frame: Option<&AnyTrapFrame>) {
    serial_write("\r\n--- REGISTER DUMP ---\r\n");
    match frame {
        Some(f) => dump_registers(f),
        None => serial_write("OOPS_REG_NOTE: no trap frame (bare Rust panic)\r\n"),
    }

    serial_write("\r\n--- BACKTRACE ---\r\n");
    backtrace(frame);

    #[cfg(feature = "trace")]
    {
        serial_write("\r\n--- TRACE RING ---\r\n");
        crate::debug::trace::drain_to_serial();
    }

    serial_write("\r\n--- END OOPS ---\r\n");
}

// ---------------------------------------------------------------------------
// Register dump  (one `OOPS_REG_<NAME>: 0xHEX` line per register)
// ---------------------------------------------------------------------------

/// Emit one register line in the canonical `OOPS_REG_<NAME>: 0x<HEX>` format.
macro_rules! reg {
    ($name:literal, $val:expr) => {
        serial_write(&alloc::format!(
            concat!("OOPS_REG_", $name, ": {:#018x}\r\n"),
            $val
        ));
    };
}

#[cfg(target_arch = "x86_64")]
fn dump_registers(f: &AnyTrapFrame) {
    reg!("RAX", f.rax);
    reg!("RBX", f.rbx);
    reg!("RCX", f.rcx);
    reg!("RDX", f.rdx);
    reg!("RSI", f.rsi);
    reg!("RDI", f.rdi);
    reg!("RBP", f.rbp);
    reg!("RSP", f.rsp);
    reg!("R8",  f.r8);
    reg!("R9",  f.r9);
    reg!("R10", f.r10);
    reg!("R11", f.r11);
    reg!("R12", f.r12);
    reg!("R13", f.r13);
    reg!("R14", f.r14);
    reg!("R15", f.r15);
    reg!("RIP", f.rip);
    reg!("RFLAGS", f.rflags);
    reg!("CS", f.cs);
    reg!("SS", f.ss);
    reg!("ERR", f.error_code);
    reg!("VEC", f.vector);
    // Faulting address lives in CR2 for page faults; expose it explicitly.
    #[cfg(not(any(feature = "boot_minimal", feature = "userspace_boot")))]
    {
        let fault = crate::kernel::panic::take_fault_addr_for_oops();
        if fault != 0 {
            serial_write(&alloc::format!("OOPS_REG_FAULT_ADDR: {:#018x}\r\n", fault));
        }
    }
}

#[cfg(target_arch = "riscv64")]
fn dump_registers(f: &AnyTrapFrame) {
    reg!("RA",  f.ra);
    reg!("SP",  f.sp);
    reg!("GP",  f.gp);
    reg!("TP",  f.tp);
    reg!("T0",  f.t0);
    reg!("T1",  f.t1);
    reg!("T2",  f.t2);
    reg!("S0",  f.s0);
    reg!("S1",  f.s1);
    reg!("A0",  f.a0);
    reg!("A1",  f.a1);
    reg!("A2",  f.a2);
    reg!("A3",  f.a3);
    reg!("A4",  f.a4);
    reg!("A5",  f.a5);
    reg!("A6",  f.a6);
    reg!("A7",  f.a7);
    reg!("S2",  f.s2);
    reg!("S3",  f.s3);
    reg!("S4",  f.s4);
    reg!("S5",  f.s5);
    reg!("S6",  f.s6);
    reg!("S7",  f.s7);
    reg!("S8",  f.s8);
    reg!("S9",  f.s9);
    reg!("S10", f.s10);
    reg!("S11", f.s11);
    reg!("T3",  f.t3);
    reg!("T4",  f.t4);
    reg!("T5",  f.t5);
    reg!("T6",  f.t6);
    reg!("SEPC",   f.sepc);
    reg!("SCAUSE", f.scause);
    // stval = faulting address on page faults
    reg!("STVAL",  f.stval);
    serial_write(&alloc::format!("OOPS_REG_FAULT_ADDR: {:#018x}\r\n", f.stval));
}

#[cfg(target_arch = "aarch64")]
fn dump_registers(f: &AnyTrapFrame) {
    for (i, r) in f.x.iter().enumerate() {
        serial_write(&alloc::format!("OOPS_REG_X{i:02}: {r:#018x}\r\n"));
    }
    reg!("SP",     f.sp);
    reg!("PC",     f.pc);
    reg!("PSTATE", f.pstate);
    reg!("ESR",    f.esr);
    // FAR = faulting address register on data/instruction aborts
    reg!("FAR",    f.far);
    serial_write(&alloc::format!("OOPS_REG_FAULT_ADDR: {:#018x}\r\n", f.far));
}

// ---------------------------------------------------------------------------
// Frame-pointer backtrace  (one `OOPS_FRAME #N: 0xHEX` line per frame)
// ---------------------------------------------------------------------------

fn backtrace(frame: Option<&AnyTrapFrame>) {
    // Prefer the frame pointer from the trap frame when one is available;
    // otherwise snapshot the current register via inline asm.
    let start_fp: usize = match frame {
        #[cfg(target_arch = "x86_64")]
        Some(f) => f.rbp as usize,
        #[cfg(target_arch = "riscv64")]
        Some(f) => f.s0 as usize,
        #[cfg(target_arch = "aarch64")]
        Some(f) => f.x[29] as usize, // x29 == fp
        _ => {
            let mut fp: usize = 0;
            #[cfg(target_arch = "x86_64")]
            unsafe { core::arch::asm!("mov {}, rbp", out(reg) fp) }
            #[cfg(target_arch = "riscv64")]
            unsafe { core::arch::asm!("mv {}, s0", out(reg) fp) }
            #[cfg(target_arch = "aarch64")]
            unsafe { core::arch::asm!("mov {}, x29", out(reg) fp) }
            fp
        }
    };

    let mut fp = start_fp;
    let mut depth = 0usize;
    loop {
        if fp == 0 || depth > 64 {
            break;
        }
        // Descending stack layout used by all three architectures:
        //   fp[0]  → saved frame pointer of caller
        //   fp[-1] → return address (one pointer-width below fp)
        let saved_fp = unsafe { *(fp as *const usize) };
        let ret_addr = unsafe { *((fp - core::mem::size_of::<usize>()) as *const usize) };
        serial_write(&alloc::format!("OOPS_FRAME #{depth}: {ret_addr:#018x}\r\n"));
        fp = saved_fp;
        depth += 1;
    }
    if depth == 0 {
        serial_write("OOPS_FRAME_NOTE: no frame pointers — rebuild with force-frame-pointers = true\r\n");
    }
}

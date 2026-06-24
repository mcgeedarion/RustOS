//! Fault-to-signal bridge.
//!
//! Called by architecture-specific fault handlers to translate hardware
//! exceptions into POSIX signals.  Each function sends the appropriate
//! signal to the current process and returns — the signal will be delivered
//! the next time the process is about to return to user space (i.e., at the
//! end of the fault-return path in the arch exception handler).
//!
//! ## Integration (x86_64)
//!
//! In the x86_64 page-fault handler (`src/arch/x86_64/exceptions.rs`):
//!
//! ```rust,ignore
//! // #PF — page fault
//! pub fn handle_page_fault(cr2: usize, error_code: u64) {
//!     let pid = crate::proc::scheduler::current_pid();
//!     if !crate::mm::try_handle_cow(pid, cr2) {
//!         // Not a CoW fault; attempt CoW expansion, then signal.
//!         if !crate::mm::mmap::handle_anonymous_demand(pid, cr2) {
//!             crate::proc::fault_signal::on_page_fault(cr2, error_code);
//!         }
//!     }
//! }
//!
//! // #UD — undefined opcode
//! pub fn handle_invalid_opcode(rip: usize) {
//!     crate::proc::fault_signal::on_invalid_opcode(rip);
//! }
//!
//! // #DE — divide error
//! pub fn handle_divide_error(rip: usize) {
//!     crate::proc::fault_signal::on_divide_error(rip);
//! }
//!
//! // #AC — alignment check
//! pub fn handle_alignment_check(addr: usize) {
//!     crate::proc::fault_signal::on_alignment_check(addr);
//! }
//! ```
//!
//! ## Integration (RISC-V / AArch64)
//!
//! Map the relevant mcause / ESR_EL1 codes to the corresponding
//! `on_*` function below.

use crate::proc::scheduler;

/// Page-fault exception.  Sends SIGSEGV (access error) or SIGBUS (alignment).
///
/// `fault_addr` — the faulting virtual address (CR2 on x86_64, FAR_EL1 on
///               AArch64, `stval` on RISC-V).
/// `error_code` — architecture-specific fault flags.  Bit 0 on x86_64:
///   0 = page not present (SEGV_MAPERR), 1 = protection violation (SEGV_ACCERR).
pub fn on_page_fault(fault_addr: usize, error_code: u64) {
    let pid = scheduler::current_pid();
    // Bit 0: protection violation vs. mapping missing.
    if error_code & 1 != 0 {
        crate::proc::signal::send_sigsegv_accerr(pid, fault_addr);
    } else {
        crate::proc::signal::send_sigsegv(pid, fault_addr);
    }
}

/// Undefined / illegal instruction (#UD on x86_64, SIGILL trap on other arches).
///
/// `fault_addr` — the instruction pointer at the faulting instruction.
pub fn on_invalid_opcode(fault_addr: usize) {
    let pid = scheduler::current_pid();
    crate::proc::signal::send_sigill(pid, fault_addr);
}

/// Divide-by-zero / integer overflow (#DE on x86_64).
///
/// `fault_addr` — instruction pointer at the faulting instruction.
pub fn on_divide_error(fault_addr: usize) {
    let pid = scheduler::current_pid();
    crate::proc::signal::send_sigfpe(pid, fault_addr);
}

/// Floating-point exception (e.g. x87 #MF / SSE #XF on x86_64).
///
/// `fault_addr` — instruction pointer at the faulting instruction.
pub fn on_fp_exception(fault_addr: usize) {
    let pid = scheduler::current_pid();
    crate::proc::signal::send_sigfpe(pid, fault_addr);
}

/// Bus error / alignment check (#AC on x86_64, unaligned memory access).
///
/// `fault_addr` — the misaligned address.
pub fn on_alignment_check(fault_addr: usize) {
    let pid = scheduler::current_pid();
    crate::proc::signal::send_sigbus(pid, fault_addr);
}

/// General protection fault (#GP on x86_64).  Sends SIGSEGV to the process.
///
/// `fault_addr` — typically 0 for #GP (no specific address); the RIP is
///               stored in `info.addr` by convention.
pub fn on_general_protection(fault_addr: usize) {
    let pid = scheduler::current_pid();
    crate::proc::signal::send_sigsegv(pid, fault_addr);
}

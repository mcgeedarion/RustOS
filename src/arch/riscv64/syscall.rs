//! RISC-V 64 syscall handling.
//!
//! RISC-V uses the `ecall` instruction for system calls, similar to how
//! other architectures use special instructions (svc on ARM, syscall on x86).
//!
//! ## ABI (Linux RISC-V)
//!
//!   a7  — syscall number
//!   a0  — arg0 / return value
//!   a1  — arg1
//!   a2  — arg2
//!   a3  — arg3
//!   a4  — arg4
//!   a5  — arg5
//!
//! The syscall number is passed in a7, and arguments are in a0-a5.
//! Return value is in a0.

use crate::arch::api::TrapFrame;
use crate::proc::scheduler;

/// Handle a syscall from user mode.
/// Called from the trap handler when an ecall is received from U-mode.
pub fn handle(frame: &mut TrapFrame) {
    let nr = frame.regs[17]; // a7 = syscall number
    let args = [
        frame.regs[10], // a0
        frame.regs[11], // a1
        frame.regs[12], // a2
        frame.regs[13], // a3
        frame.regs[14], // a4
        frame.regs[15], // a5
    ];

    // Mark CPU as inside syscall for scheduler time-accounting.
    let cpu = scheduler::current_cpu_id();
    unsafe {
        crate::smp::percpu::PERCPU_BLOCKS[cpu].in_syscall += 1;
    }

    // Dispatch the syscall
    let ret = crate::syscall::dispatch(
        nr as usize,
        args[0] as usize,
        args[1] as usize,
        args[2] as usize,
        args[3] as usize,
        args[4] as usize,
        args[5] as usize,
    );

    // Set return value in a0
    frame.regs[10] = ret as u64;

    unsafe {
        crate::smp::percpu::PERCPU_BLOCKS[cpu].in_syscall -= 1;
    }

    // Deliver any pending signals before returning to user mode.
    check_and_deliver_signals(frame);
}

/// Check and deliver pending signals to the current thread.
fn check_and_deliver_signals(frame: &mut TrapFrame) {
    let pid = scheduler::current_pid();
    if pid == 0 {
        return;
    }

    // Check for pending signals
    if let Some((signo, handler)) = crate::proc::signal::get_pending_signal(pid) {
        // Set up signal frame and redirect execution to signal handler
        setup_signal_frame(frame, signo, handler);
    }
}

/// Set up a signal frame on the user stack.
fn setup_signal_frame(frame: &mut TrapFrame, signo: u32, handler: usize) {
    // Allocate space on user stack for signal frame
    let user_sp = frame.user_sp;
    
    // Create signal frame (simplified - would need proper struct)
    // Signal frame contains: saved context, signal info, trampoline code
    
    // Update frame to execute signal handler
    frame.pc = handler as u64;
    frame.regs[10] = signo as u64; // First arg = signal number
    
    // Would also set up rt_sigreturn trampoline for returning from handler
}

/// Syscall return - restore user registers and return via sret.
/// This is called after syscall processing to return to user mode.
pub unsafe fn syscall_return(frame: *const TrapFrame) -> ! {
    let f = &*frame;
    
    core::arch::asm!(
        "mv a0, {ret}",       // Return value
        "mv sp, {usp}",       // User stack pointer  
        "csrw sepc, {pc}",    // Exception return PC
        "csrw sstatus, {flags}", // Restore status (SPP=0 for user mode)
        "sret",               // Return to user mode
        ret   = in(reg) f.regs[10],
        usp   = in(reg) f.user_sp,
        pc    = in(reg) f.pc,
        flags = in(reg) f.flags,
        options(noreturn)
    );
}

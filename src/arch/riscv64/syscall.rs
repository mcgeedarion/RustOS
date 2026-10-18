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
//!
//! User signal-handler delivery is intentionally disabled on RISC-V until the
//! ABI-compatible signal frame, executable restorer trampoline, and rt_sigreturn
//! register restoration are implemented as one tested path.

use crate::arch::api::TrapFrame;
use crate::proc::scheduler;

/// Handle a syscall from user mode.
/// Called from the trap handler when an ecall is received from U-mode.
pub fn handle(frame: &mut TrapFrame) {
    let nr = frame.regs[17]; // a7 = syscall number

    // Save every integer register before dispatch so fork can construct a child
    // return frame with a0=0 and resume after the ecall instruction.
    let pid = scheduler::current_pid() as usize;
    if pid != 0 {
        scheduler::with_proc_mut(pid, |pcb, _| {
            for index in 0..32 {
                pcb.user_context.regs[index] = frame.regs[index] as usize;
            }
            pcb.user_context.pc = frame.pc as usize + 4;
            pcb.user_context.sp = frame.user_sp as usize;
            pcb.user_context.flags = frame.flags as usize;
            pcb.pc = pcb.user_context.pc;
            pcb.sp = pcb.user_context.sp;
        });
    }
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

    // RISC-V sepc points at the ecall instruction. Save that address for
    // syscall metadata, then advance it so sret resumes after the ecall.
    let saved_pc = frame.pc;
    frame.pc = frame.pc.wrapping_add(4);

    // Translate the Linux RISC-V ABI at the explicit architecture boundary.
    let ret = crate::syscall::dispatch_arch(
        crate::syscall::SyscallAbi::RiscV64,
        nr as usize,
        [
            args[0] as usize, args[1] as usize, args[2] as usize,
            args[3] as usize, args[4] as usize, args[5] as usize,
        ],
        saved_pc,
    );

    // Set return value in a0
    frame.regs[10] = ret as u64;

    unsafe {
        crate::smp::percpu::PERCPU_BLOCKS[cpu].in_syscall -= 1;
    }

    // Signal delivery is disabled on this architecture until the complete
    // user signal frame/restorer/rt_sigreturn ABI is implemented. Do not
    // redirect PC to a handler without saving a restorable context.
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

/// Saved process CPU context.
///
/// This minimal representation is enough for process-table construction and
/// can be extended by architecture-specific switch code as the full context
/// switch path is wired back up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Context {
    pub regs: [usize; 16],
    pub sp: usize,
    pub pc: usize,
}

impl Context {
    pub const fn zero() -> Self {
        Self {
            regs: [0; 16],
            sp: 0,
            pc: 0,
        }
    }
}


/// Architecture-neutral saved userspace register state.
///
/// regs uses architectural integer register numbering: x86-64 uses its
/// documented slot map in arch/x86_64/syscall.rs, AArch64 uses x0-x30, and
/// RISC-V uses x0-x31. Entry stubs capture this frame before dispatch so a
/// fork child can resume after the syscall with its return register set to zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserContext {
    pub regs: [usize; 32],
    pub pc: usize,
    pub sp: usize,
    pub flags: usize,
}

impl UserContext {
    pub const fn zero() -> Self {
        Self {
            regs: [0; 32],
            pc: 0,
            sp: 0,
            flags: 0x202,
        }
    }
}

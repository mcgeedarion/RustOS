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

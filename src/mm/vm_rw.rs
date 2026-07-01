//! Compatibility stubs for process_vm_readv/process_vm_writev.
//!
//! Cross-process vectored memory access requires stable remote-mm locking and
//! credential checks.  Return `ENOSYS` for now instead of leaving router symbols
//! unresolved.

const ENOSYS: isize = -38;

pub fn sys_process_vm_readv(
    _pid: usize,
    _local_iov: usize,
    _liovcnt: usize,
    _remote_iov: usize,
    _riovcnt: usize,
    _flags: u32,
) -> isize {
    ENOSYS
}

pub fn sys_process_vm_writev(
    _pid: usize,
    _local_iov: usize,
    _liovcnt: usize,
    _remote_iov: usize,
    _riovcnt: usize,
    _flags: u32,
) -> isize {
    ENOSYS
}

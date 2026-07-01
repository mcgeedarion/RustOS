//! Compatibility stub for the bpf(2) syscall.

const ENOSYS: isize = -38;

pub fn sys_bpf(_cmd: i32, _attr: usize, _size: u32) -> isize {
    ENOSYS
}

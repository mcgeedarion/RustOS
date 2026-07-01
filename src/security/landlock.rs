//! Compatibility stubs for Landlock syscalls.

const ENOSYS: isize = -38;

pub fn sys_landlock_create_ruleset(_attr: usize, _size: usize, _flags: u32) -> isize {
    ENOSYS
}

pub fn sys_landlock_add_rule(
    _ruleset_fd: usize,
    _rule_type: u32,
    _rule_attr: usize,
    _flags: u32,
) -> isize {
    ENOSYS
}

pub fn sys_landlock_restrict_self(_ruleset_fd: usize, _flags: u32) -> isize {
    ENOSYS
}

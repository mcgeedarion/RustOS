//! Clone entry points backed by the independent-address-space fork lifecycle.
//!
//! Shared VM, thread groups and vfork suspension require lifecycle support that
//! fork does not provide. Reject those modes before creating a child instead of
//! silently turning them into a different kind of process.

const SIGCHLD: u64 = 17;

fn validate_fork_clone(flags: u64, stack: u64) -> Result<(), isize> {
    if flags & 0xff > 64 {
        return Err(-22);
    }
    if flags != SIGCHLD || stack != 0 {
        return Err(-95);
    }
    Ok(())
}

pub fn sys_clone(
    flags: usize,
    stack: usize,
    _parent_tid: usize,
    _child_tid: usize,
    _tls: usize,
) -> isize {
    if let Err(e) = validate_fork_clone(flags as u64, stack as u64) {
        return e;
    }
    crate::proc::fork_syscall::sys_fork()
}

pub fn sys_vfork() -> isize {
    // Parent suspension and shared-address-space teardown are not implemented.
    -95
}

pub fn sys_clone3(args: usize, size: usize) -> isize {
    // The original clone_args version has 64 bytes. Later versions append
    // set_tid, set_tid_size and cgroup. Unknown nonzero extensions are E2BIG.
    if size < 64 {
        return -22;
    }
    if size > 4096 {
        return -7;
    }
    let mut bytes = [0u8; 88];
    let known = size.min(bytes.len());
    if crate::uaccess::copy_from_user(&mut bytes[..known], args).is_err() {
        return -14;
    }
    for i in known..size {
        let Some(va) = args.checked_add(i) else { return -14 };
        let mut byte = [0u8; 1];
        if crate::uaccess::copy_from_user(&mut byte, va).is_err() {
            return -14;
        }
        if byte[0] != 0 {
            return -7;
        }
    }
    let word = |i: usize| u64::from_ne_bytes(bytes[i..i + 8].try_into().unwrap());
    let flags = word(0);
    let exit_signal = word(32);
    if flags & 0xff != 0 || exit_signal > 64 {
        return -22;
    }
    if word(40) == 0 && word(48) != 0 || word(40) != 0 && word(48) == 0 {
        return -22;
    }
    if word(64) != 0 || word(72) != 0 || word(80) != 0 {
        return -95;
    }
    if let Err(e) = validate_fork_clone(flags | exit_signal, word(40)) {
        return e;
    }
    crate::proc::fork_syscall::sys_fork()
}

#[cfg(test)]
mod tests {
    use super::validate_fork_clone;

    #[test]
    fn fork_flags_are_accepted() {
        assert_eq!(validate_fork_clone(17, 0), Ok(()));
    }

    #[test]
    fn sharing_and_alternate_stacks_are_not_silently_ignored() {
        for flags in [0, 17 | 0x100, 17 | 0x4000, 17 | 0x10000, 17 | 0x100000] {
            assert_eq!(validate_fork_clone(flags, 0), Err(-95));
        }
        assert_eq!(validate_fork_clone(17, 4096), Err(-95));
        assert_eq!(validate_fork_clone(255, 0), Err(-22));
    }
}

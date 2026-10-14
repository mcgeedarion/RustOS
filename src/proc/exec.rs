extern crate alloc;

use alloc::{string::String, vec::Vec};

const MAX_CSTR_LEN: usize = 4096;
const MAX_EXEC_STRINGS: usize = 4096;
const MAX_EXEC_BYTES: usize = 128 * 1024;
const EFAULT: isize = -14;
const E2BIG: isize = -7;
const ENOENT: isize = -2;
const ENOSYS: isize = -38;

/// Copy a NUL-terminated userspace string into a kernel-owned `String`.
///
/// Returns `None` when the pointer is null, unmapped, not NUL-terminated
/// within the bounded scan, or not valid UTF-8.
pub fn read_cstr_safe(addr: usize) -> Option<String> {
    if addr == 0 {
        return None;
    }

    let mut bytes = Vec::new();
    for offset in 0..MAX_CSTR_LEN {
        let mut byte = 0u8;
        if crate::kernel::uaccess::copy_from_user(&mut byte as *mut u8, addr + offset, 1).is_err() {
            return None;
        }
        if byte == 0 {
            return String::from_utf8(bytes).ok();
        }
        bytes.push(byte);
    }

    None
}

/// Minimal execve syscall surface used until full image replacement is wired.
///
/// Bad path pointers must report `EFAULT`; otherwise return `ENOSYS` instead
/// of panicking or pretending exec succeeded.
/// Copy a NULL-terminated userspace argv/envp vector with strict bounds.
///
/// A zero vector address is accepted as an empty vector for Linux compatibility.
/// The aggregate bytes for argv and envp share one budget to prevent unbounded
/// allocation from a userspace pointer array.
fn read_cstr_vector_safe(addr: usize, remaining_bytes: &mut usize) -> Result<Vec<String>, isize> {
    if addr == 0 {
        return Ok(Vec::new());
    }

    let pointer_size = core::mem::size_of::<usize>();
    let mut strings = Vec::new();
    for index in 0..MAX_EXEC_STRINGS {
        let offset = index.checked_mul(pointer_size).ok_or(EFAULT)?;
        let pointer_addr = addr.checked_add(offset).ok_or(EFAULT)?;
        let mut raw = [0u8; core::mem::size_of::<usize>()];
        if crate::kernel::uaccess::copy_from_user(raw.as_mut_ptr(), pointer_addr, pointer_size).is_err() {
            return Err(EFAULT);
        }

        let string_addr = usize::from_ne_bytes(raw);
        if string_addr == 0 {
            return Ok(strings);
        }

        let value = read_cstr_safe(string_addr).ok_or(EFAULT)?;
        let cost = value.len().checked_add(1).ok_or(E2BIG)?;
        if cost > *remaining_bytes {
            return Err(E2BIG);
        }
        *remaining_bytes -= cost;
        strings.push(value);
    }

    Err(E2BIG)
}

/// Validate all user pointers before attempting executable replacement.
///
/// The image replacement backend remains unavailable: otherwise-valid input
/// returns ENOSYS until ELF mappings, an ABI stack, architecture return state,
/// and atomic replacement are integrated.
pub fn sys_execve(path_va: usize, argv_va: usize, envp_va: usize) -> isize {
    let path = match read_cstr_safe(path_va) {
        Some(path) if !path.is_empty() => path,
        Some(_) => return ENOENT,
        None => return EFAULT,
    };

    let mut remaining_bytes = MAX_EXEC_BYTES;
    let argv = match read_cstr_vector_safe(argv_va, &mut remaining_bytes) {
        Ok(value) => value,
        Err(errno) => return errno,
    };
    let envp = match read_cstr_vector_safe(envp_va, &mut remaining_bytes) {
        Ok(value) => value,
        Err(errno) => return errno,
    };

    // Keep the validated copies alive until an executable backend consumes
    // them. Execution remains blocked until the kernel can commit replacement
    // without destroying the current process image.
    let _validated_exec_input = (path, argv, envp);
    ENOSYS
}

/// Compatibility entry point for process creation.
/// No executable userspace backend is integrated yet, so this always fails closed.
/// Do not treat the bytes-based helper below as a complete loader either.
pub fn spawn_user_process(_path: &str, _argv: &[&str], _envp: &[&str]) -> bool {
    false
}

/// Staged entry point for spawning a user process from ELF bytes.
///
/// This path is deliberately not promoted as working process execution:
/// PT_LOAD installation, a valid initial userspace stack, and architecture
/// return-state setup are not integrated. It returns false until those pieces
/// are implemented and an executable guest test proves the result.
///
/// Parameters:
/// - pid: Pre-allocated PID from scheduler::alloc_pid()
/// - path: Path to the executable (for debugging/naming)
/// - elf: Raw ELF binary bytes
/// - argv: Argument strings
/// - envp: Environment strings
/// - entry: ELF entry point address
///
/// Returns true on success, false on any error.
pub fn spawn_user_process_from_bytes_full(
    pid: usize,
    path: &str,
    elf: &[u8],
    argv: &[&str],
    envp: &[&str],
    entry: u64,
) -> bool {
    use crate::proc::process::Process;
    use crate::proc::scheduler;

    crate::serial_println!(
        "exec: creating process {} (PID {}) entry={:#x}",
        path,
        pid,
        entry
    );

    // Step 1: Create process with proper credentials
    let process = Process::new_user_process(
        pid,
        0, // PPID = 0 for init
        path.to_string(),
    );

    // Step 2: Map ELF segments into process address space
    if !map_elf_segments_for_process(&process, elf, entry) {
        crate::serial_println!("exec: failed to map ELF segments for PID {}", pid);
        return false;
    }

    // Step 3: Set up initial stack frame with argv/envp/auxv
    if !setup_user_stack(&process, entry, argv, envp) {
        crate::serial_println!("exec: failed to set up stack for PID {}", pid);
        return false;
    }

    // Step 4: Enqueue on scheduler run-queue
    scheduler::enqueue_process(process);

    crate::serial_println!("exec: successfully enqueued PID {} for scheduling", pid);
    true
}

/// Install the ELF PT_LOAD segments in this process's isolated address space.
///
/// The mapper is not implemented yet. This function deliberately returns false
/// instead of treating ELF header validation as page-table installation.
fn map_elf_segments_for_process(_process: &Process, _elf: &[u8], entry: u64) -> bool {
    // Do not enqueue a process while its address space is not actually mapped.
    // Claiming success here would make a source-level stub look like a working
    // ELF loader and could send the scheduler to an unmapped user entry.
    crate::serial_println!(
        "exec: refusing PID creation; PT_LOAD mapping is not implemented (entry={:#x})",
        entry
    );
    false
}

/// Set up an ABI-compliant initial user stack with argv, envp, and auxv.
///
/// The initial stack builder is not implemented; scheduling a process before
/// this exists would enter user mode with an invalid SP.
fn setup_user_stack(_process: &Process, entry: u64, argv: &[&str], envp: &[&str]) -> bool {
    // The process cannot safely execute until its initial user stack contains
    // argc/argv/envp/auxv and the saved user SP points at that stack.
    crate::serial_println!(
        "exec: refusing PID creation; initial user stack is not implemented (entry={:#x}, argc={}, envc={})",
        entry,
        argv.len(),
        envp.len()
    );
    false
}

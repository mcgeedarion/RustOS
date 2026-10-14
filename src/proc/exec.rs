extern crate alloc;

use alloc::{string::String, vec::Vec};

const MAX_CSTR_LEN: usize = 4096;

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
pub fn sys_execve(path_va: usize, _argv_va: usize, _envp_va: usize) -> isize {
    match read_cstr_safe(path_va) {
        Some(_) => -38, // ENOSYS
        None => -14,    // EFAULT
    }
}

/// Full ELF userspace spawning - stub for backward compatibility.
/// Use spawn_user_process_from_bytes_full for complete process creation.
pub fn spawn_user_process(_path: &str, _argv: &[&str], _envp: &[&str]) -> bool {
    false
}

/// Spawn a user process from ELF bytes with full scheduler integration.
///
/// This function:
/// 1. Allocates a new PID and Process structure
/// 2. Maps PT_LOAD segments into the process address space
/// 3. Sets up the initial stack with argv/envp/auxv
/// 4. Enqueues the process on the scheduler run-queue
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

/// Map ELF PT_LOAD segments into process address space.
///
/// This is a simplified implementation that logs the mapping request.
/// Full implementation would integrate with mm::mmap and page table setup.
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

/// Set up initial user stack with argv, envp, and auxv.
///
/// The stack layout follows the System V ABI convention:
/// - argc (argument count)
/// - argv[] (argument pointers, NULL-terminated)
/// - envp[] (environment pointers, NULL-terminated)
/// - auxv[] (auxiliary vector)
/// - argument strings
/// - environment strings
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

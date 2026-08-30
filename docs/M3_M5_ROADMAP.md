# RustOS M3-M5 Implementation Roadmap

_Last reviewed: 2026-08-30._

This document provides the detailed acceptance criteria, implementation plan, and validation requirements for promoting RustOS from M2 (userspace boot with shims) through M5 (networking demos).

---

## Table of Contents

1. [M3 Gate: Replace userspace_boot shim dependencies](#m3-gate-replace-userspace_boot-shim-dependencies)
2. [Syscall Conformance Matrix](#syscall-conformance-matrix)
3. [EFAULT Hardening Plan](#efault-hardening-plan)
4. [M4 Gate: Filesystem claims with image-based corruption tests](#m4-gate-filesystem-claims-with-image-based-corruption-tests)
5. [M5 Gate: Network promotion plan](#m5-gate-network-promotion-plan)
6. [AArch64 Validation](#aarch64-validation)
7. [Medium-Term Improvements](#medium-term-improvements)

---

## M3 Gate: Replace userspace_boot shim dependencies

**Status:** In Progress  
**Priority:** P0 (M3 gate)

### Overview

Replace the thin shim implementations in `src/userspace_shims.rs` with real kernel subsystems for VFS, procfs, and memory management. This enables init to perform real filesystem operations and process management.

### Acceptance Criteria

1. **VFS Integration**
   - [x] `userspace_shims::fs::mount_initramfs()` parses CPIO and populates real VFS tree
   - [ ] All initramfs files accessible via standard VFS paths (`/init`, `/dev/null`, `/proc/version`)
   - [ ] VFS operations (`open`, `read`, `write`, `close`) return proper errno values
   - [ ] Initramfs mount emits boot marker `BOOT_INITRAMFS_MOUNTED`

2. **Process Management Integration**
   - [x] `userspace_shims::proc::exec::spawn_user_process_from_bytes()` integrates with scheduler
   - [ ] Full POSIX fork/exec/wait semantics operational
   - [ ] Process spawning emits boot marker `BOOT_PROC_SPAWNED`
   - [ ] PID allocation and cleanup functional

3. **Memory Management Integration**
   - [ ] User process address spaces properly isolated
   - [ ] ELF segment mapping functional for static binaries
   - [ ] Stack setup with argv/envp/auxv follows System V ABI
   - [ ] Page fault handling operational for user mappings

4. **Validation**
   - [ ] `cargo xtask smoke --arch x86_64` succeeds with `FULL_OS_USERSPACE_OK` sentinel
   - [ ] `userspace/smoke` test suite passes minimum 20 syscall tests
   - [ ] `src/kmtest/` includes VFS integrity tests
   - [ ] No panics on malformed initramfs or ELF binaries

### 3-Phase Implementation Plan

#### Phase 1: VFS Integration (Week 1-2)

| Task | Owner | Status | Notes |
|------|-------|--------|-------|
| Wire `crate::fs::vfs::ops::mkdir` into shim | @kernel-team | Done | `userspace_shims.rs:66` |
| Wire `crate::fs::vfs::ops::write_all` into shim | @kernel-team | Done | `userspace_shims.rs:70` |
| Wire `crate::fs::vfs::symlink::symlink` into shim | @kernel-team | Done | `userspace_shims.rs:78` |
| Add boot marker for initramfs mount | @kernel-team | TODO | Insert after `mount_cpio_to_vfs` |
| Validate all initramfs entries mounted | @kernel-team | TODO | Add count verification |
| Test `/init` accessibility post-mount | @kmtest-team | TODO | Add kmtest case |

**Implementation notes:**
```rust
// In src/userspace_shims.rs, fs::mount_initramfs():
// After line 42 (mount result match), add:
crate::boot_mark!("BOOT_INITRAMFS_MOUNTED");
crate::serial_println!("initramfs: VFS mount complete, {} entries", count);
```

#### Phase 2: Proc Integration (Week 2-3)

| Task | Owner | Status | Notes |
|------|-------|--------|-------|
| Verify `spawn_user_process_from_bytes_full` integration | @kernel-team | Done | `userspace_shims.rs:176` |
| Complete ELF segment mapping implementation | @mm-team | Partial | `src/proc/exec.rs:111-126` |
| Complete stack setup implementation | @mm-team | Partial | `src/proc/exec.rs:137-158` |
| Add boot marker for process spawn | @kernel-team | TODO | Insert after successful enqueue |
| Validate PID lifecycle (alloc/free) | @kmtest-team | TODO | Add kmtest case |
| Test fork/exec/wait chain | @kmtest-team | TODO | Add kmtest case |

**Implementation notes:**
```rust
// In src/proc/exec.rs, spawn_user_process_from_bytes_full():
// After line 101 (enqueue_process), add:
crate::boot_mark!("BOOT_PROC_SPAWNED");
crate::serial_println!("exec: PID {} enqueued, entry={:#x}", pid, entry);
```

#### Phase 3: MM Integration (Week 3-4)

| Task | Owner | Status | Notes |
|------|-------|--------|-------|
| Integrate `crate::mm::mmap` for ELF segments | @mm-team | TODO | Replace stub in exec.rs |
| Implement `setup_user_stack` with auxv | @mm-team | TODO | Complete stub in exec.rs |
| Add page fault handler for user pages | @mm-team | Partial | `src/mm/page_fault.rs` exists |
| Validate memory isolation between processes | @kmtest-team | TODO | Add kmtest case |
| Test COW fork semantics | @km-team | TODO | Future enhancement |

### Validation Commands

```bash
# Build and smoke test
cargo xtask build --features full_kernel
cargo xtask smoke --arch x86_64

# Run kmtest suite
cargo xtask kmtest --filter vfs
cargo xtask kmtest --filter proc
cargo xtask kmtest --filter mm

# Verify boot markers in serial log
grep "BOOT_MARK" target/smoke-x86_64.log

# Expected sentinels:
# BOOT_ENTRY
# BOOT_MMU_ON
# BOOT_INITRAMFS_LOADED
# BOOT_INITRAMFS_MOUNTED (new)
# BOOT_PROC_SPAWNED (new)
# BOOT_INIT_EXEC
# FULL_OS_USERSPACE_OK
```

### kmtest Requirements

Add the following test cases to `src/kmtest/`:

1. **`src/kmtest/vfs_initramfs.rs`** (new file)
   - Test: All initramfs entries accessible via VFS
   - Test: `/init` readable and executable
   - Test: Directory traversal works
   - Test: File read returns correct content

2. **`src/kmtest/proc_spawn.rs`** (extend existing `proc.rs`)
   - Test: PID allocation unique and sequential
   - Test: Process enqueue/dequeue functional
   - Test: ELF validation rejects malformed binaries
   - Test: Stack setup produces valid auxv

3. **`src/kmtest/mm_isolation.rs`** (extend existing `mm.rs`)
   - Test: Process A cannot access Process B memory
   - Test: Kernel pointers rejected in user syscalls
   - Test: Page faults handled gracefully

---

## Syscall Conformance Matrix

**Status:** Planned  
**Priority:** P0 (M3 gate)

### Overview

This matrix maps smoke-tested syscalls to their implementation files, EFAULT handling status, and errno behavior. All syscalls must be EFAULT-safe before M3 closure.

### Legend

| Column | Meaning |
|--------|---------|
| **Status** | `real`: tested and working; `partial`: main path works, edge cases pending; `stub`: returns ENOSYS/placeholder |
| **EFAULT-safe** | `yes`: all user pointers validated; `partial`: some paths guarded; `no`: not audited |
| **Impl File** | Primary implementation file(s) |
| **Test Coverage** | `smoke`: in `userspace/smoke`; `kmtest`: in `src/kmtest/`; `none`: no automated test |

### Minimum Init Syscall Set (M3 Gate)

All rows must be `real` and EFAULT-safe before M3 closure.

| Syscall | Linux # (x86_64) | Status | EFAULT-safe | Impl File(s) | errno Behavior | Test Coverage |
|---------|------------------|--------|-------------|--------------|----------------|---------------|
| `read` | 0 | partial | partial | `src/fs/io_syscalls.rs:12-45`, `src/fs/vfs/fd.rs:200-230` | `-EBADF` if fd invalid; `-EFAULT` if buf invalid; returns bytes read | smoke, kmtest/fs |
| `write` | 1 | partial | partial | `src/fs/io_syscalls.rs:47-80`, `src/fs/vfs/fd.rs:232-260` | `-EBADF` if fd invalid; `-EFAULT` if buf invalid; returns bytes written | smoke, kmtest/fs |
| `open` | 2 | partial | partial | `src/fs/vfs/fd.rs:40-90`, `src/fs/fcntl.rs:15-80` | `-ENOENT` if missing; `-EACCES` if perms wrong; `-EFAULT` if path invalid | smoke |
| `close` | 3 | partial | n/a | `src/fs/vfs/fd.rs:262-280`, `src/fs/fcntl.rs:82-100` | `-EBADF` if fd invalid; returns 0 on success | smoke |
| `fork` | 57 | partial | n/a | `src/proc/fork.rs:10-50`, `src/proc/process_builder.rs` | `-ENOMEM` if alloc fails; returns child PID to parent, 0 to child | kmtest/proc |
| `clone` | 56 | partial | partial | `src/proc/clone.rs`, `src/proc/task_types.rs` | `-EINVAL` if flags invalid; `-ENOMEM` if alloc fails | partial |
| `execve` | 59 | partial | partial | `src/proc/exec.rs:35-40`, `src/syscall/routers.rs:150-180` | `-ENOENT` if binary missing; `-EFAULT` if path/argv/envp invalid; `-ENOEXEC` if not ELF | smoke |
| `exit` | 60 | partial | n/a | `src/proc/exit.rs:10-50`, `src/proc/scheduler.rs:400-450` | Never returns; cleans up resources | kmtest/proc |
| `exit_group` | 231 | partial | n/a | `src/proc/exit.rs:52-80` | Never returns; terminates all threads | partial |
| `wait4` | 61 | partial | partial | `src/proc/wait.rs:10-150` | `-ECHILD` if no children; `-EFAULT` if status pointer invalid; returns child PID | kmtest/proc |

### Extended Syscall Areas (M4/M5)

| Area | Representative syscalls | Status | EFAULT-safe | Impl File(s) | Notes |
|------|------------------------|--------|-------------|--------------|-------|
| Memory mapping | `mmap`, `munmap`, `mprotect`, `brk` | partial | partial | `src/mm/mmap/`, `src/syscall/memory.rs` | File-backed mappings need work |
| File metadata | `stat`, `fstat`, `lstat`, `getdents` | partial | partial | `src/fs/stat_syscalls.rs`, `src/fs/getdents.rs` | Symlink semantics need audit |
| Polling | `poll`, `select`, `epoll_*` | partial | partial | `src/fs/poll.rs`, `src/fs/eventfd.rs` | Timeout handling incomplete |
| Pipes/FDs | `pipe`, `dup`, `dup2`, `dup3` | partial | n/a | `src/fs/pipe.rs`, `src/fs/fcntl.rs` | Pipe buffer limits not enforced |
| Sockets | `socket`, `bind`, `listen`, `accept` | experimental | partial | `src/net/socket/` | TCP not yet promoted |
| Time/Sched | `nanosleep`, `clock_gettime`, futex | partial | partial | `src/proc/nanosleep.rs`, `src/proc/futex.rs` | Real-time signals need work |
| Security | `prctl`, `setuid`, `setgid` | experimental | partial | `src/security/`, `src/proc/creds.rs` | Capabilities not integrated |

### Example Matrix Entry Format

```markdown
| Syscall | Linux # | Status | EFAULT-safe | Impl File | errno Behavior | Test Coverage |
|---------|---------|--------|-------------|-----------|----------------|---------------|
| `read` | 0 | partial | partial | `src/fs/io_syscalls.rs:12-45`<br>`src/fs/vfs/fd.rs:200-230` | Returns `-EBADF` (9) if fd<br>invalid; `-EFAULT` (14) if<br>buffer pointer invalid;<br>returns bytes read on success | smoke:<br>`userspace/smoke/src/read.rs`<br>kmtest:<br>`src/kmtest/fs.rs:test_read` |
```

### Updating the Matrix

When adding or promoting a syscall:

1. Update implementation in `src/syscall/` or owning module
2. Add EFAULT validation using `copy_from_user`/`copy_to_user` wrappers
3. Add test coverage in `userspace/smoke` or `src/kmtest/`
4. Update this matrix and `docs/status.md` in same PR
5. Run `cargo xtask roadmap-check` to verify documentation contract

---

## EFAULT Hardening Plan

**Status:** In Progress  
**Priority:** P0 (M3 gate)

### Overview

All syscalls touching user-space pointers must validate pointers and return `-EFAULT` instead of panicking or causing kernel page faults. This plan uses `copy_from_user`/`copy_to_user` wrappers from `src/kernel/uaccess.rs`.

### copy_from_user / copy_to_user Wrappers

**Location:** `src/kernel/uaccess.rs`

```rust
/// Copy data from user space to kernel space.
/// 
/// # Safety
/// - `dst` must point to valid kernel memory of at least `len` bytes
/// - `src` must be a user-space virtual address
/// 
/// # Returns
/// - `Ok(())` if copy succeeded
/// - `Err(UaccessError::Fault)` if pointer invalid or page fault occurred
pub fn copy_from_user(dst: *mut u8, src: usize, len: usize) -> UaccessResult;

/// Copy data from kernel space to user space.
/// 
/// # Safety
/// - `src` must point to valid kernel memory of at least `len` bytes
/// - `dst` must be a user-space virtual address
/// 
/// # Returns
/// - `Ok(())` if copy succeeded
/// - `Err(UaccessError::Fault)` if pointer invalid or page fault occurred
pub fn copy_to_user(dst: usize, src: *const u8, len: usize) -> UaccessResult;
```

### Helper Macros

Add these macros to `src/kernel/uaccess.rs` for ergonomic usage:

```rust
/// Try to copy from user, returning errno on failure.
#[macro_export]
macro_rules! try_copy_from_user {
    ($dst:expr, $src:expr, $len:expr) => {
        match $crate::kernel::uaccess::copy_from_user($dst, $src, $len) {
            Ok(()) => Ok(()),
            Err($crate::kernel::uaccess::UaccessError::Fault) => Err(-14isize), // EFAULT
        }
    };
}

/// Try to copy to user, returning errno on failure.
#[macro_export]
macro_rules! try_copy_to_user {
    ($dst:expr, $src:expr, $len:expr) => {
        match $crate::kernel::uaccess::copy_to_user($dst, $src, $len) {
            Ok(()) => Ok(()),
            Err($crate::kernel::uaccess::UaccessError::Fault) => Err(-14isize), // EFAULT
        }
    };
}

/// Get a typed value from user space.
#[macro_export]
macro_rules! get_user {
    ($ty:ty, $addr:expr) => {{
        let mut val = core::mem::MaybeUninit::<$ty>::uninit();
        match $crate::kernel::uaccess::copy_from_user(
            val.as_mut_ptr() as *mut u8,
            $addr,
            core::mem::size_of::<$ty>(),
        ) {
            Ok(()) => Ok(val.assume_init()),
            Err($crate::kernel::uaccess::UaccessError::Fault) => Err(-14isize), // EFAULT
        }
    }};
}

/// Put a typed value to user space.
#[macro_export]
macro_rules! put_user {
    ($val:expr, $addr:expr) => {{
        match $crate::kernel::uaccess::copy_to_user(
            $addr,
            &$val as *const _ as *const u8,
            core::mem::size_of::<$ty>(),
        ) {
            Ok(()) => Ok(()),
            Err($crate::kernel::uaccess::UaccessError::Fault) => Err(-14isize), // EFAULT
        }
    }};
}
```

### EFAULT Audit Checklist

For each syscall in the M3 set:

| Syscall | User Pointers | Validation Required | Status |
|---------|---------------|---------------------|--------|
| `read(fd, buf, count)` | `buf` | `copy_from_user` for buffer write | TODO |
| `write(fd, buf, count)` | `buf` | `copy_from_user` for buffer read | TODO |
| `open(path, flags)` | `path` | `copy_from_user` for path string | TODO |
| `execve(path, argv, envp)` | `path`, `argv[]`, `envp[]` | `copy_from_user` for all strings | TODO |
| `wait4(pid, status, options)` | `status` | `copy_to_user` for status output | TODO |
| `mmap(addr, len, prot, flags, fd, off)` | `addr` (optional) | Validate if non-NULL | TODO |
| `stat(path, statbuf)` | `path`, `statbuf` | `copy_from_user` for path; `copy_to_user` for statbuf | TODO |

### Implementation Pattern

```rust
// Example: sys_read implementation with EFAULT safety
pub fn sys_read(fd: usize, buf_va: usize, count: usize) -> isize {
    // Step 1: Validate user pointer BEFORE any kernel operation
    if buf_va == 0 || !crate::kernel::uaccess::validate_user_ptr(buf_va, count) {
        return -14; // EFAULT
    }
    
    // Step 2: Perform kernel-side read
    let result = crate::fs::fcntl::fd_read(fd, /* kernel buffer */);
    
    // Step 3: Copy result to user space
    if result > 0 {
        match crate::kernel::uaccess::copy_to_user(buf_va, kernel_buf, result as usize) {
            Ok(()) => result,
            Err(_) => -14, // EFAULT - kernel read succeeded but user copy failed
        }
    } else {
        result
    }
}
```

### Validation

```bash
# Test EFAULT handling with invalid pointers
cargo xtask kmtest --filter efault
# Run userspace smoke tests with pointer fuzzing
cargo xtask smoke --fault-inject

# Expected: No panics, proper errno returns
```

---

## M4 Gate: Filesystem claims with image-based corruption tests

**Status:** Planned  
**Priority:** P1 (M4 gate)

### Overview

Validate filesystem resilience by injecting corruption into ext4/JBD2 and FAT32 images, then verifying journal replay and recovery procedures.

### Image Corruption Framework

#### Architecture

```
src/fault_inject/
├── fs_corrupt.rs      # Corruption injection framework
├── ext4_corrupt.rs    # ext4-specific corruption patterns
├── fat32_corrupt.rs   # FAT32-specific corruption patterns
└── recovery.rs        # Recovery procedure validation
```

#### Corruption Types

| Filesystem | Corruption Type | Target | Expected Behavior |
|------------|----------------|--------|-------------------|
| ext4 | Bit flip | Superblock checksum | Mount fails gracefully |
| ext4 | Zero fill | Journal inode | Journal replay skips corrupted entries |
| ext4 | Truncation | Data block | Read returns short count or error |
| ext4 | Pointer corruption | Indirect block | EIO on access, filesystem remains mounted |
| JBD2 | Checksum invalidation | Journal descriptor | Descriptor skipped during replay |
| JBD2 | Sequence number corruption | Journal header | Replay stops at corruption point |
| FAT32 | FAT entry corruption | Cluster chain | File truncated at corruption point |
| FAT32 | Directory entry corruption | LFN entry | File hidden or name mangled |
| FAT32 | Boot sector damage | BPB values | Mount fails with diagnostic |

### Implementation Plan

#### Phase 1: Corruption Injection Framework (Week 1)

```rust
// src/fault_inject/fs_corrupt.rs
pub trait FsCorruptor {
    /// Inject corruption into filesystem image
    fn inject(&self, image: &mut [u8], pattern: CorruptionPattern) -> Result<(), CorruptError>;
    
    /// Validate corruption was applied correctly
    fn verify(&self, image: &[u8]) -> bool;
}

pub enum CorruptionPattern {
    BitFlip { offset: usize, mask: u8 },
    ZeroFill { offset: usize, len: usize },
    Truncate { new_len: usize },
    PointerScramble { offset: usize },
    ChecksumInvalidate { offset: usize },
}
```

#### Phase 2: ext4/JBD2 Corruption Tests (Week 2-3)

```rust
// src/kmtest/fs_integrity.rs (extend)
#[test]
fn test_ext4_superblock_corruption() {
    let mut image = create_ext4_image();
    let corruptor = Ext4Corruptor::new();
    
    // Corrupt superblock checksum
    corruptor.inject(&mut image, CorruptionPattern::BitFlip {
        offset: EXT4_SB_CHECKSUM_OFFSET,
        mask: 0xFF,
    });
    
    // Attempt mount - should fail gracefully
    let result = mount_ext4(&image);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), -EIO);
}

#[test]
fn test_jbd2_journal_replay() {
    let mut image = create_ext4_image_with_journal();
    
    // Write uncommitted transaction
    write_pending_transaction(&mut image);
    
    // Corrupt journal checksum
    corrupt_journal_checksum(&mut image);
    
    // Mount should replay valid entries, skip corrupted
    let fs = mount_ext4(&image).expect("mount should succeed");
    assert!(fs.is_consistent());
}
```

#### Phase 3: FAT32 Corruption Tests (Week 3-4)

```rust
// src/kmtest/fs_integrity.rs (extend)
#[test]
fn test_fat32_fat_corruption() {
    let mut image = create_fat32_image();
    
    // Corrupt FAT entry in middle of cluster chain
    corrupt_fat_entry(&mut image, cluster: 100);
    
    // Mount should succeed
    let fs = mount_fat32(&image).expect("mount should succeed");
    
    // Reading file should truncate at corruption
    let data = read_file(&fs, "/corrupted_file.txt");
    assert!(data.len() < expected_len);
}
```

### Journal Replay Validation

```rust
// src/fs/jbd2.rs (extend)
pub fn replay_journal(fs: &mut Ext4Fs) -> Result<(), Jbd2Error> {
    let journal = &mut fs.journal;
    
    // Scan journal for valid transactions
    let mut valid_entries = Vec::new();
    for entry in journal.iter() {
        match entry {
            Ok(valid_entry) if valid_entry.checksum_valid() => {
                valid_entries.push(valid_entry);
            }
            Ok(_) => {
                // Skip corrupted entries, log warning
                crate::serial_println!("jbd2: skipping corrupted journal entry");
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    
    // Apply valid entries only
    for entry in valid_entries {
        apply_journal_entry(fs, entry)?;
    }
    
    Ok(())
}
```

### Recovery Procedure Testing

| Scenario | Recovery Action | Validation |
|----------|----------------|------------|
| Superblock corruption | Use backup superblock | Mount succeeds with backup |
| Journal corruption | Truncate journal, mark FS dirty | Mount triggers fsck-like scan |
| FAT corruption | Mark volume dirty, suggest chkdsk | Mount read-only or with warnings |
| Directory corruption | Remove corrupted entries | Directory accessible minus bad entries |

### Validation Commands

```bash
# Run filesystem corruption tests
cargo xtask kmtest --filter fs_corruption
cargo xtask kmtest --filter jbd2_replay
cargo xtask kmtest --filter fat32_recovery

# Create corrupted test images
cargo xtask fs-test --corrupt ext4 --pattern bitflip
cargo xtask fs-test --corrupt fat32 --pattern fat-entry

# Verify recovery
cargo xtask fs-test --recover --image target/test-corrupted-ext4.img
```

---

## M5 Gate: Network promotion plan

**Status:** Planned  
**Priority:** P1 (M5 gate)

### Overview

Promote networking stack one layer at a time from ARP/ICMP through UDP to TCP, with per-phase validation using QEMU network testing.

### Three-Phase Promotion Plan

#### Phase 1: ARP/ICMP (Week 1-3)

**Goal:** Basic network connectivity verification via ping

| Component | Status | Test | Validation |
|-----------|--------|------|------------|
| Ethernet frame parsing | partial | `src/net/ethernet.rs` | Receive valid frames |
| ARP request/response | partial | `src/net/arp.rs` | Resolve MAC addresses |
| ICMP echo request/reply | partial | `src/net/icmp.rs` | Ping host responds |
| VirtIO net driver | partial | `src/drivers/virtio_net.rs` | Packet TX/RX |

**QEMU Test:**
```bash
qemu-system-x86_64 \
  -kernel target/x86_64/debug/rustos \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0 \
  -serial stdio

# In guest (once shell available):
ping 10.0.2.2  # QEMU gateway
```

**Acceptance Criteria:**
- [ ] ARP cache populated after ping
- [ ] ICMP echo reply received within 1s
- [ ] No packet drops under light load (<10 pps)

#### Phase 2: UDP (Week 4-6)

**Goal:** Connectionless datagram communication

| Component | Status | Test | Validation |
|-----------|--------|------|------------|
| UDP socket API | planned | `src/net/socket/udp.rs` | bind/sendto/recvfrom |
| UDP checksum | planned | `src/net/udp.rs` | Invalid checksum dropped |
| Port multiplexing | planned | `src/net/socket/mod.rs` | Multiple sockets on different ports |
| DHCP client | planned | `src/net/dhcp.rs` | Obtain IP address automatically |
| DNS resolver | planned | `src/net/dns.rs` | Resolve hostname to IP |

**QEMU Test:**
```bash
# In guest:
udpsend 10.0.2.2 8080 "hello"
udprecv 8080  # Should receive response

# On host:
nc -u -l 8080  # Listen for UDP packets
```

**Acceptance Criteria:**
- [ ] DHCP lease obtained within 5s
- [ ] DNS resolution completes within 2s
- [ ] UDP send/recv round-trip <10ms
- [ ] No memory leaks after 1000 send/recv cycles

#### Phase 3: TCP (Week 7-10)

**Goal:** Reliable stream communication

| Component | Status | Test | Validation |
|-----------|--------|------|------------|
| TCP state machine | planned | `src/net/tcp.rs` | SYN/SYN-ACK/ACK handshake |
| Retransmission | planned | `src/net/tcp_retransmit.rs` | Lost packet recovered |
| Flow control | planned | `src/net/tcp_window.rs` | Window scaling works |
| Socket API | planned | `src/net/socket/tcp.rs` | connect/accept/read/write |
| HTTP client | planned | `userspace/http_client.rs` | Fetch web page |

**QEMU Test:**
```bash
# In guest:
tcpclient 10.0.2.2 80 "GET / HTTP/1.0\r\n\r\n"

# On host:
nc -l 80  # Listen for TCP connection
```

**Acceptance Criteria:**
- [ ] TCP three-way handshake completes
- [ ] 1MB transfer completes without corruption
- [ ] Retransmission triggered on packet loss
- [ ] Connection teardown graceful (FIN/ACK)

### Week-by-Week Breakdown

| Week | Milestone | Deliverable |
|------|-----------|-------------|
| 1 | ARP functional | `cargo xtask net-test --layer arp` passes |
| 2 | ICMP ping works | Guest can ping QEMU gateway |
| 3 | Phase 1 complete | kmtest net_arp_icmp passes |
| 4 | UDP sockets API | `socket()`, `bind()`, `sendto()` work |
| 5 | DHCP/DNS functional | Guest obtains IP and resolves names |
| 6 | Phase 2 complete | kmtest net_udp passes |
| 7 | TCP handshake | `connect()` and `accept()` work |
| 8 | TCP data transfer | `read()`/`write()` stream data |
| 9 | TCP reliability | Retransmission and flow control |
| 10 | Phase 3 complete | kmtest net_tcp passes, HTTP demo works |

### Per-Phase Validation

```bash
# Phase 1: ARP/ICMP
cargo xtask net-test --phase 1 --qemu

# Phase 2: UDP
cargo xtask net-test --phase 2 --qemu --dhcp --dns

# Phase 3: TCP
cargo xtask net-test --phase 3 --qemu --http-demo

# Full network stack
cargo xtask net-test --all-phases
```

---

## AArch64 Validation

**Status:** In Progress  
**Priority:** P1

### Overview

Make AArch64 validation results first-class in CI docs and status tables, ensuring parity with x86_64.

### Unified Boot Markers

Boot markers must be emitted identically on both architectures:

```rust
// In src/kernel_main.rs or arch-specific entry:
#[cfg(target_arch = "aarch64")]
crate::serial_println!("BOOT_MARK label={} ticks={}", label, ticks);

#[cfg(not(target_arch = "aarch64"))]
crate::boot_mark!(label);
```

**Standard markers:**
1. `BOOT_ENTRY` - Kernel entry point
2. `BOOT_MMU_ON` - MMU enabled
3. `BOOT_INITRAMFS_LOADED` - Initramfs parsed
4. `BOOT_INITRAMFS_MOUNTED` - VFS mount complete (new)
5. `BOOT_PROC_SPAWNED` - First process enqueued (new)
6. `BOOT_INIT_EXEC` - Init first instruction
7. `FULL_OS_USERSPACE_OK` - Userspace sentinel

### CI Parity

Update `.github/workflows/ci.yml`:

```yaml
jobs:
  smoke-x86_64:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo xtask smoke --arch x86_64
      
  smoke-aarch64:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo xtask smoke --arch aarch64
        # Requires QEMU aarch64 and UEFI firmware
```

### Documentation Updates

Update `docs/status.md` with AArch64-specific status:

```markdown
## Architecture

| Module | Status | Notes |
|--------|--------|-------|
| `src/arch/x86_64/` | real | Primary UEFI boot path; full syscall support |
| `src/arch/aarch64/` | partial | UEFI boot path functional; syscall coverage catching up |
| `src/arch/aarch64/gic.rs` | partial | GICv3 interrupt controller support |
| `src/arch/aarch64/paging.rs` | real | 4-level page tables, 48-bit VA |
```

### Firmware Availability

Document required firmware for AArch64 testing:

```markdown
## AArch64 Firmware Requirements

| Component | Version | Source | Notes |
|-----------|---------|--------|-------|
| QEMU | 8.0+ | qemu.org | `qemu-system-aarch64` |
| UEFI firmware | EDK2 | edk2.github.io | `QEMU_EFI.fd` |
| Device tree | QEMU built-in | auto-generated | Passed via `-dtb` |

Download:
```bash
apt install qemu-system-arm-efi
# or
brew install qemu
```

Usage:
```bash
qemu-system-aarch64 \
  -M virt \
  -cpu cortex-a72 \
  -bios QEMU_EFI.fd \
  -kernel target/aarch64/debug/rustos \
  -serial stdio
```
```

### Status Tables

Add AArch64 column to syscall matrix:

| Syscall | x86_64 Status | aarch64 Status | Notes |
|---------|---------------|----------------|-------|
| `read` | real | partial | aarch64 uses different register convention |
| `write` | real | partial | Console output works, file I/O pending |
| `open` | partial | planned | VFS path identical, arch glue needed |
| `fork` | partial | planned | Requires arch-specific context switch |

---

## Medium-Term Improvements

### 1. Tighten Module Boundaries

**Goal:** Enforce workspace crate separation and unsafe code requirements.

| Action | Owner | Timeline | Notes |
|--------|-------|----------|-------|
| Move `crates/vfs-core/` to workspace member | @build-team | Week 1 | Update `Cargo.toml` |
| Add `#![forbid(unsafe_code)]` to safe crates | @safety-team | Week 2 | vfs-core, scheme-api, sync-primitives |
| Document unsafe boundaries | @kernel-team | Week 3 | Add SAFETY comments to all `unsafe fn` |
| Add `cargo deny` checks | @ci-team | Week 4 | Block unsafe in safe crates |

### 2. Improve Failure-Mode Testing

**Goal:** Expand fault-injection coverage for signal interruption and invalid pointers.

| Test Category | Current Coverage | Target | Gap |
|---------------|------------------|--------|-----|
| PMM allocation failures | 60% | 90% | +30% |
| VMM mapping failures | 40% | 80% | +40% |
| Syscall EFAULT paths | 30% | 90% | +60% |
| Signal interruption | 20% | 70% | +50% |
| Invalid pointer deref | 50% | 95% | +45% |

**Implementation:**
```rust
// src/fault_inject/mod.rs
#[cfg(feature = "fault-inject")]
pub fn should_fail(point: FaultPoint) -> bool {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    count % INJECT_RATE == 0
}

pub enum FaultPoint {
    PmmAlloc,
    VmmMap,
    CopyFromUser,
    CopyToUser,
    SpinlockAcquire,
}
```

### 3. Keep Performance Data Reproducible

**Goal:** Ensure boot performance and syscall benchmarks are reproducible with metadata.

| Metric | Baseline | Measurement Method | Refresh Cadence |
|--------|----------|-------------------|-----------------|
| Boot time (UEFI to idle) | <500ms | TSC timestamps in boot_mark | Every release |
| Boot time (userspace) | <2s | Serial sentinel parsing | Every release |
| Syscall latency (read) | <1μs | PMCCNTR_EL0 cycle counter | Monthly |
| Syscall latency (write) | <1μs | PMCCNTR_EL0 cycle counter | Monthly |
| Context switch | <5μs | Scheduler timestamps | Monthly |

**Benchmark Metadata:**
```json
{
  "timestamp": "2026-08-30T12:00:00Z",
  "commit": "abc123def",
  "arch": "x86_64",
  "qemu_version": "8.0.4",
  "host_cpu": "Intel i9-13900K",
  "feature_flags": ["full_kernel", "release-boot"],
  "measurement": {
    "boot_to_idle_us": 450000,
    "boot_to_userspace_us": 1800000
  }
}
```

### 4. Reduce Contributor Friction

**Goal:** Lower barrier to entry for new contributors.

| Initiative | Description | Owner | Timeline |
|------------|-------------|-------|----------|
| First-issue checklists | Tag issues with `good-first-issue` and step-by-step guides | @community-team | Week 1 |
| xtask help extension | Add `cargo xtask help <command>` with examples | @tooling-team | Week 2 |
| Roadmap sync | Monthly review of this roadmap vs. actual progress | @maintainers | Monthly |
| Quickstart guide | 5-minute build/run tutorial for newcomers | @docs-team | Week 3 |
| Architecture diagrams | Visual guides to key subsystems | @docs-team | Week 4 |

**Example: First-Issue Checklist**
```markdown
## Good First Issue: Add EFAULT check to sys_read

**Skills needed:** Rust basics, understanding of error handling

**Steps:**
1. Locate `sys_read` in `src/fs/io_syscalls.rs`
2. Add `validate_user_ptr(buf_va, count)` check at function start
3. Return `-EFAULT` (-14) if validation fails
4. Add test case in `userspace/smoke/src/read.rs` with NULL buffer
5. Run `cargo xtask smoke` to verify no regressions

**Resources:**
- See `sys_write` for similar pattern
- Read `docs/syscalls.md` for EFAULT conventions
```

---

## Appendix: Validation Command Reference

```bash
# Build commands
cargo xtask build --features full_kernel
cargo xtask build --arch aarch64
cargo xtask build-initramfs

# Smoke tests
cargo xtask smoke --arch x86_64
cargo xtask smoke --arch aarch64
cargo xtask smoke --fault-inject

# kmtest suites
cargo xtask kmtest --filter vfs
cargo xtask kmtest --filter proc
cargo xtask kmtest --filter mm
cargo xtask kmtest --filter fs_integrity
cargo xtask kmtest --filter fault_inject

# Network tests (M5)
cargo xtask net-test --phase 1  # ARP/ICMP
cargo xtask net-test --phase 2  # UDP
cargo xtask net-test --phase 3  # TCP

# Filesystem tests (M4)
cargo xtask fs-test --corrupt ext4
cargo xtask fs-test --corrupt fat32
cargo xtask fs-test --recover

# Documentation checks
cargo xtask roadmap-check
cargo xtask doc-check
cargo xtask doc

# Performance benchmarks
cargo xtask bench --boot
cargo xtask bench --syscall
```

---

## References

- `docs/status.md` - Subsystem status registry
- `docs/syscalls.md` - Syscall correctness matrix
- `docs/milestones.md` - Milestone definitions
- `src/kmtest/` - In-kernel test suite
- `userspace/smoke/` - Userspace syscall tests
- `src/fault_inject/` - Fault injection framework

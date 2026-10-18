# RustOS Audit Ledger

- Repository: `mcgeedarion/RustOS`
- Branch: `main`
- Audited revision: `996559683a4332519f27133a2de8acdf92da9fb8`
- Audit date: 2026-10-18
- Status: **Initial source-level ledger, not a completed exhaustive audit**
- Validation note: source findings below are based on code inspection. No local build, unit-test run, QEMU run, or hardware NVMe reproduction was performed during this pass. The latest GitHub Actions run observed for the audited revision was queued, not complete.

## Scope and coverage

The repository tree at the audited revision contains 2,413 tracked files, including 1,779 paths under `target/` and `target-xtask/`, and 470 Rust files under `src/`. A focused line-level review was performed for the kernel/user memory boundary, NVMe driver, ELF validation entry points, architecture syscall paths, synchronization primitives, feature profile declarations, CI workflow, and ignore rules. The rest of the source tree has **not** yet received a manual line-by-line review. Do not treat absence of a finding for a file as evidence that it is defect-free.

Severity scale:
- **Critical**: directly reachable undefined behavior, memory corruption, or a kernel-wide safety failure.
- **High**: likely data corruption, privilege boundary failure, deadlock, or core functionality failure.
- **Medium**: correctness or reliability defect with narrower reach, or a major validation gap.
- **Low**: maintainability, efficiency, or repository hygiene issue with limited runtime impact.

## Findings

### RUSTOS-AUDIT-001: Safe user-copy APIs can invoke undefined behavior through caller-controlled kernel pointers

- **Severity:** Critical
- **Location:** `src/kernel/uaccess.rs:250-285` and `src/kernel/uaccess.rs:287-324`
- **Reproduction:**
  1. Call `copy_from_user(core::ptr::null_mut(), valid_user_address, 1)`.
  2. Or call `copy_to_user(valid_user_address, core::ptr::null(), 1)`.
  3. Both functions are safe to call, but their internal `copy_nonoverlapping` operations require valid, non-null kernel-side pointers for nonzero lengths.
- **Root cause:** Safe functions accept raw pointers for kernel-owned buffers and then dereference/copy through them in an unsafe block without being able to establish allocation validity. Null checks alone would not prove that an arbitrary non-null pointer is valid.
- **Proposed correction:** Make the kernel-side buffer parameter a safe slice (`&mut [u8]` / `&[u8]`) and derive its pointer internally, or make the raw-pointer API `unsafe` with a precise safety contract and expose a safe slice-based wrapper. Keep user-address validation and fault fixups separate from kernel-buffer validity.
- **Regression tests:**
  - Compile-time/API test that ordinary safe callers cannot pass arbitrary raw pointers.
  - Unit tests for zero-length operations and valid slice copies.
  - Negative tests that invalid user addresses return `EFAULT` without panicking.
  - Run Miri or an equivalent host-side harness on any host-testable copy abstraction.

### RUSTOS-AUDIT-002: `get_user<T: Copy>` can create invalid Rust values from untrusted user bytes

- **Severity:** Critical
- **Location:** `src/kernel/uaccess.rs:326-343`
- **Reproduction:**
  1. Instantiate `get_user::<bool>(addr)` where the user bytes at `addr` contain a value other than 0 or 1, assuming the address is mapped and readable.
  2. The implementation copies raw bytes into `MaybeUninit<T>` and calls `assume_init()`.
  3. `Copy` does not imply that every bit pattern is a valid value of `T`; invalid bool/enum/non-zero types can therefore cause undefined behavior.
- **Root cause:** The generic bound only requires `Copy`, which is not a valid bit-pattern-safety contract.
- **Proposed correction:** Restrict raw reads to an audited plain-old-data trait whose contract guarantees all bit patterns are valid, or copy bytes into a byte array and perform type-specific validation/conversion. Do not expose a safe generic API that assumes arbitrary user bytes are valid for any `Copy` type.
- **Regression tests:**
  - Compile-fail/API test proving unsupported types such as `bool` and enums cannot use the raw-byte API.
  - Tests for valid integer values, invalid encoded values, and unaligned addresses.
  - Verify invalid input returns an error rather than constructing an invalid Rust value.

### RUSTOS-AUDIT-003: NVMe driver treats a virtual slice pointer as a DMA physical address and rejects ordinary unaligned buffers

- **Severity:** High
- **Location:** `src/drivers/block/nvme.rs:157-168`, `src/drivers/block/nvme.rs:464-476`, `src/drivers/block/nvme.rs:636-643`
- **Reproduction:**
  1. Call `read_sectors` with a normal byte slice whose address is not 4 KiB aligned, a common case for stack or heap buffers.
  2. `io_rw` returns `buffer not page aligned`.
  3. For an aligned slice, the driver still casts the virtual pointer to an integer and uses it as PRP/DMA address. This is only correct if that pointer is also the physical address and the whole transfer satisfies the driver's contiguous-DMA assumptions.
- **Root cause:** The public API accepts ordinary Rust slices but the low-level path interprets their virtual addresses as physical addresses. The code does not translate/pin/map caller buffers or stage them through a dedicated DMA buffer.
- **Proposed correction:** Introduce a DMA buffer abstraction that owns pinned physical pages and provides the correct PRP list, or stage reads/writes through a known DMA-capable buffer and copy to/from the caller's slice. Make physical-contiguity and address-translation requirements explicit in the type system instead of relying on identity mapping.
- **Regression tests:**
  - Read/write using buffers with offsets 0, 1, 7, 512, and 4095 from a page-aligned allocation.
  - Test a physically non-contiguous buffer and ensure it is either correctly represented with PRPs or rejected before command submission.
  - Mock physical-to-virtual mappings that differ to catch virtual-as-physical assumptions.
  - Verify timeout and controller-error paths release locks and DMA resources.

### RUSTOS-AUDIT-004: NVMe transfer size assumes 512-byte sectors despite parsing namespace sector size

- **Severity:** High
- **Location:** `src/drivers/block/nvme.rs:432-436` and `src/drivers/block/nvme.rs:464-474`
- **Reproduction:**
  1. Use a namespace whose selected LBA format has a logical block size other than 512 bytes.
  2. `_init` records the actual size in `DiskInfo.sector_size`.
  3. `io_rw` calculates DMA length as `count * 512`, so PRP coverage and transfer memory length disagree with the namespace's actual block size.
- **Root cause:** Namespace metadata is parsed, but the low-level I/O path hardcodes 512 rather than using the selected namespace's validated sector size.
- **Proposed correction:** Pass the validated namespace sector size into `io_rw` or resolve it from controller state. Validate supported LBA formats and use checked arithmetic for byte lengths before creating PRPs.
- **Regression tests:**
  - Unit-test PRP byte lengths for 512-byte, 4096-byte, and unsupported sector sizes.
  - Verify command byte coverage equals `count * sector_size`.
  - Verify unsupported LBA formats fail initialization cleanly without a panic.

### RUSTOS-AUDIT-005: NVMe namespace LBA format is not validated before shifting

- **Severity:** High
- **Location:** `src/drivers/block/nvme.rs:432-436`
- **Reproduction:**
  1. Feed a mocked/corrupt Identify Namespace payload whose selected LBA format's `LBADS` byte is 32 or greater.
  2. The code computes `1u32 << lbads` without checking the shift count.
  3. In checked/debug builds this can panic; in any build the resulting behavior is not a valid supported-sector-size decision.
- **Root cause:** Device-provided metadata is trusted before checking that the shift is within the width of `u32` and that the sector size is supported by the driver.
- **Proposed correction:** Validate the LBA format index against the controller-reported number of formats and reject `LBADS` values outside the supported range before shifting. Return an initialization error instead of panicking.
- **Regression tests:**
  - Mock `LBADS` values 0, 9, 12, 31, 32, and 255.
  - Verify only explicitly supported sector sizes are accepted.
  - Ensure invalid Identify data returns an error and does not panic or publish a disk.

### RUSTOS-AUDIT-006: AArch64 IRQ mask assembly allows compiler memory reordering across the IRQ critical section

- **Severity:** High
- **Location:** `src/sync/spinlock.rs:208-219` and `src/sync/spinlock.rs:239-243`
- **Reproduction:**
  1. Build optimized AArch64 code using `IrqSpinLock`.
  2. Inspect optimized assembly around `lock_irqsave` and guard drop, or construct a stress test where an interrupt handler and interrupted context touch the same protected state.
  3. The DAIF mask/unmask inline assembly is declared with `options(nostack, nomem)`, which tells the compiler the assembly does not access memory and permits memory operations to be moved across it.
- **Root cause:** The IRQ state change is a compiler-ordering boundary, but the inline assembly explicitly opts out of memory effects. Acquire/release atomics serialize lock ownership, but they do not replace the requirement that accesses not move across local IRQ disable/enable when protecting against same-CPU interrupt reentrancy.
- **Proposed correction:** Remove `nomem` from the DAIF mask/unmask assembly or use an explicit compiler barrier around the critical-section boundary, then inspect generated code. Keep the architecture's lock ordering contract documented.
- **Regression tests:**
  - AArch64 optimized assembly check that protected accesses remain inside the IRQ-disabled interval.
  - QEMU AArch64 stress test with an interrupt handler acquiring the same IRQ-safe lock.
  - Nested IRQ-save tests proving prior interrupt state is restored exactly.

### RUSTOS-AUDIT-007: RISC-V signal delivery lacks a usable signal-return trampoline

- **Severity:** High for signal-handler execution; otherwise a known incomplete feature
- **Location:** `src/arch/riscv64/syscall.rs:89-111`
- **Reproduction:**
  1. Run a RISC-V userspace process that installs a signal handler.
  2. Deliver a pending signal so `setup_signal_frame` redirects `frame.pc` to the handler.
  3. Return from the handler. The code explicitly comments that it would still need to set up an `rt_sigreturn` trampoline, so a handler cannot reliably restore the original context through this path.
- **Root cause:** The signal setup path changes the program counter and argument register but does not construct a complete ABI-compatible signal frame, stack layout, or return trampoline/syscall convention.
- **Proposed correction:** Implement the RISC-V Linux signal-frame ABI end-to-end, including stack alignment, saved registers, signal mask, restorer/trampoline, and `rt_sigreturn` validation. Until then, report signal-handler support as unavailable rather than treating signal delivery as complete.
- **Regression tests:**
  - RISC-V userspace test installs a handler, handles a signal, returns to the interrupted instruction, and verifies all callee/caller-saved registers and signal mask.
  - Malformed signal-frame and invalid-restorer tests return an error or terminate safely.
  - QEMU test covers both synchronous and asynchronous signal delivery.

### RUSTOS-AUDIT-008: Generated build outputs are committed and the ignore rule does not ignore them

- **Severity:** Medium
- **Location:** `.gitignore:1`; repository tree contains 1,779 tracked files under `target/` and `target-xtask/`
- **Reproduction:**
  1. Run `git ls-files target target-xtask` after checkout.
  2. The tracked build cache, dependency metadata, object files, and target-specific outputs appear in the index.
  3. The sole ignore line is `(docs/M3_M5_ROADMAP.md)`, which is not a rule for build outputs.
- **Root cause:** The ignore file contains a parenthesized path instead of standard ignore patterns and does not exclude Cargo output directories.
- **Proposed correction:** Replace the malformed entry with deliberate valid patterns, including `/target/` and `/target-xtask/`. Remove generated artifacts from Git tracking while preserving intentional deliverables such as source, lockfiles, and any deliberately versioned boot image. Review the 64 MiB `boot-x86_64.img` and 10 MiB `rustup-init.exe` separately before deciding whether they belong in source control.
- **Regression tests:**
  - Verify `git check-ignore target/debug/example` and `git check-ignore target-xtask/debug/example` succeed.
  - Verify a clean checkout can regenerate build outputs using the documented build command.
  - Confirm no required generated output is consumed from a tracked `target/` path.

### RUSTOS-AUDIT-009: AArch64 user-access code clears the wrong control bit and does not recover from user faults

- **Severity:** Critical
- **Location:** `src/kernel/uaccess.rs:139-170`, with the architecture dispatch at `src/kernel/uaccess.rs:120-136`
- **Reproduction:**
  1. On AArch64, call `copy_from_user` or `copy_to_user` with an unmapped user address.
  2. The helper modifies `SCTLR_EL1` using `sctlr & !0x40000`; the mask is bit 18, not bit 22, and PAN is not controlled by that SCTLR bit.
  3. The AArch64 path then returns `Ok(result)` without checking a per-CPU fault-fixup state. A real translation fault may take the kernel's normal fault path instead of being converted to `EFAULT`.
- **Root cause:** The code conflates PAN state with an SCTLR bit and its fault-recovery contract is not implemented in the AArch64 helper. The module documentation itself says exception-table fault fixups have not landed, while the public copy APIs claim faults become errors.
- **Proposed correction:** Implement AArch64 user access using the architecturally correct PAN mechanism for the supported exception level, with a guaranteed restoration path. Add exception-table/fault fixups that redirect faults occurring inside the copy window and return `UaccessError::Fault`. Until fault fixups are implemented, do not expose this path as fault-safe; fail closed or keep affected syscall operations unavailable.
- **Regression tests:**
  - AArch64 QEMU tests for valid reads/writes, unmapped pages, read-only destinations, page-boundary crossing, and invalid top-half addresses.
  - Verify each invalid access returns `EFAULT` and the kernel continues running.
  - Verify the original PAN/interrupt state is restored on success and fault paths.
  - Review generated assembly against the ARM architecture manual.

### RUSTOS-AUDIT-010: RISC-V build references an undefined user-address limit

- **Severity:** High
- **Location:** `src/kernel/uaccess.rs:21-31`, `src/kernel/uaccess.rs:39-54`, and unconditional module export at `src/kernel/mod.rs:23`
- **Reproduction:**
  1. Build the full kernel for `riscv64` with the module graph enabled.
  2. `USER_ADDR_MAX` is defined only under `target_arch = "aarch64"` and `target_arch = "x86_64"`, but `user_access_ok` and `USER_SPACE_END` reference it without an architecture-specific RISC-V definition.
  3. Since `kernel::uaccess` is exported unconditionally, the RISC-V compilation path reaches these references.
- **Root cause:** The user/kernel virtual-address split was added only for x86_64 and AArch64, while the source tree still includes a RISC-V architecture path.
- **Proposed correction:** Define the RISC-V user address ceiling from the actual Sv39/Sv48/Sv57 paging configuration, or centralize the bound in the architecture paging API. Do not guess a limit that disagrees with the active page-table mode.
- **Regression tests:**
  - Compile the full kernel for every claimed architecture, including RISC-V, in CI.
  - Unit-test the last valid user address, the first kernel address, wraparound ranges, and addresses outside the selected paging mode.
  - Assert the exported `USER_SPACE_END` matches the architecture's paging configuration.

### RUSTOS-AUDIT-011: `wait4` can overflow when interpreting the most-negative PID selector

- **Severity:** Medium
- **Location:** `src/proc/wait.rs:102-120`
- **Reproduction:**
  1. Invoke `wait4` / `waitpid` with the selector value `isize::MIN` and any valid status pointer.
  2. The negative-selector branch evaluates `-n` to obtain a process-group ID.
  3. The absolute value of the minimum signed integer is not representable in `isize`, so this can overflow or produce a nonsensical selector depending on build overflow settings.
- **Root cause:** A syscall-controlled signed integer is negated without checked arithmetic.
- **Proposed correction:** Use `checked_neg()` or `checked_abs()` and return a defined error for an unrepresentable selector. Keep `-1` and `0` special cases separate as they are now.
- **Regression tests:**
  - Test selectors `-1`, `0`, positive PID, ordinary negative process-group ID, and `isize::MIN`.
  - Confirm the minimum value never panics and never matches an unintended process group.

## Related files inspected in this pass

The following files received focused inspection. This is not a claim that every line in each file has been proven correct.

- `Cargo.toml:1-230`: feature declarations and workspace setup. Default profile is `full_kernel`, expanding to `uefi_boot`; the slim-profile mutual exclusion guard is in `src/lib.rs:29-31`.
- `src/lib.rs:1-242`: profile/module graph and mutual-exclusion compile error.
- `src/kernel/uaccess.rs:180-360`: x86 user-access fault flag, copy helpers, and generic read helper.
- `src/drivers/block/nvme.rs:80-644`: controller init, PRP construction, I/O path, completion polling, and DMA allocation.
- `src/exec/elf.rs:105-260`: static ELF validation and disabled legacy loader.
- `src/proc/exec.rs:100-420`: argument copying, image preparation, VMA construction, and deferred image cleanup.
- `src/proc/wait.rs:1-375`: wait/reap implementation.
- `src/proc/scheduler.rs:1-1183`: scheduler implementation and policy declarations.
- `src/sync/advanced.rs:100-235`: reader/writer lock acquisition paths.
- `src/sync/spinlock.rs:25-245`: ticket lock and IRQ-save lock paths.
- `src/arch/x86_64/syscall.rs:1-424`: syscall entry and return path.
- `src/arch/aarch64/syscall.rs:1-75`: AArch64 syscall dispatch and signal-return path.
- `src/arch/riscv64/syscall.rs:1-131`: RISC-V syscall dispatch and signal setup.
- `build.rs:1-164`: CRT compilation and cross-compiler selection.
- `.github/workflows/ci.yml:1-81`: formatting, clippy, architecture build matrix, and x86 QEMU smoke.
- `.gitignore:1`: current ignore rule.

## Required next audit passes

1. Finish line-by-line review of every Rust file under `src/`, starting with `src/mm/`, `src/proc/`, `src/syscall/`, `src/fs/`, `src/arch/`, `src/smp/`, and `src/drivers/`.
2. Audit workspace crates, userspace programs, build scripts, fuzz targets, and all CI workflows.
3. Run the documented formatting, clippy, build matrix, focused regressions, fuzz smoke, and QEMU tests. Record each command, target/profile, commit SHA, and result.
4. For unsafe code and hardware paths, add tests for invalid inputs, overflow, lifecycle cleanup, concurrency, and architecture-specific ABI behavior.
5. Update each file's coverage status and append findings rather than assuming unlisted files are clean.

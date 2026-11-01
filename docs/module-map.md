# Complete source-file index

This index covers all 510 tracked Rust, C, and assembly files at `08a14f61030facca4d8f52223f29a6479881eab8`, including 473 Rust files under `src/`. It was generated from the source snapshot used by the 2026-11-01 [audit](source-audit.md), with 118,491 physical source lines including comments and blank lines.

## Reading the index

Each file appears once, grouped by source location rather than claimed support level. Names are representative lexical declarations, including private/test items; they are navigation aids, not an exhaustive public API or proof of runtime reachability.

“No direct declaration/include found” means the structural scan found no file edge through `mod`, literal `#[path]`, or literal `include!` variants. It is not a compiler-derived reachability result, and macro/generated/build-script paths require separate review; cfg-name mentions may include comments or tests.

The four syscall fragments `p0_gaps.rs`, `openat2_mincore.rs`, `stubs.rs`, and `posix_full.rs` are textually included and are not mislabeled as unregistered. Conditional declarations, duplicate candidates, and empty files remain in the inventory.

## Coverage summary

| Group | Source files | Guide |
| --- | --- | --- |
| Kernel crate roots and build script | 11 | [Kernel crate roots and build script](architecture.md) |
| Common architecture interfaces | 2 | [Common architecture interfaces](firmware-and-platforms.md) |
| x86_64 architecture | 20 | [x86_64 architecture](firmware-and-platforms.md) |
| AArch64 architecture | 17 | [AArch64 architecture](firmware-and-platforms.md) |
| RISC-V architecture | 13 | [RISC-V architecture](firmware-and-platforms.md) |
| Memory allocators | 1 | [Memory allocators](memory-management.md) |
| Block storage | 2 | [Block storage](filesystems-and-storage.md) |
| Console | 2 | [Console](debugging.md) |
| Kernel core utilities | 7 | [Kernel core utilities](synchronization.md) |
| Debugging | 16 | [Debugging](debugging.md) |
| Device model | 6 | [Device model](drivers-and-devices.md) |
| Display | 15 | [Display](drivers-and-devices.md) |
| Device drivers | 37 | [Device drivers](drivers-and-devices.md) |
| ELF validation and loading | 2 | [ELF validation and loading](processes-and-scheduling.md) |
| Fault injection | 2 | [Fault injection](fault_inject.md) |
| Firmware | 15 | [Firmware](firmware-and-platforms.md) |
| Filesystems and VFS | 89 | [Filesystems and VFS](filesystems-and-storage.md) |
| Initialization and runtime support | 14 | [Initialization and runtime support](firmware-and-platforms.md) |
| Common input events | 1 | [Common input events](drivers-and-devices.md) |
| Asynchronous I/O | 14 | [Asynchronous I/O](ipc-and-async-io.md) |
| Interprocess communication | 8 | [Interprocess communication](ipc-and-async-io.md) |
| Interrupt controllers | 4 | [Interrupt controllers](firmware-and-platforms.md) |
| Kernel services | 6 | [Kernel services](architecture.md) |
| Kernel test suites | 10 | [Kernel test suites](code_coverage.md) |
| Memory management | 33 | [Memory management](memory-management.md) |
| Networking | 28 | [Networking](networking.md) |
| Unregistered generic power management | 1 | [Unregistered generic power management](firmware-and-platforms.md) |
| Processes and scheduler | 37 | [Processes and scheduler](processes-and-scheduling.md) |
| Security | 24 | [Security](security.md) |
| Multiprocessing | 4 | [Multiprocessing](synchronization.md) |
| Kernel synchronization | 9 | [Kernel synchronization](synchronization.md) |
| System calls | 19 | [System calls](syscalls.md) |
| Time and timers | 6 | [Time and timers](firmware-and-platforms.md) |
| Terminals | 7 | [Terminals](drivers-and-devices.md) |
| Portable workspace crates | 9 | [Portable workspace crates](architecture.md) |
| Freestanding userspace | 11 | [Freestanding userspace](userspace.md) |
| Parser fuzz harnesses | 4 | [Parser fuzz harnesses](code_coverage.md) |
| Root integration tests | 2 | [Root integration tests](code_coverage.md) |
| Direct Rust host harness | 1 | [Direct Rust host harness](code_coverage.md) |
| Build automation | 1 | [Build automation](build-reference.md) |

## Kernel crate roots and build script

This group contains 11 tracked source files. Read [the subsystem guide](architecture.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`build.rs`](../build.rs) | 163 | `main`, `write_initramfs_embed`, `compile_crt`, `configure_crt_compiler`, `explicit_cc_override_is_set`, … | See source and subsystem guide |
| [`src/allocator.rs`](../src/allocator.rs) | 66 | `BootHeap`, `InterruptGuard`, `KernelAllocator`, `new`, `drop`, … | Installs RegionHeap in full builds |
| [`src/boot_minimal.rs`](../src/boot_minimal.rs) | 98 | `MinimalBootArch`, `BootMinimalAllocator`, `early_console_init`, `idle_once`, `enter`, … | See source and subsystem guide |
| [`src/boot_perf.rs`](../src/boot_perf.rs) | 124 | `read_hw_counter` | See source and subsystem guide |
| [`src/error_production.rs`](../src/error_production.rs) | 403 | `KernelError`, `KernelResult`, `SyscallResult`, `ErrorContext`, `IntoKernelError`, … | No direct declaration/include found |
| [`src/error.rs`](../src/error.rs) | 208 | `KernelError`, `KernelResult`, `SyscallResult`, `IntoKernelError`, `fmt`, … | See source and subsystem guide |
| [`src/kernel_main.rs`](../src/kernel_main.rs) | 30 | `kernel_main` | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/lib.rs`](../src/lib.rs) | 241 | No named Rust items extracted | Cfg-name mentions: `boot_minimal`, `fault-inject`, `input_events`, `kmtest`, `userspace_boot` |
| [`src/main.rs`](../src/main.rs) | 20 | `_start` | See source and subsystem guide |
| [`src/userspace_boot.rs`](../src/userspace_boot.rs) | 83 | `EarlyBumpAllocator`, `UserspaceBootArch`, `alloc`, `dealloc`, `early_console_init`, … | See source and subsystem guide |
| [`src/userspace_shims.rs`](../src/userspace_shims.rs) | 244 | `SpawnError`, `as_str`, `spawn_user_process_from_bytes`, `validate_executable_elf`, `validate_elf64`, … | See source and subsystem guide |

## Common architecture interfaces

This group contains 2 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/arch/api.rs`](../src/arch/api.rs) | 179 | `PageFlags`, `Output`, `TrapFrame`, `ArchInit`, `Interrupts`, … | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/arch/mod.rs`](../src/arch/mod.rs) | 101 | `Arch`, `current_unix_time_secs`, `early_putchar`, `init` | Cfg-name mentions: `boot_minimal`, `userspace_boot` |

## x86_64 architecture

This group contains 20 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/arch/x86_64/ap_boot.s`](../src/arch/x86_64/ap_boot.s) | 108 | No named Rust items extracted | See source and subsystem guide |
| [`src/arch/x86_64/apic.rs`](../src/arch/x86_64/apic.rs) | 550 | `x2apic`, `set_lapic_base`, `lapic_virt`, `lapic_read`, `lapic_write`, … | Cfg-name mentions: `boot_debug` |
| [`src/arch/x86_64/cpu.rs`](../src/arch/x86_64/cpu.rs) | 147 | `rdmsr`, `rdmsr2`, `wrmsr`, `cpuid`, `cpuid1_ecx`, … | See source and subsystem guide |
| [`src/arch/x86_64/gdt.rs`](../src/arch/x86_64/gdt.rs) | 463 | `Tss`, `PerCpuGdt`, `GdtPointer`, `PerCpu`, `zero`, … | Cfg-name mentions: `boot_debug` |
| [`src/arch/x86_64/hal.rs`](../src/arch/x86_64/hal.rs) | 405 | `ArchImpl`, `early_init`, `late_init`, `enable`, `disable`, … | See source and subsystem guide |
| [`src/arch/x86_64/idt.rs`](../src/arch/x86_64/idt.rs) | 929 | `InterruptFrame`, `IrqHandler`, `IdtEntry`, `IdtPointer`, `NmiStatus`, … | Cfg-name mentions: `gdbstub` |
| [`src/arch/x86_64/interrupts.rs`](../src/arch/x86_64/interrupts.rs) | 104 | `timer_irq_handler` | See source and subsystem guide |
| [`src/arch/x86_64/kernel_main.rs`](../src/arch/x86_64/kernel_main.rs) | 286 | `StorageBackend`, `init`, `name`, `read_sector`, `probe_storage`, … | Cfg-name mentions: `gdbstub` |
| [`src/arch/x86_64/mem_layout.rs`](../src/arch/x86_64/mem_layout.rs) | 232 | `phys_to_virt`, `virt_to_phys` | See source and subsystem guide |
| [`src/arch/x86_64/memory.rs`](../src/arch/x86_64/memory.rs) | 43 | `discover`, `discover_uefi` | See source and subsystem guide |
| [`src/arch/x86_64/mod.rs`](../src/arch/x86_64/mod.rs) | 99 | `UserspaceBootArch`, `MinimalArch`, `init`, `early_console_init`, `idle_once` | Cfg-name mentions: `boot_minimal`, `uefi_boot`, `userspace_boot` |
| [`src/arch/x86_64/paging.rs`](../src/arch/x86_64/paging.rs) | 301 | `current_cr3`, `kernel_cr3`, `load_cr3`, `invlpg`, `alloc_table`, … | See source and subsystem guide |
| [`src/arch/x86_64/pci.rs`](../src/arch/x86_64/pci.rs) | 365 | `PciDevice`, `outl`, `inl`, `config_addr`, `config_read_u32`, … | See source and subsystem guide |
| [`src/arch/x86_64/power.rs`](../src/arch/x86_64/power.rs) | 10 | `save_and_flush` | See source and subsystem guide |
| [`src/arch/x86_64/serial.rs`](../src/arch/x86_64/serial.rs) | 87 | `outb`, `inb`, `init`, `write_byte`, `write_str`, … | See source and subsystem guide |
| [`src/arch/x86_64/syscall.rs`](../src/arch/x86_64/syscall.rs) | 423 | `SyscallFrame`, `syscall_asm_entry`, `rust_syscall_handler`, `syscall_setup`, `sys_set_tid_address`, … | See source and subsystem guide |
| [`src/arch/x86_64/uefi_boot_stack.rs`](../src/arch/x86_64/uefi_boot_stack.rs) | 16 | `BootStackStorage` | See source and subsystem guide |
| [`src/arch/x86_64/uefi_entry.rs`](../src/arch/x86_64/uefi_entry.rs) | 524 | `EfiStatus`, `EfiHandle`, `EfiTableHeader`, `EfiSystemTable`, `EfiSimpleTextOutput`, … | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/arch/x86_64/uentry.rs`](../src/arch/x86_64/uentry.rs) | 148 | `alloc_user_stack`, `sysret_to_user`, `sysret_to_user_context` | See source and subsystem guide |
| [`src/arch/x86_64/xsave.rs`](../src/arch/x86_64/xsave.rs) | 125 | `xsave_init`, `xsave_area_size`, `xsave_to`, `xrstor_from`, `cpuid_leaf1`, … | See source and subsystem guide |

## AArch64 architecture

This group contains 17 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/arch/aarch64/boot.rs`](../src/arch/aarch64/boot.rs) | 75 | `aarch64_boot_main` | See source and subsystem guide |
| [`src/arch/aarch64/cpu.rs`](../src/arch/aarch64/cpu.rs) | 101 | `cpu_id`, `enable_fp_simd`, `read_ttbr0`, `write_ttbr0`, `read_ttbr1`, … | See source and subsystem guide |
| [`src/arch/aarch64/hal.rs`](../src/arch/aarch64/hal.rs) | 343 | `ArchImpl`, `early_init`, `late_init`, `enable`, `disable`, … | See source and subsystem guide |
| [`src/arch/aarch64/interrupts.rs`](../src/arch/aarch64/interrupts.rs) | 110 | `ExceptionFrame`, `init`, `aarch64_sync_handler`, `aarch64_irq_handler`, `aarch64_fiq_handler`, … | See source and subsystem guide |
| [`src/arch/aarch64/kernel_main.rs`](../src/arch/aarch64/kernel_main.rs) | 155 | `StorageBackend`, `init`, `name`, `read_sector`, `probe_storage` | See source and subsystem guide |
| [`src/arch/aarch64/mem_layout.rs`](../src/arch/aarch64/mem_layout.rs) | 151 | `align_up`, `align_down`, `l0_index`, `l1_index`, `l2_index`, … | See source and subsystem guide |
| [`src/arch/aarch64/mod.rs`](../src/arch/aarch64/mod.rs) | 92 | `UserspaceBootArch`, `MinimalArch`, `init`, `early_console_init`, `idle_once` | Cfg-name mentions: `boot_minimal`, `uefi_boot`, `userspace_boot` |
| [`src/arch/aarch64/paging.rs`](../src/arch/aarch64/paging.rs) | 178 | `zero_page`, `alloc_root_page_table`, `kernel_ttbr1`, `init_mmu`, `map_page`, … | See source and subsystem guide |
| [`src/arch/aarch64/pci.rs`](../src/arch/aarch64/pci.rs) | 118 | `PciId`, `ecam_init`, `cfg_addr`, `read32`, `write32`, … | See source and subsystem guide |
| [`src/arch/aarch64/secondary_entry.S`](../src/arch/aarch64/secondary_entry.S) | 37 | No named Rust items extracted | See source and subsystem guide |
| [`src/arch/aarch64/serial.rs`](../src/arch/aarch64/serial.rs) | 94 | `reg`, `init`, `write_byte`, `write_bytes`, `read_byte` | See source and subsystem guide |
| [`src/arch/aarch64/simd.rs`](../src/arch/aarch64/simd.rs) | 120 | `FpState`, `new`, `save`, `restore` | See source and subsystem guide |
| [`src/arch/aarch64/smp.rs`](../src/arch/aarch64/smp.rs) | 86 | `bring_up_secondaries`, `secondary_entry`, `secondary_rust_main` | See source and subsystem guide |
| [`src/arch/aarch64/syscall.rs`](../src/arch/aarch64/syscall.rs) | 74 | `handle` | See source and subsystem guide |
| [`src/arch/aarch64/uefi_entry.rs`](../src/arch/aarch64/uefi_entry.rs) | 497 | `EfiHandle`, `EfiStatus`, `EfiTableHeader`, `EfiSystemTable`, `EfiSimpleTextOutput`, … | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/arch/aarch64/uentry.rs`](../src/arch/aarch64/uentry.rs) | 173 | `alloc_user_stack`, `eret_to_user`, `eret_to_user_context` | See source and subsystem guide |
| [`src/arch/aarch64/vectors.S`](../src/arch/aarch64/vectors.S) | 138 | No named Rust items extracted | See source and subsystem guide |

## RISC-V architecture

This group contains 13 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/arch/riscv64/cpu.rs`](../src/arch/riscv64/cpu.rs) | 85 | `cpu_id`, `enable_fpu`, `read_time`, `current_privilege`, `fence_i`, … | See source and subsystem guide |
| [`src/arch/riscv64/hal.rs`](../src/arch/riscv64/hal.rs) | 449 | `ArchImpl`, `early_init`, `late_init`, `enable`, `disable`, … | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/arch/riscv64/interrupts.rs`](../src/arch/riscv64/interrupts.rs) | 129 | `init`, `init_percpu`, `handle_irq`, `enable_irq`, `disable_irq`, … | See source and subsystem guide |
| [`src/arch/riscv64/kernel_main.rs`](../src/arch/riscv64/kernel_main.rs) | 151 | `StorageBackend`, `init`, `name`, `read_sector`, `probe_storage` | See source and subsystem guide |
| [`src/arch/riscv64/mem_layout.rs`](../src/arch/riscv64/mem_layout.rs) | 112 | `align_up`, `align_down`, `l2_index`, `l1_index`, `l0_index`, … | Audited slim checks fail: unresolved SIZE |
| [`src/arch/riscv64/mod.rs`](../src/arch/riscv64/mod.rs) | 82 | `UserspaceBootArch`, `MinimalArch`, `init`, `early_console_init`, `idle_once` | Cfg-name mentions: `boot_minimal`, `uefi_boot`, `userspace_boot` |
| [`src/arch/riscv64/paging.rs`](../src/arch/riscv64/paging.rs) | 352 | `current_satp`, `satp_to_root_pa`, `kernel_satp`, `make_satp`, `load_satp`, … | See source and subsystem guide |
| [`src/arch/riscv64/pci.rs`](../src/arch/riscv64/pci.rs) | 107 | `ecam_init`, `ecam_base`, `cfg_addr`, `config_read`, `config_write`, … | See source and subsystem guide |
| [`src/arch/riscv64/serial.rs`](../src/arch/riscv64/serial.rs) | 83 | `init`, `write_byte`, `read_byte`, `putc`, `getc` | See source and subsystem guide |
| [`src/arch/riscv64/smp.rs`](../src/arch/riscv64/smp.rs) | 85 | `bring_up_secondaries`, `secondary_entry` | See source and subsystem guide |
| [`src/arch/riscv64/syscall.rs`](../src/arch/riscv64/syscall.rs) | 106 | `handle`, `syscall_return` | See source and subsystem guide |
| [`src/arch/riscv64/uefi_entry.rs`](../src/arch/riscv64/uefi_entry.rs) | 487 | `EfiHandle`, `EfiStatus`, `EfiTableHeader`, `EfiSystemTable`, `EfiSimpleTextOutput`, … | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/arch/riscv64/uentry.rs`](../src/arch/riscv64/uentry.rs) | 106 | `sret_to_user`, `sret_to_user_context` | See source and subsystem guide |

## Memory allocators

This group contains 1 tracked source file. Read [the subsystem guide](memory-management.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/allocator/region_heap.rs`](../src/allocator/region_heap.rs) | 214 | `Region`, `RegionHeap`, `Arena`, `new`, `align_up`, … | See source and subsystem guide |

## Block storage

This group contains 2 tracked source files. Read [the subsystem guide](filesystems-and-storage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/block/mod.rs`](../src/block/mod.rs) | 226 | `BlockDev`, `BlkHandle`, `BlkScheme`, `read`, `write`, … | See source and subsystem guide |
| [`src/block/virtio_blk.rs`](../src/block/virtio_blk.rs) | 370 | `VirtqDesc`, `VirtqAvail`, `VirtqUsedElem`, `VirtqUsed`, `BlkReqHeader`, … | See source and subsystem guide |

## Console

This group contains 2 tracked source files. Read [the subsystem guide](debugging.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/console/console.rs`](../src/console/console.rs) | 24 | `W`, `print`, `println`, `print_fmt`, `write_str` | See source and subsystem guide |
| [`src/console/mod.rs`](../src/console/mod.rs) | 62 | No named Rust items extracted | See source and subsystem guide |

## Kernel core utilities

This group contains 7 tracked source files. Read [the subsystem guide](synchronization.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/core/cfg_aliases.rs`](../src/core/cfg_aliases.rs) | 261 | `arch_name`, `build_profile`, `is_debug`, `test_arch_detection`, `test_build_profile`, … | Cfg-name mentions: `boot_minimal`, `debug_kernel`, `device_tree`, `fault_injection`, `gdbstub`, `kasan`, `kmtest`, `smp`, `uefi`, `userspace_boot` |
| [`src/core/collections.rs`](../src/core/collections.rs) | 179 | `ListNode`, `IntrusiveList`, `Linkable`, `RingBuf`, `new`, … | See source and subsystem guide |
| [`src/core/cpu_local.rs`](../src/core/cpu_local.rs) | 132 | `PerCpuBlock`, `CpuLocalKey`, `CpuLocal`, `read_slot`, `write_slot`, … | See source and subsystem guide |
| [`src/core/error.rs`](../src/core/error.rs) | 90 | `KernelError`, `KResult`, `to_errno`, `fmt` | See source and subsystem guide |
| [`src/core/fast_hash.rs`](../src/core/fast_hash.rs) | 243 | `KernelFastHasher`, `KernelFastMap`, `Item`, `IntoIter`, `default`, … | See source and subsystem guide |
| [`src/core/mod.rs`](../src/core/mod.rs) | 24 | No named Rust items extracted | See source and subsystem guide |
| [`src/core/panic.rs`](../src/core/panic.rs) | 111 | `EarlyWriter`, `PanicContext`, `write_str`, `do_panic`, `arch_halt` | See source and subsystem guide |

## Debugging

This group contains 16 tracked source files. Read [the subsystem guide](debugging.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/debug/commands.rs`](../src/debug/commands.rs) | 191 | `dispatch`, `cmd_info`, `cmd_info_mem`, `cmd_info_proc`, `cmd_bt`, … | See source and subsystem guide |
| [`src/debug/debugcon.rs`](../src/debug/debugcon.rs) | 61 | `_Dc`, `putb`, `puts`, `write_str` | See source and subsystem guide |
| [`src/debug/ftrace.rs`](../src/debug/ftrace.rs) | 101 | `__cyg_profile_func_enter`, `__cyg_profile_func_exit`, `drain_last_n` | Cfg-name mentions: `debug`, `trace` |
| [`src/debug/gdbstub/arch.rs`](../src/debug/gdbstub/arch.rs) | 1 | No named Rust items extracted | No direct declaration/include found |
| [`src/debug/gdbstub/breakpoints.rs`](../src/debug/gdbstub/breakpoints.rs) | 1 | No named Rust items extracted | No direct declaration/include found |
| [`src/debug/gdbstub/mod.rs`](../src/debug/gdbstub/mod.rs) | 1 | No named Rust items extracted | Empty module root; does not connect RSP files |
| [`src/debug/gdbstub/rsp_aarch64.rs`](../src/debug/gdbstub/rsp_aarch64.rs) | 138 | `AArch64Session`, `rsp_packet`, `new`, `sw_bps`, `hw_bps`, … | No direct declaration/include found |
| [`src/debug/gdbstub/rsp_vcont.rs`](../src/debug/gdbstub/rsp_vcont.rs) | 107 | `RspState`, `new`, `handle_vcont`, `range_step_done`, `arch_enable_single_step` | No direct declaration/include found |
| [`src/debug/gdbstub/rsp_x86_64.rs`](../src/debug/gdbstub/rsp_x86_64.rs) | 200 | `X86Session`, `build_gdb_regs`, `unpack_gdb_regs`, `step_set_tf`, `step_clear_tf`, … | No direct declaration/include found |
| [`src/debug/gdbstub/rsp.rs`](../src/debug/gdbstub/rsp.rs) | 269 | `Session`, `rsp_packet`, `arch_reg_buf_len`, `arch_read_regs`, `arch_write_regs`, … | No direct declaration/include found |
| [`src/debug/gdbstub/serial.rs`](../src/debug/gdbstub/serial.rs) | 136 | `SerialPort`, `SerialError`, `outb`, `inb`, `init`, … | No direct declaration/include found |
| [`src/debug/gdbstub/session.rs`](../src/debug/gdbstub/session.rs) | 1 | No named Rust items extracted | No direct declaration/include found |
| [`src/debug/gdbstub/target.rs`](../src/debug/gdbstub/target.rs) | 175 | `GdbTarget`, `attach`, `read_mem`, `write_mem`, `read_regs`, … | No direct declaration/include found |
| [`src/debug/mod.rs`](../src/debug/mod.rs) | 31 | No named Rust items extracted | Cfg-name mentions: `debug`, `gdbstub`, `kmtest`, `trace` |
| [`src/debug/oops.rs`](../src/debug/oops.rs) | 28 | `emit`, `oops`, `resolve_symbol` | Cfg-name mentions: `kmtest` |
| [`src/debug/trace.rs`](../src/debug/trace.rs) | 141 | `TraceKind`, `TraceEvent`, `RingBuf`, `emit`, `drain`, … | See source and subsystem guide |

## Device model

This group contains 6 tracked source files. Read [the subsystem guide](drivers-and-devices.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/device/mod.rs`](../src/device/mod.rs) | 35 | No named Rust items extracted | See source and subsystem guide |
| [`src/device/pci/bus.rs`](../src/device/pci/bus.rs) | 81 | `PciBus`, `init`, `all`, `find`, `probe_all`, … | See source and subsystem guide |
| [`src/device/pci/ecam.rs`](../src/device/pci/ecam.rs) | 53 | `set_base`, `addr`, `cfg_read32`, `cfg_read16`, `cfg_read8`, … | See source and subsystem guide |
| [`src/device/pci/enumerate.rs`](../src/device/pci/enumerate.rs) | 7 | No named Rust items extracted | No direct declaration/include found |
| [`src/device/pci/mod.rs`](../src/device/pci/mod.rs) | 121 | `PciDevice`, `devices`, `encode_bdf`, `decode_bdf`, `find_by_bdf`, … | See source and subsystem guide |
| [`src/device/pci/msix.rs`](../src/device/pci/msix.rs) | 58 | `msix_configure` | See source and subsystem guide |

## Display

This group contains 15 tracked source files. Read [the subsystem guide](drivers-and-devices.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/display/console.rs`](../src/display/console.rs) | 254 | `Cell`, `Console`, `default`, `new`, `feed_char`, … | See source and subsystem guide |
| [`src/display/drm/acp.rs`](../src/display/drm/acp.rs) | 435 | `AcpVersion`, `AcpEndpointKind`, `AcpEndpointId`, `AcpDmaDescriptor`, `DmaRing`, … | See source and subsystem guide |
| [`src/display/drm/connector.rs`](../src/display/drm/connector.rs) | 97 | `ConnectorType`, `ConnectionStatus`, `Connector`, `is_hdmi`, `new`, … | See source and subsystem guide |
| [`src/display/drm/crtc.rs`](../src/display/drm/crtc.rs) | 38 | `Crtc`, `new`, `set_mode`, `disable` | See source and subsystem guide |
| [`src/display/drm/encoder.rs`](../src/display/drm/encoder.rs) | 34 | `EncoderType`, `Encoder`, `new` | See source and subsystem guide |
| [`src/display/drm/framebuffer.rs`](../src/display/drm/framebuffer.rs) | 67 | `PixelFormat`, `Framebuffer`, `FramebufferDesc`, `bpp`, `new`, … | See source and subsystem guide |
| [`src/display/drm/gem.rs`](../src/display/drm/gem.rs) | 39 | `GemObject`, `new`, `get`, `put` | See source and subsystem guide |
| [`src/display/drm/hdmi.rs`](../src/display/drm/hdmi.rs) | 319 | `HdmiVersion`, `FrlRate`, `ScdcRegister`, `HdmiSinkFeatures`, `HdmiCapabilities`, … | See source and subsystem guide |
| [`src/display/drm/mod.rs`](../src/display/drm/mod.rs) | 97 | `DisplayMode`, `DrmDriver`, `DrmError`, `fmt`, `name`, … | See source and subsystem guide |
| [`src/display/drm/plane.rs`](../src/display/drm/plane.rs) | 39 | `PlaneType`, `Plane`, `new`, `attach_fb`, `detach_fb` | See source and subsystem guide |
| [`src/display/font.rs`](../src/display/font.rs) | 169 | `Psf2Header`, `Psf2Font`, `Psf2Error`, `parse`, `width`, … | See source and subsystem guide |
| [`src/display/mod.rs`](../src/display/mod.rs) | 38 | No named Rust items extracted | Cfg-name mentions: `wayland` |
| [`src/display/wayland/compositor.rs`](../src/display/wayland/compositor.rs) | 39 | `vblank_notify` | See source and subsystem guide |
| [`src/display/wayland/mod.rs`](../src/display/wayland/mod.rs) | 16 | No named Rust items extracted | See source and subsystem guide |
| [`src/display/wayland/server.rs`](../src/display/wayland/server.rs) | 50 | No named Rust items extracted | See source and subsystem guide |

## Device drivers

This group contains 37 tracked source files. Read [the subsystem guide](drivers-and-devices.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/drivers/block/ahci.rs`](../src/drivers/block/ahci.rs) | 454 | `CmdHeader`, `PrdtEntry`, `CmdTable`, `AhciPort`, `AhciBlockDev`, … | See source and subsystem guide |
| [`src/drivers/block/mod.rs`](../src/drivers/block/mod.rs) | 99 | `required_len`, `read_sectors`, `write_sectors`, `read_sectors_vec`, `write_sectors_raw` | See source and subsystem guide |
| [`src/drivers/block/nvme.rs`](../src/drivers/block/nvme.rs) | 799 | `SqEntry`, `CqEntry`, `DiskInfo`, `NvmeCtrl`, `NvmeBlockDev`, … | See source and subsystem guide |
| [`src/drivers/gpu/amdgpu_gem.rs`](../src/drivers/gpu/amdgpu_gem.rs) | 155 | `GemBo`, `AmdGpu`, `init`, `is_initialised`, `display_info`, … | See source and subsystem guide |
| [`src/drivers/gpu/drm.rs`](../src/drivers/gpu/drm.rs) | 553 | `DisplayMode`, `ConnectorType`, `Connector`, `Crtc`, `PlaneType`, … | See source and subsystem guide |
| [`src/drivers/gpu/framebuffer.rs`](../src/drivers/gpu/framebuffer.rs) | 119 | `PixelFormat`, `Framebuffer`, `bytes_per_pixel`, `default`, `new`, … | See source and subsystem guide |
| [`src/drivers/gpu/gop.rs`](../src/drivers/gpu/gop.rs) | 102 | `GopState`, `GopPixelFormat`, `init`, `is_initialised`, `display_info`, … | See source and subsystem guide |
| [`src/drivers/gpu/gpu.rs`](../src/drivers/gpu/gpu.rs) | 154 | `DisplayInfo`, `Backend`, `init_virtio`, `init_amdgpu`, `init_vga`, … | Cfg-name mentions: `amdgpu` |
| [`src/drivers/gpu/mod.rs`](../src/drivers/gpu/mod.rs) | 22 | No named Rust items extracted | Cfg-name mentions: `amdgpu` |
| [`src/drivers/gpu/vga.rs`](../src/drivers/gpu/vga.rs) | 264 | `VgaMode`, `VgaState`, `attr`, `init`, `init_lfb`, … | See source and subsystem guide |
| [`src/drivers/gpu/virtio_gpu.rs`](../src/drivers/gpu/virtio_gpu.rs) | 512 | `CtrlHdr`, `Rect`, `DisplayOne`, `RespDisplayInfo`, `CmdResource2d`, … | See source and subsystem guide |
| [`src/drivers/input/bluetooth.rs`](../src/drivers/input/bluetooth.rs) | 303 | `BdAddr`, `BtState`, `init`, `is_initialised`, `poll`, … | See source and subsystem guide |
| [`src/drivers/input/evdev.rs`](../src/drivers/input/evdev.rs) | 165 | `InputEvent`, `EventRing`, `new`, `push`, `pop`, … | See source and subsystem guide |
| [`src/drivers/input/hid.rs`](../src/drivers/input/hid.rs) | 247 | `Field`, `HidReport`, `parse_descriptor`, `dispatch`, `extract_bits`, … | See source and subsystem guide |
| [`src/drivers/input/keyboard.rs`](../src/drivers/input/keyboard.rs) | 229 | `KbdState`, `handle_scancode`, `hid_to_char`, `shift_symbol`, `read_char`, … | See source and subsystem guide |
| [`src/drivers/input/mod.rs`](../src/drivers/input/mod.rs) | 18 | No named Rust items extracted | See source and subsystem guide |
| [`src/drivers/input/mouse.rs`](../src/drivers/input/mouse.rs) | 76 | `MouseState`, `handle_byte`, `decode_packet` | See source and subsystem guide |
| [`src/drivers/input/usb.rs`](../src/drivers/input/usb.rs) | 350 | `Trb`, `SlotCtx`, `EpCtx`, `InputCtx`, `DevCtx`, … | See source and subsystem guide |
| [`src/drivers/input/virtio_input.rs`](../src/drivers/input/virtio_input.rs) | 281 | `Desc`, `Avail`, `UsedElem`, `Used`, `Vq`, … | See source and subsystem guide |
| [`src/drivers/keyboard.rs`](../src/drivers/keyboard.rs) | 10 | `read_char` | See source and subsystem guide |
| [`src/drivers/media/isp4.rs`](../src/drivers/media/isp4.rs) | 387 | `Isp4State`, `mmio_write`, `mmio_read`, `mbox_wait_ready`, `mbox_send`, … | See source and subsystem guide |
| [`src/drivers/media/media.rs`](../src/drivers/media/media.rs) | 151 | `CaptureFrame`, `Backend`, `PixelFormat`, `data`, `init_isp4`, … | Cfg-name mentions: `amd-isp4` |
| [`src/drivers/media/mod.rs`](../src/drivers/media/mod.rs) | 18 | No named Rust items extracted | Cfg-name mentions: `amd-isp4` |
| [`src/drivers/mod.rs`](../src/drivers/mod.rs) | 35 | No named Rust items extracted | See source and subsystem guide |
| [`src/drivers/net/e1000e.rs`](../src/drivers/net/e1000e.rs) | 330 | `RxDesc`, `TxDesc`, `E1000e`, `init`, `is_initialised`, … | See source and subsystem guide |
| [`src/drivers/net/mod.rs`](../src/drivers/net/mod.rs) | 91 | `init` | See source and subsystem guide |
| [`src/drivers/net/nic.rs`](../src/drivers/net/nic.rs) | 66 | `MacAddr`, `NicStats`, `is_initialised`, `mac`, `stats`, … | See source and subsystem guide |
| [`src/drivers/net/virtio_net_mmio.rs`](../src/drivers/net/virtio_net_mmio.rs) | 28 | `is_initialised`, `mac`, `stats`, `send`, `recv` | Compatibility stub |
| [`src/drivers/net/virtio_net.rs`](../src/drivers/net/virtio_net.rs) | 325 | `Desc`, `Avail`, `UsedElem`, `Used`, `Queue`, … | See source and subsystem guide |
| [`src/drivers/platform/gpio.rs`](../src/drivers/platform/gpio.rs) | 264 | `GpioBank`, `IrqTrigger`, `gpio_read`, `gpio_write`, `gpio_set_bits`, … | See source and subsystem guide |
| [`src/drivers/platform/mod.rs`](../src/drivers/platform/mod.rs) | 10 | No named Rust items extracted | See source and subsystem guide |
| [`src/drivers/platform/pcie.rs`](../src/drivers/platform/pcie.rs) | 88 | `set_ecam_base`, `cfg_read32`, `cfg_read16`, `cfg_read8`, `cfg_write32`, … | See source and subsystem guide |
| [`src/drivers/platform/tty.rs`](../src/drivers/platform/tty.rs) | 220 | `TtyMode`, `TtyState`, `new`, `tty_input`, `send_signal_to_pgrp`, … | See source and subsystem guide |
| [`src/drivers/virtio_blk.rs`](../src/drivers/virtio_blk.rs) | 63 | `required_len`, `read_sectors`, `write_sectors`, `read_sectors_vec`, `is_present` | See source and subsystem guide |
| [`src/drivers/virtio/mmio.rs`](../src/drivers/virtio/mmio.rs) | 150 | `VirtioMmio`, `new`, `device_id`, `read_device_features`, `write_driver_features`, … | See source and subsystem guide |
| [`src/drivers/virtio/mod.rs`](../src/drivers/virtio/mod.rs) | 9 | No named Rust items extracted | See source and subsystem guide |
| [`src/drivers/virtio/virtqueue.rs`](../src/drivers/virtio/virtqueue.rs) | 162 | `VirtqDesc`, `VirtqAvail`, `VirtqUsedElem`, `VirtqUsed`, `Virtqueue`, … | See source and subsystem guide |

## ELF validation and loading

This group contains 2 tracked source files. Read [the subsystem guide](processes-and-scheduling.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/exec/elf.rs`](../src/exec/elf.rs) | 655 | `Elf64Header`, `Elf64Phdr`, `ElfError`, `Elf64Hdr`, `ParseError`, … | See source and subsystem guide |
| [`src/exec/mod.rs`](../src/exec/mod.rs) | 10 | No named Rust items extracted | See source and subsystem guide |

## Fault injection

This group contains 2 tracked source files. Read [the subsystem guide](fault_inject.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/fault_inject/hooks.rs`](../src/fault_inject/hooks.rs) | 63 | `pmm_alloc_frame_hook`, `pmm_alloc_contiguous_hook`, `vmm_map_hook`, `vmm_region_hook`, `syscall_resource_hook`, … | No direct declaration/include found; Cfg-name mentions: `fault-inject` |
| [`src/fault_inject/mod.rs`](../src/fault_inject/mod.rs) | 220 | `FaultPoint`, `new`, `arm`, `disarm`, `reset_triggers`, … | Cfg-name mentions: `fault-inject` |

## Firmware

This group contains 15 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/firmware/acpi/battery.rs`](../src/firmware/acpi/battery.rs) | 266 | `BatteryInfo`, `BatteryStatus`, `read_dword`, `scan_bif`, `scan_bst`, … | See source and subsystem guide |
| [`src/firmware/acpi/cpufreq.rs`](../src/firmware/acpi/cpufreq.rs) | 346 | `Pstate`, `get_pstate_table_mut`, `rdmsr`, `wrmsr`, `read_aml_dword`, … | See source and subsystem guide |
| [`src/firmware/acpi/dmar.rs`](../src/firmware/acpi/dmar.rs) | 416 | `DmarTable`, `DmarStructureType`, `Error`, `DmarStructureHeader`, `DrhdHeader`, … | See source and subsystem guide |
| [`src/firmware/acpi/error.rs`](../src/firmware/acpi/error.rs) | 201 | `AcpiError`, `AcpiResult`, `fmt`, `is_valid_physical_address`, `test_error_display`, … | See source and subsystem guide |
| [`src/firmware/acpi/hotplug.rs`](../src/firmware/acpi/hotplug.rs) | 190 | `inb`, `outb`, `init`, `discover_hotplug_gpe_bit`, `enable_gpe_bit`, … | Cfg-name mentions: `pcie` |
| [`src/firmware/acpi/hpet.rs`](../src/firmware/acpi/hpet.rs) | 271 | `HpetTable`, `init`, `get_base_address`, `get_timer_number`, `get_minimum_tick`, … | See source and subsystem guide |
| [`src/firmware/acpi/mod.rs`](../src/firmware/acpi/mod.rs) | 499 | `RsdpV1`, `RsdpV2`, `SdtHeader`, `Madt`, `MadtEntryHdr`, … | See source and subsystem guide |
| [`src/firmware/acpi/numa.rs`](../src/firmware/acpi/numa.rs) | 425 | `MemRange`, `NumaNode`, `SratLapic`, `SratMem`, `SratX2Apic`, … | See source and subsystem guide |
| [`src/firmware/acpi/power.rs`](../src/firmware/acpi/power.rs) | 341 | `outb`, `outw`, `inw`, `fadt_u8`, `fadt_u16`, … | See source and subsystem guide |
| [`src/firmware/acpi/sleep.rs`](../src/firmware/acpi/sleep.rs) | 142 | `Facs`, `init`, `set_wakeup_vector`, `suspend_s3`, `on_resume` | See source and subsystem guide |
| [`src/firmware/acpi/tests.rs`](../src/firmware/acpi/tests.rs) | 267 | `create_rsdp_v1`, `create_rsdp_v2`, `create_sdt_header`, `test_rsdp_v1_creation`, `test_rsdp_v2_creation`, … | See source and subsystem guide |
| [`src/firmware/dt.rs`](../src/firmware/dt.rs) | 5 | `parse` | See source and subsystem guide |
| [`src/firmware/mod.rs`](../src/firmware/mod.rs) | 22 | No named Rust items extracted | See source and subsystem guide |
| [`src/firmware/psci.rs`](../src/firmware/psci.rs) | 161 | `PsciError`, `from_code`, `hvc_call`, `smc_call`, `cpu_on`, … | See source and subsystem guide |
| [`src/firmware/topology.rs`](../src/firmware/topology.rs) | 97 | `set_boot_mpidr`, `secondary_mpidrs`, `set_cpu_count`, `cpu_count` | See source and subsystem guide |

## Filesystems and VFS

This group contains 89 tracked source files. Read [the subsystem guide](filesystems-and-storage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/fs/binfmt_misc.rs`](../src/fs/binfmt_misc.rs) | 302 | `MatchType`, `BinfmtEntry`, `BinfmtTable`, `new`, `register`, … | See source and subsystem guide |
| [`src/fs/btrfs/allocator.rs`](../src/fs/btrfs/allocator.rs) | 133 | `align_up`, `alloc_alignment`, `init_alloc_cursor`, `try_alloc_logical_block`, `alloc_logical_block`, … | See source and subsystem guide |
| [`src/fs/btrfs/api.rs`](../src/fs/btrfs/api.rs) | 167 | `mount`, `parse_sys_chunk_array`, `resolve_fs_tree_root`, `with_fs`, `with_fs_mut`, … | See source and subsystem guide |
| [`src/fs/btrfs/checksum.rs`](../src/fs/btrfs/checksum.rs) | 104 | `make_crc32c_table`, `crc32c`, `verify_header_csum`, `crc32c_known_vector`, `verify_header_csum_rejects_short_blocks`, … | See source and subsystem guide |
| [`src/fs/btrfs/compression.rs`](../src/fs/btrfs/compression.rs) | 653 | `DecompressError`, `BitReader`, `HuffmanEntry`, `Huffman`, `LzoInput`, … | See source and subsystem guide |
| [`src/fs/btrfs/directory.rs`](../src/fs/btrfs/directory.rs) | 12 | `BtrfsDirItem` | See source and subsystem guide |
| [`src/fs/btrfs/extent.rs`](../src/fs/btrfs/extent.rs) | 16 | `BtrfsFileExtentItem` | See source and subsystem guide |
| [`src/fs/btrfs/inode.rs`](../src/fs/btrfs/inode.rs) | 26 | `BtrfsInodeItem` | See source and subsystem guide |
| [`src/fs/btrfs/mod.rs`](../src/fs/btrfs/mod.rs) | 24 | No named Rust items extracted | See source and subsystem guide |
| [`src/fs/btrfs/mount.rs`](../src/fs/btrfs/mount.rs) | 108 | `mount`, `parse_sys_chunk_array`, `resolve_fs_tree_root`, `read_sectors_raw`, `write_sectors_raw`, … | See source and subsystem guide |
| [`src/fs/btrfs/ops.rs`](../src/fs/btrfs/ops.rs) | 265 | `readdir`, `readlink`, `statfs`, `alloc_ino`, `insert_dirent`, … | See source and subsystem guide |
| [`src/fs/btrfs/superblock.rs`](../src/fs/btrfs/superblock.rs) | 539 | `BtrfsKey`, `BtrfsInodeItem`, `BtrfsDirItem`, `BtrfsFileExtentItem`, `BtrfsChunkItem`, … | See source and subsystem guide |
| [`src/fs/btrfs/transaction.rs`](../src/fs/btrfs/transaction.rs) | 207 | `BtrfsTransaction`, `begin`, `generation`, `is_dirty`, `is_aborted`, … | See source and subsystem guide |
| [`src/fs/btrfs/tree.rs`](../src/fs/btrfs/tree.rs) | 7 | No named Rust items extracted | See source and subsystem guide |
| [`src/fs/cdfs.rs`](../src/fs/cdfs.rs) | 214 | `CdImage`, `read_sector`, `read_u32_le`, `parse_name`, `list_dir`, … | See source and subsystem guide |
| [`src/fs/cgroupfs.rs`](../src/fs/cgroupfs.rs) | 333 | `CgFd`, `CgDirEntry`, `next_cgfs_fd`, `is_cgroupfs_fd`, `cgroupfs_open`, … | See source and subsystem guide |
| [`src/fs/close_range.rs`](../src/fs/close_range.rs) | 49 | `sys_close_range` | See source and subsystem guide |
| [`src/fs/dcache.rs`](../src/fs/dcache.rs) | 116 | `DKey`, `DCache`, `new`, `lookup`, `insert`, … | See source and subsystem guide |
| [`src/fs/devfs.rs`](../src/fs/devfs.rs) | 206 | `DevCell`, `DevfsTable`, `SyntheticDev`, `empty_majors`, `get`, … | Cfg-name mentions: `input_events` |
| [`src/fs/elf.rs`](../src/fs/elf.rs) | 177 | `ElfNote`, `NtFileEntry`, `is_elf`, `read_elf_header`, `read_phdrs`, … | See source and subsystem guide |
| [`src/fs/eventfd.rs`](../src/fs/eventfd.rs) | 174 | `EventFd`, `eventfd_register`, `eventfd_read`, `eventfd_write`, `is_eventfd`, … | See source and subsystem guide |
| [`src/fs/exfat.rs`](../src/fs/exfat.rs) | 1278 | `ClusteredExtent`, `ExfatVol`, `DirEntry`, `sector_bytes`, `max_cluster_exclusive`, … | Cfg-name mentions: `boot_debug` |
| [`src/fs/ext2/api.rs`](../src/fs/ext2/api.rs) | 195 | `mount`, `with_fs`, `with_fs_mut`, `sys_stat`, `sys_lstat`, … | See source and subsystem guide |
| [`src/fs/ext2/bitmap.rs`](../src/fs/ext2/bitmap.rs) | 18 | `alloc_block`, `alloc_inode`, `free_block`, `free_inode` | See source and subsystem guide |
| [`src/fs/ext2/block.rs`](../src/fs/ext2/block.rs) | 12 | `read_block`, `write_block` | See source and subsystem guide |
| [`src/fs/ext2/directory.rs`](../src/fs/ext2/directory.rs) | 7 | No named Rust items extracted | See source and subsystem guide |
| [`src/fs/ext2/inode.rs`](../src/fs/ext2/inode.rs) | 951 | `Superblock`, `BgDesc`, `Inode`, `RawDirEntry`, `Ext2Stat`, … | See source and subsystem guide |
| [`src/fs/ext2/mod.rs`](../src/fs/ext2/mod.rs) | 33 | `is_dir` | See source and subsystem guide |
| [`src/fs/ext2/superblock.rs`](../src/fs/ext2/superblock.rs) | 192 | `Superblock`, `BgDesc`, `Inode`, `DirEntry`, `Ext2Stat`, … | See source and subsystem guide |
| [`src/fs/ext2/symlink.rs`](../src/fs/ext2/symlink.rs) | 111 | `is_symlink_mode`, `is_fast_symlink`, `fast_symlink_bytes`, `read_fast_symlink_target`, `encode_fast_symlink_target`, … | See source and subsystem guide |
| [`src/fs/ext4_write.rs`](../src/fs/ext4_write.rs) | 822 | `WriteResult`, `BlockAllocation`, `mark_dirty`, `clear_dirty`, `flush_dirty`, … | See source and subsystem guide |
| [`src/fs/ext4.rs`](../src/fs/ext4.rs) | 1308 | `Superblock`, `BgDesc32`, `BgDesc64`, `Inode`, `ExtentHeader`, … | See source and subsystem guide |
| [`src/fs/ext4/journal/replay.rs`](../src/fs/ext4/journal/replay.rs) | 311 | `JournalReplay`, `Ext4Superblock`, `ReplayError`, `BlockType`, `TransactionDescriptor`, … | No direct declaration/include found |
| [`src/fs/fanotify.rs`](../src/fs/fanotify.rs) | 387 | `QueuedEvent`, `Mark`, `FanotifyInstance`, `FanotifyScheme`, `scheme_bfd_to_table_fdno`, … | See source and subsystem guide |
| [`src/fs/fat32.rs`](../src/fs/fat32.rs) | 780 | `Bpb`, `DirEntry`, `FatFile`, `Fat32Fs`, `FatStatfs`, … | Raw block read/write are simulated |
| [`src/fs/fcntl.rs`](../src/fs/fcntl.rs) | 464 | `FdMeta`, `count_open_fds`, `check_nofile_limit`, `set_cloexec`, `is_cloexec`, … | See source and subsystem guide |
| [`src/fs/flock.rs`](../src/fs/flock.rs) | 275 | `Flock`, `LockEntry`, `InodeLocks`, `count_locks_for`, `check_lock_limit`, … | See source and subsystem guide |
| [`src/fs/fs_recognizer.rs`](../src/fs/fs_recognizer.rs) | 122 | `probe`, `fs_type_name` | See source and subsystem guide |
| [`src/fs/getdents.rs`](../src/fs/getdents.rs) | 142 | `Dent`, `strip_proc_pid`, `gather_entries`, `sys_getdents64`, `sys_getdents` | See source and subsystem guide |
| [`src/fs/initramfs.rs`](../src/fs/initramfs.rs) | 108 | `mount_initramfs`, `parent_of`, `count_entries` | Cfg-name mentions: `vfs_symlink` |
| [`src/fs/inotify.rs`](../src/fs/inotify.rs) | 374 | `QueuedEvent`, `Watch`, `InotifyInstance`, `InotifyScheme`, `scheme_bfd_to_table_fdno`, … | See source and subsystem guide |
| [`src/fs/io_syscalls.rs`](../src/fs/io_syscalls.rs) | 735 | `IoVec`, `cpid`, `synth_offset_get`, `synth_offset_advance`, `synth_offset_reset`, … | See source and subsystem guide |
| [`src/fs/ioctl/block.rs`](../src/fs/ioctl/block.rs) | 31 | `blk_ioctl` | See source and subsystem guide |
| [`src/fs/ioctl/consts.rs`](../src/fs/ioctl/consts.rs) | 53 | No named Rust items extracted | See source and subsystem guide |
| [`src/fs/ioctl/file.rs`](../src/fs/ioctl/file.rs) | 23 | `vfs_fionread`, `pipe_fionread`, `set_nonblock` | See source and subsystem guide |
| [`src/fs/ioctl/mod.rs`](../src/fs/ioctl/mod.rs) | 119 | `sys_ioctl` | See source and subsystem guide |
| [`src/fs/ioctl/net.rs`](../src/fs/ioctl/net.rs) | 100 | `sioc_ioctl`, `read_ifname` | See source and subsystem guide |
| [`src/fs/ioctl/tty.rs`](../src/fs/ioctl/tty.rs) | 71 | `tty_ioctl`, `vfs_fionread` | See source and subsystem guide |
| [`src/fs/ipc_proxy_scheme.rs`](../src/fs/ipc_proxy_scheme.rs) | 233 | `IpcProxyScheme`, `new`, `with_endpoint`, `call`, `open`, … | See source and subsystem guide |
| [`src/fs/jbd2.rs`](../src/fs/jbd2.rs) | 942 | `JournalFeatures`, `JournalSuperblock`, `DescriptorTag`, `RevokeRecord`, `PendingTxn`, … | See source and subsystem guide |
| [`src/fs/mod.rs`](../src/fs/mod.rs) | 60 | No named Rust items extracted | See source and subsystem guide |
| [`src/fs/mount.rs`](../src/fs/mount.rs) | 423 | `FsType`, `OverlayOpts`, `MountEntry`, `FsHandle`, `MountTable`, … | See source and subsystem guide |
| [`src/fs/nfs.rs`](../src/fs/nfs.rs) | 829 | `Fh`, `NfsStat`, `NfsDirEntry`, `NfsFsStat`, `NfsClient`, … | See source and subsystem guide |
| [`src/fs/ntfs.rs`](../src/fs/ntfs.rs) | 380 | `NtfsVol`, `read_u16_le`, `read_u32_le`, `read_u64_le`, `read_i64_le`, … | See source and subsystem guide |
| [`src/fs/overlayfs.rs`](../src/fs/overlayfs.rs) | 601 | `OverlayMount`, `Fs`, `TestOverlay`, `from_opts`, `upper_path`, … | See source and subsystem guide |
| [`src/fs/path.rs`](../src/fs/path.rs) | 36 | `normalize`, `is_root`, `resolve_path` | See source and subsystem guide |
| [`src/fs/pidfd.rs`](../src/fs/pidfd.rs) | 273 | `PidFdScheme`, `scheme_bfd_to_table_fdno`, `open`, `read`, `write`, … | See source and subsystem guide |
| [`src/fs/pipe.rs`](../src/fs/pipe.rs) | 619 | `PipeInner`, `PipeState`, `PipeTable`, `PipeReadSource`, `PipeWriteSource`, … | See source and subsystem guide |
| [`src/fs/poll_ext.rs`](../src/fs/poll_ext.rs) | 5 | `poll_fd_once` | See source and subsystem guide |
| [`src/fs/poll.rs`](../src/fs/poll.rs) | 141 | `StdinSource`, `user_fd_to_bfd`, `new`, `poll`, `wait_queue`, … | See source and subsystem guide |
| [`src/fs/proc_debug.rs`](../src/fs/proc_debug.rs) | 340 | `ProcDebugKind`, `DebugFd`, `is_proc_debug_fd`, `alloc_fd`, `may_debug`, … | See source and subsystem guide |
| [`src/fs/proc/syscall_stats.rs`](../src/fs/proc/syscall_stats.rs) | 59 | `read_syscall_stats` | Cfg-name mentions: `syscall-trace` |
| [`src/fs/process_fd.rs`](../src/fs/process_fd.rs) | 546 | `FdEntry`, `ProcFdTable`, `new`, `alloc_fd`, `get`, … | See source and subsystem guide |
| [`src/fs/procfs_binfmt.rs`](../src/fs/procfs_binfmt.rs) | 173 | `is_globally_enabled`, `owns_path`, `read`, `write`, `render_entry`, … | See source and subsystem guide |
| [`src/fs/procfs.rs`](../src/fs/procfs.rs) | 860 | `ProcFd`, `ProcSchemeFd`, `ProcFs`, `is_procfs_fd`, `procfs_read`, … | See source and subsystem guide |
| [`src/fs/ramfs.rs`](../src/fs/ramfs.rs) | 1130 | `INodeData`, `INode`, `TmpFsStatfs`, `TmpFs`, `RamSchemeFd`, … | See source and subsystem guide |
| [`src/fs/scheme_fd.rs`](../src/fs/scheme_fd.rs) | 222 | `SchemeFdEntry`, `SchemeFdStore`, `DummyScheme`, `alloc_scheme_backing_fd`, `free_scheme_backing_fd`, … | See source and subsystem guide |
| [`src/fs/scheme_table.rs`](../src/fs/scheme_table.rs) | 263 | `Scheme`, `SchemeTable`, `EchoScheme`, `open`, `read`, … | See source and subsystem guide |
| [`src/fs/shm.rs`](../src/fs/shm.rs) | 118 | `shm_path`, `shm_open`, `shm_unlink`, `sys_shm_open`, `sys_shm_unlink` | See source and subsystem guide |
| [`src/fs/signalfd.rs`](../src/fs/signalfd.rs) | 172 | `SignalFd`, `read_mask`, `encode_siginfo`, `signalfd_register`, `is_signalfd`, … | See source and subsystem guide |
| [`src/fs/splice.rs`](../src/fs/splice.rs) | 126 | `sys_splice`, `read_loff`, `write_loff`, `current_offset`, `advance_offset`, … | See source and subsystem guide |
| [`src/fs/stat_syscalls.rs`](../src/fs/stat_syscalls.rs) | 692 | `Stat`, `read_path`, `user_fd_to_bfd`, `current_cwd`, `resolve_path`, … | See source and subsystem guide |
| [`src/fs/sysfs.rs`](../src/fs/sysfs.rs) | 496 | `SysEntry`, `DirEntry`, `SysSchemeFd`, `SysFs`, `is_sysfs_fd`, … | See source and subsystem guide |
| [`src/fs/timerfd.rs`](../src/fs/timerfd.rs) | 373 | `ItimerspecNs`, `TimerFdEntry`, `TimerFdScheme`, `scheme_bfd_to_table_fdno`, `open`, … | See source and subsystem guide |
| [`src/fs/tmpfs.rs`](../src/fs/tmpfs.rs) | 871 | `TmpfsKind`, `TmpfsNode`, `TmpfsMount`, `TmpSchemeFd`, `TmpFs`, … | See source and subsystem guide |
| [`src/fs/url_dispatch.rs`](../src/fs/url_dispatch.rs) | 944 | `OverlaySchemeFd`, `OverlayFs`, `CgroupSchemeFd`, `CgroupFs`, `DevSchemeKind`, … | Cfg-name mentions: `input_events` |
| [`src/fs/vfs_extras.rs`](../src/fs/vfs_extras.rs) | 185 | `FlockOp`, `AdvisoryLock`, `set_times`, `sync_all`, `fsync_fd`, … | See source and subsystem guide |
| [`src/fs/vfs/fd.rs`](../src/fs/vfs/fd.rs) | 536 | `RawFd`, `VfsStat`, `InodeMeta`, `alloc_raw_fd`, `lsm_ctx_for_stat`, … | See source and subsystem guide |
| [`src/fs/vfs/inode_ref.rs`](../src/fs/vfs/inode_ref.rs) | 150 | `InodeKey`, `Entry`, `inc_open`, `dec_open`, `mark_unlinked`, … | See source and subsystem guide |
| [`src/fs/vfs/link.rs`](../src/fs/vfs/link.rs) | 143 | `LinkArgs`, `is_dir`, `link_preflight`, `base`, `link_ok`, … | See source and subsystem guide |
| [`src/fs/vfs/mod.rs`](../src/fs/vfs/mod.rs) | 198 | `VfsMountOps`, `VfsScheme`, `fstat`, `exists`, `is_dir`, … | See source and subsystem guide |
| [`src/fs/vfs/open_check.rs`](../src/fs/vfs/open_check.rs) | 276 | `InodeInfo`, `OpenAction`, `is_write`, `is_creat`, `is_excl`, … | See source and subsystem guide |
| [`src/fs/vfs/ops.rs`](../src/fs/vfs/ops.rs) | 1185 | `KStat`, `KStatfs`, `DirEntry`, `PollFlags`, `Output`, … | See source and subsystem guide |
| [`src/fs/vfs/perm.rs`](../src/fs/vfs/perm.rs) | 322 | `Creds`, `InodePerm`, `PermError`, `is_root`, `from`, … | See source and subsystem guide |
| [`src/fs/vfs/rename.rs`](../src/fs/vfs/rename.rs) | 224 | `RenameArgs`, `RenameOutcome`, `is_dir`, `dst_is_descendant_of_src`, `rename_preflight`, … | See source and subsystem guide |
| [`src/fs/vfs/symlink.rs`](../src/fs/vfs/symlink.rs) | 354 | `ResolveError`, `FsOps`, `Ctx`, `MemFs`, `from`, … | See source and subsystem guide |
| [`src/fs/vfs/tests/posix_vfs.rs`](../src/fs/vfs/tests/posix_vfs.rs) | 604 | `MemFs`, `ucreds`, `rcreds`, `existing`, `missing_rw`, … | No direct declaration/include found |
| [`src/fs/vfs/unlink.rs`](../src/fs/vfs/unlink.rs) | 192 | `UnlinkTarget`, `UnlinkOutcome`, `is_dir`, `unlink_preflight`, `key`, … | See source and subsystem guide |
| [`src/fs/vfs/uring.rs`](../src/fs/vfs/uring.rs) | 99 | `alloc_fd_for_uring`, `uring_fd_to_ring`, `close_uring_fd`, `is_uring_fd`, `fsync`, … | See source and subsystem guide |

## Initialization and runtime support

This group contains 14 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/init/boot_info.rs`](../src/init/boot_info.rs) | 144 | `BootRange`, `EfiMemoryMapInfo`, `FramebufferInfo`, `BootPriority`, `BootInfo`, … | See source and subsystem guide |
| [`src/init/crt/compiler_rt.c`](../src/init/crt/compiler_rt.c) | 46 | No named Rust items extracted | Freestanding C input to build.rs |
| [`src/init/crt/crt0.c`](../src/init/crt/crt0.c) | 34 | No named Rust items extracted | Freestanding C input to build.rs |
| [`src/init/crt/memcpy.c`](../src/init/crt/memcpy.c) | 40 | No named Rust items extracted | Freestanding C input to build.rs |
| [`src/init/crt/memmove.c`](../src/init/crt/memmove.c) | 20 | No named Rust items extracted | Freestanding C input to build.rs |
| [`src/init/crt/memset.c`](../src/init/crt/memset.c) | 12 | No named Rust items extracted | Freestanding C input to build.rs |
| [`src/init/crt/mod.rs`](../src/init/crt/mod.rs) | 17 | No named Rust items extracted | See source and subsystem guide |
| [`src/init/initramfs/mod.rs`](../src/init/initramfs/mod.rs) | 303 | `InitramfsHandle`, `CpioEntry`, `CpioIter`, `Item`, `set_initramfs_range`, … | See source and subsystem guide |
| [`src/init/loader/auxv.rs`](../src/init/loader/auxv.rs) | 133 | `build_stack` | See source and subsystem guide |
| [`src/init/loader/elf64.rs`](../src/init/loader/elf64.rs) | 236 | `ElfError`, `Elf64Ehdr`, `Elf64Phdr`, `LoadedElf`, `load_elf64` | See source and subsystem guide |
| [`src/init/loader/mod.rs`](../src/init/loader/mod.rs) | 4 | No named Rust items extracted | See source and subsystem guide |
| [`src/init/mod.rs`](../src/init/mod.rs) | 33 | No named Rust items extracted | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/init/schemes.rs`](../src/init/schemes.rs) | 69 | `init` | See source and subsystem guide |
| [`src/init/service_manager.rs`](../src/init/service_manager.rs) | 104 | `RestartPolicy`, `ServiceSpec`, `ServiceState`, `driver_service_caps`, `register_service`, … | See source and subsystem guide |

## Common input events

This group contains 1 tracked source file. Read [the subsystem guide](drivers-and-devices.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/input/mod.rs`](../src/input/mod.rs) | 617 | `InputEvent`, `EvdevRingBuf`, `DeviceInfo`, `CapBits`, `InputDevice`, … | Cfg-name mentions: `input_events` |

## Asynchronous I/O

This group contains 14 tracked source files. Read [the subsystem guide](ipc-and-async-io.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/io_uring/cqe.rs`](../src/io_uring/cqe.rs) | 70 | `Cqe`, `zeroed`, `result`, `raw_result`, `would_block`, … | See source and subsystem guide |
| [`src/io_uring/mod.rs`](../src/io_uring/mod.rs) | 41 | `IoUringError`, `is_epoll_fd` | See source and subsystem guide |
| [`src/io_uring/ops/accept.rs`](../src/io_uring/ops/accept.rs) | 103 | `IoAccept`, `Output`, `handle`, `new`, `poll` | Cfg-name mentions: `boot_debug` |
| [`src/io_uring/ops/connect.rs`](../src/io_uring/ops/connect.rs) | 80 | `IoConnect`, `Output`, `handle`, `new`, `poll` | Cfg-name mentions: `boot_debug` |
| [`src/io_uring/ops/mod.rs`](../src/io_uring/ops/mod.rs) | 67 | `dispatch`, `handle_close` | Cfg-name mentions: `boot_debug` |
| [`src/io_uring/ops/read.rs`](../src/io_uring/ops/read.rs) | 96 | `IoRead`, `Output`, `handle`, `new`, `poll` | Cfg-name mentions: `boot_debug` |
| [`src/io_uring/ops/write.rs`](../src/io_uring/ops/write.rs) | 87 | `IoWrite`, `Output`, `handle`, `new`, `poll` | Cfg-name mentions: `boot_debug` |
| [`src/io_uring/ring_buf.rs`](../src/io_uring/ring_buf.rs) | 77 | `RingBuffer`, `new`, `slot`, `available`, `free_slots`, … | See source and subsystem guide |
| [`src/io_uring/ring_pub.rs`](../src/io_uring/ring_pub.rs) | 42 | `CqRingHeadTail`, `available` | See source and subsystem guide |
| [`src/io_uring/ring.rs`](../src/io_uring/ring.rs) | 539 | `IoUringSqe`, `IoUringCqe`, `IoUringParams`, `SqRingOffsets`, `CqRingOffsets`, … | See source and subsystem guide |
| [`src/io_uring/scheduler_integration.rs`](../src/io_uring/scheduler_integration.rs) | 73 | `scheduler_tick`, `post_cqe_from_irq` | See source and subsystem guide |
| [`src/io_uring/sqe.rs`](../src/io_uring/sqe.rs) | 161 | `Sqe`, `zeroed`, `read`, `write`, `accept`, … | See source and subsystem guide |
| [`src/io_uring/syscall.rs`](../src/io_uring/syscall.rs) | 328 | `current_cancel`, `sys_io_uring_setup`, `sys_io_uring_enter`, `sys_io_uring_register`, `io_uring_close` | See source and subsystem guide |
| [`src/io_uring/token.rs`](../src/io_uring/token.rs) | 17 | `next`, `peek` | No direct declaration/include found |

## Interprocess communication

This group contains 8 tracked source files. Read [the subsystem guide](ipc-and-async-io.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/ipc/completeness.rs`](../src/ipc/completeness.rs) | 1201 | `IpcStatus`, `BenchmarkResult`, `TestResult`, `SmpTestState`, `IpcStatusSummary`, … | See source and subsystem guide |
| [`src/ipc/key.rs`](../src/ipc/key.rs) | 23 | `ftok` | See source and subsystem guide |
| [`src/ipc/mod.rs`](../src/ipc/mod.rs) | 236 | `EndpointState`, `IpcPerm`, `valid_message_len`, `endpoint_create_for_current`, `endpoint_exists`, … | See source and subsystem guide |
| [`src/ipc/mq.rs`](../src/ipc/mq.rs) | 381 | `MqAttr`, `PriMsg`, `MqInner`, `MqObject`, `MqdEntry`, … | See source and subsystem guide |
| [`src/ipc/msg.rs`](../src/ipc/msg.rs) | 225 | `Message`, `MsgQueue`, `alloc_id`, `msgget`, `msgsnd`, … | See source and subsystem guide |
| [`src/ipc/pipe_scheme.rs`](../src/ipc/pipe_scheme.rs) | 317 | `PipeBuf`, `PipeScheme`, `new`, `is_empty`, `is_full`, … | See source and subsystem guide |
| [`src/ipc/sem.rs`](../src/ipc/sem.rs) | 268 | `Sem`, `SemSet`, `WaitKind`, `SemBuf`, `alloc_id`, … | See source and subsystem guide |
| [`src/ipc/shm.rs`](../src/ipc/shm.rs) | 219 | `ShmSegment`, `alloc_id`, `shmget`, `shmat`, `shmdt`, … | See source and subsystem guide |

## Interrupt controllers

This group contains 4 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/irq/aarch64/gic.rs`](../src/irq/aarch64/gic.rs) | 99 | `GicVersion`, `GicConfig`, `qemu_virt_v2`, `qemu_virt_v3`, `init`, … | See source and subsystem guide |
| [`src/irq/aarch64/mod.rs`](../src/irq/aarch64/mod.rs) | 8 | No named Rust items extracted | See source and subsystem guide |
| [`src/irq/mod.rs`](../src/irq/mod.rs) | 13 | No named Rust items extracted | See source and subsystem guide |
| [`src/irq/riscv64/mod.rs`](../src/irq/riscv64/mod.rs) | 12 | No named Rust items extracted | Declares missing plic file |

## Kernel services

This group contains 6 tracked source files. Read [the subsystem guide](architecture.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/kernel/architecture.rs`](../src/kernel/architecture.rs) | 111 | `KernelArchitecture`, `HybridKernelContract`, `validate_hybrid_contract`, `is_hybrid_kernel`, `log_kernel_architecture`, … | See source and subsystem guide |
| [`src/kernel/mod.rs`](../src/kernel/mod.rs) | 32 | No named Rust items extracted | Cfg-name mentions: `boot_minimal`, `userspace_boot` |
| [`src/kernel/panic.rs`](../src/kernel/panic.rs) | 271 | `ArchSerialWriter`, `set_fault_addr`, `take_fault_addr`, `halt_loop`, `panic`, … | Cfg-name mentions: `boot_minimal`, `debug`, `userspace_boot` |
| [`src/kernel/rand.rs`](../src/kernel/rand.rs) | 223 | `PaddedAtomicU64`, `read_apic_id`, `seed_from_hw`, `seed_from_tsc`, `hw_seed_raw`, … | Cfg-name mentions: `debug` |
| [`src/kernel/uaccess.rs`](../src/kernel/uaccess.rs) | 809 | `UaccessError`, `UaccessResult`, `UserPod`, `UserPtr`, `UserSlice`, … | See source and subsystem guide |
| [`src/kernel/utils.rs`](../src/kernel/utils.rs) | 127 | `align_up`, `align_down`, `is_aligned`, `round_down_pow2`, `round_up_pow2`, … | See source and subsystem guide |

## Kernel test suites

This group contains 10 tracked source files. Read [the subsystem guide](code_coverage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/kmtest/fault_inject_tests.rs`](../src/kmtest/fault_inject_tests.rs) | 265 | `reset_all`, `test_pmm_oom_injection`, `test_pmm_contiguous_skip_budget`, `test_vmm_map_failure`, `test_vmm_region_failure`, … | No direct declaration/include found; Cfg-name mentions: `fault-inject`, `kmtest` |
| [`src/kmtest/fs_integrity.rs`](../src/kmtest/fs_integrity.rs) | 802 | `path`, `vfs_basic_write_read`, `test_vfs_directory_operations`, `test_vfs_seek_operations`, `test_ext4_large_file_handling`, … | Cfg-name mentions: `kmtest` |
| [`src/kmtest/fs.rs`](../src/kmtest/fs.rs) | 246 | `path`, `fs_write_read_roundtrip`, `fs_excl_existing`, `fs_rename_atomic`, `fs_unlink_removes`, … | See source and subsystem guide |
| [`src/kmtest/ipc.rs`](../src/kmtest/ipc.rs) | 156 | `ipc_pipe_roundtrip`, `ipc_pipe_multi_write`, `ipc_socketpair_echo`, `ipc_io_uring_nop`, `ipc_sysv_msg_roundtrip`, … | See source and subsystem guide |
| [`src/kmtest/mm.rs`](../src/kmtest/mm.rs) | 244 | `mm_map_unmap`, `mm_partial_unmap`, `mm_mprotect_upgrade`, `mm_mprotect_none_roundtrip`, `mm_double_unmap_safe`, … | See source and subsystem guide |
| [`src/kmtest/mod.rs`](../src/kmtest/mod.rs) | 66 | `KmTestResult`, `KmTestEntry`, `register_entry`, `with_registry`, `init` | Cfg-name mentions: `kmtest` |
| [`src/kmtest/oom_recovery.rs`](../src/kmtest/oom_recovery.rs) | 387 | `ProcessInfo`, `oom_detection_threshold`, `oom_allocation_failure`, `memory_pressure_page_cache_drop`, `anonymous_page_reclaim`, … | Cfg-name mentions: `kmtest` |
| [`src/kmtest/panic_format_tests.rs`](../src/kmtest/panic_format_tests.rs) | 207 | `capture_write`, `captured`, `reset_capture`, `assert_contains`, `test_oops_section_headers`, … | Cfg-name mentions: `kmtest` |
| [`src/kmtest/proc.rs`](../src/kmtest/proc.rs) | 189 | `proc_fork_pid_semantics`, `proc_fork_multiple`, `proc_waitpid_wnohang`, `proc_waitpid_bad_status_preserves_zombie`, `proc_signal_self_ignore`, … | See source and subsystem guide |
| [`src/kmtest/sync.rs`](../src/kmtest/sync.rs) | 147 | `sync_spinlock_basic`, `sync_mutex_trylock`, `sync_mutex_value`, `sync_semaphore_counting`, `sync_rwlock_readers_writers`, … | See source and subsystem guide |

## Memory management

This group contains 33 tracked source files. Read [the subsystem guide](memory-management.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/mm/allocator/mod.rs`](../src/mm/allocator/mod.rs) | 19 | No named Rust items extracted | Compatibility aliases, not independent global allocators |
| [`src/mm/boot_memory.rs`](../src/mm/boot_memory.rs) | 84 | `RegionKind`, `Region`, `Regions`, `end`, `is_usable`, … | See source and subsystem guide |
| [`src/mm/bump_allocator.rs`](../src/mm/bump_allocator.rs) | 263 | `BumpAllocator`, `new`, `init`, `is_initialized`, `bytes_allocated`, … | See source and subsystem guide |
| [`src/mm/core_dump.rs`](../src/mm/core_dump.rs) | 1 | No named Rust items extracted | See source and subsystem guide |
| [`src/mm/cow_fault.rs`](../src/mm/cow_fault.rs) | 108 | `handle_cow_fault`, `prot_to_flags` | See source and subsystem guide |
| [`src/mm/heap.rs`](../src/mm/heap.rs) | 78 | `heap_init`, `init_heap_tracking`, `grow`, `committed_pages`, `committed_bytes`, … | See source and subsystem guide |
| [`src/mm/kasan.rs`](../src/mm/kasan.rs) | 119 | `init`, `shadow_addr`, `mark`, `mark_accessible`, `mark_redzone`, … | See source and subsystem guide |
| [`src/mm/kstack.rs`](../src/mm/kstack.rs) | 143 | `Backing`, `KstackInfo`, `stack_top`, `retire`, `writable_flags`, … | See source and subsystem guide |
| [`src/mm/kstack/ownership.rs`](../src/mm/kstack/ownership.rs) | 127 | `Token`, `State`, `Slot`, `Registry`, `new`, … | See source and subsystem guide |
| [`src/mm/memmap.rs`](../src/mm/memmap.rs) | 64 | `BootSource`, `phys_to_virt`, `memmap_init` | See source and subsystem guide |
| [`src/mm/mlock.rs`](../src/mm/mlock.rs) | 257 | `page_align_down`, `page_align_up_checked`, `align_range`, `charge_lock_bytes`, `discharge_lock_bytes`, … | See source and subsystem guide |
| [`src/mm/mmap/anonymous.rs`](../src/mm/mmap/anonymous.rs) | 94 | `alloc_user_stack`, `clear_vmas_pub`, `coalesce_or_insert_heap_vma`, `trim_heap_vma` | See source and subsystem guide |
| [`src/mm/mmap/fault.rs`](../src/mm/mmap/fault.rs) | 32 | `free_address_space`, `clear_vmas_internal` | See source and subsystem guide |
| [`src/mm/mmap/mapping.rs`](../src/mm/mmap/mapping.rs) | 298 | `sys_mmap`, `va_end_of`, `remove_vma_inner`, `mmap_phys` | See source and subsystem guide |
| [`src/mm/mmap/mm_lock.rs`](../src/mm/mmap/mm_lock.rs) | 46 | `with_mm_write`, `current_as_bytes`, `check_rlimit_as` | See source and subsystem guide |
| [`src/mm/mmap/mod.rs`](../src/mm/mmap/mod.rs) | 86 | `VmaKind`, `Vma`, `is_heap`, `is_stack`, `contains` | See source and subsystem guide |
| [`src/mm/mmap/protection.rs`](../src/mm/mmap/protection.rs) | 51 | `sys_mprotect`, `prot_to_flags` | See source and subsystem guide |
| [`src/mm/mmap/syscalls.rs`](../src/mm/mmap/syscalls.rs) | 132 | `sys_munmap`, `sys_brk`, `set_brk_base`, `set_brk_base_compute` | See source and subsystem guide |
| [`src/mm/mmap/vma.rs`](../src/mm/mmap/vma.rs) | 90 | `insert_vma`, `remove_vma`, `find_vma`, `clone_vmas`, `clear_vmas_internal`, … | See source and subsystem guide |
| [`src/mm/mod.rs`](../src/mm/mod.rs) | 197 | `UserBuffer`, `init`, `remaining`, `write_bytes`, `read_bytes`, … | See source and subsystem guide |
| [`src/mm/oom_handler.rs`](../src/mm/oom_handler.rs) | 338 | `MemoryPressure`, `OomPolicy`, `MemoryStats`, `current`, `pressure`, … | Cfg-name mentions: `swap` |
| [`src/mm/page_fault.rs`](../src/mm/page_fault.rs) | 178 | `handle_demand_fault`, `prot_to_flags` | See source and subsystem guide |
| [`src/mm/phys.rs`](../src/mm/phys.rs) | 172 | `virt_to_phys`, `phys_to_virt`, `align_down`, `align_up`, `is_aligned`, … | See source and subsystem guide |
| [`src/mm/pkeys.rs`](../src/mm/pkeys.rs) | 77 | `sys_pkey_alloc`, `sys_pkey_free`, `sys_pkey_mprotect` | See source and subsystem guide |
| [`src/mm/pmm_fault.rs`](../src/mm/pmm_fault.rs) | 46 | `check_alloc_frame`, `check_alloc_contiguous` | No direct declaration/include found; Cfg-name mentions: `fault-inject` |
| [`src/mm/pmm.rs`](../src/mm/pmm.rs) | 465 | `Pool`, `PageInfo`, `NodeRange`, `BuddyNode`, `pool_base`, … | See source and subsystem guide |
| [`src/mm/pmm/init_tests.rs`](../src/mm/pmm/init_tests.rs) | 113 | `spans_reject_overflow_and_keep_only_whole_supported_pages`, `production_initialization_bootstrap_contiguity_and_accounting` | See source and subsystem guide |
| [`src/mm/rss.rs`](../src/mm/rss.rs) | 60 | `rss_charge`, `rss_discharge`, `rss_reset`, `rss_pages` | See source and subsystem guide |
| [`src/mm/slab.rs`](../src/mm/slab.rs) | 448 | `SlabHdr`, `Cache`, `CacheStats`, `SlabStats`, `SlabBox`, … | See source and subsystem guide |
| [`src/mm/swap.rs`](../src/mm/swap.rs) | 808 | `SwapSlot`, `SwapOps`, `SwapDevice`, `DevTable`, `LruEntry`, … | See source and subsystem guide |
| [`src/mm/vm_rw.rs`](../src/mm/vm_rw.rs) | 156 | `UserIovec`, `read_iovecs`, `current_only`, `copy_between_iovecs`, `sys_process_vm_readv`, … | See source and subsystem guide |
| [`src/mm/vma.rs`](../src/mm/vma.rs) | 48 | `MappingFlags`, `Output`, `empty`, `bits`, `contains`, … | See source and subsystem guide |
| [`src/mm/vmm_fault.rs`](../src/mm/vmm_fault.rs) | 49 | `check_map_page`, `check_alloc_region`, `check_cow_fault` | No direct declaration/include found; Cfg-name mentions: `fault-inject` |

## Networking

This group contains 28 tracked source files. Read [the subsystem guide](networking.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/net/arp.rs`](../src/net/arp.rs) | 256 | `ArpEntry`, `PendingPkt`, `PendingQueue`, `lookup`, `insert`, … | See source and subsystem guide |
| [`src/net/dhcp.rs`](../src/net/dhcp.rs) | 339 | `Lease`, `InboundMsg`, `leased_ip`, `leased_gateway`, `leased_mask`, … | See source and subsystem guide |
| [`src/net/dns.rs`](../src/net/dns.rs) | 424 | `DnsEntry`, `DnsCache`, `PendingReply`, `PartialEntry`, `ParseResult`, … | See source and subsystem guide |
| [`src/net/eth.rs`](../src/net/eth.rs) | 128 | `MacAddr`, `from_bytes`, `as_bytes`, `is_broadcast`, `is_multicast`, … | See source and subsystem guide |
| [`src/net/icmp.rs`](../src/net/icmp.rs) | 51 | `receive`, `echo_reply`, `port_unreachable` | See source and subsystem guide |
| [`src/net/icmpv6.rs`](../src/net/icmpv6.rs) | 354 | `NdpEntry`, `ndp_resolve`, `ndp_learn`, `checksum`, `make_icmpv6`, … | See source and subsystem guide |
| [`src/net/ip.rs`](../src/net/ip.rs) | 159 | `our_ip`, `set_ip`, `set_gw`, `set_mask`, `get_ip`, … | See source and subsystem guide |
| [`src/net/ipv6.rs`](../src/net/ipv6.rs) | 383 | `Addr6`, `FragBuf`, `solicited_node`, `is_multicast`, `is_link_local`, … | See source and subsystem guide |
| [`src/net/mod.rs`](../src/net/mod.rs) | 224 | `NetHandle`, `NetScheme`, `alloc_handle`, `fid_index`, `fmt_ipv4`, … | See source and subsystem guide |
| [`src/net/socket.rs`](../src/net/socket.rs) | 389 | `SocketDomain`, `SocketType`, `SocketProtocol`, `SocketAddrIn`, `SocketError`, … | Ambiguous with socket/mod.rs |
| [`src/net/socket/address.rs`](../src/net/socket/address.rs) | 60 | `next_ephemeral`, `read_sockaddr_in`, `read_sockaddr_in6`, `write_sockaddr_in`, `write_sockaddr_in6` | See source and subsystem guide |
| [`src/net/socket/buffer.rs`](../src/net/socket/buffer.rs) | 61 | `UnixPipe`, `UnixConn`, `PendingUnix`, `UnixListener`, `new_pair`, … | See source and subsystem guide |
| [`src/net/socket/core.rs`](../src/net/socket/core.rs) | 285 | `new_socket`, `alloc_slot`, `sys_socket`, `sys_bind`, `sys_listen`, … | See source and subsystem guide |
| [`src/net/socket/mod.rs`](../src/net/socket/mod.rs) | 44 | No named Rust items extracted | Ambiguous with socket.rs |
| [`src/net/socket/poll.rs`](../src/net/socket/poll.rs) | 94 | `is_socket_fd`, `socket_poll`, `socket_read`, `socket_write` | See source and subsystem guide |
| [`src/net/socket/syscalls.rs`](../src/net/socket/syscalls.rs) | 135 | `sys_send`, `sys_recv`, `sys_sendmsg`, `sys_recvmsg`, `sys_socketpair`, … | See source and subsystem guide |
| [`src/net/socket/tcp/connect.rs`](../src/net/socket/tcp/connect.rs) | 25 | `tcp_connect` | See source and subsystem guide |
| [`src/net/socket/tcp/listen.rs`](../src/net/socket/tcp/listen.rs) | 12 | `tcp_listen` | See source and subsystem guide |
| [`src/net/socket/tcp/mod.rs`](../src/net/socket/tcp/mod.rs) | 7 | No named Rust items extracted | See source and subsystem guide |
| [`src/net/socket/tcp/state.rs`](../src/net/socket/tcp/state.rs) | 10 | `is_connected`, `is_listening` | See source and subsystem guide |
| [`src/net/socket/traits.rs`](../src/net/socket/traits.rs) | 116 | `sys_setsockopt`, `sys_getsockopt`, `sys_shutdown` | See source and subsystem guide |
| [`src/net/socket/types.rs`](../src/net/socket/types.rs) | 77 | `SockAddr`, `SocketState`, `Socket` | See source and subsystem guide |
| [`src/net/socket/udp/mod.rs`](../src/net/socket/udp/mod.rs) | 145 | `demux_udp`, `sys_sendto`, `sys_recvfrom`, `sys_sendto_dhcp`, `inject_dhcp_reply` | See source and subsystem guide |
| [`src/net/socket/unix/mod.rs`](../src/net/socket/unix/mod.rs) | 4 | No named Rust items extracted | See source and subsystem guide |
| [`src/net/socket/unix/stream.rs`](../src/net/socket/unix/stream.rs) | 97 | `unix_bind`, `unix_listen`, `unix_accept`, `unix_connect` | See source and subsystem guide |
| [`src/net/tcp.rs`](../src/net/tcp.rs) | 857 | `TcpState`, `TcpConn`, `TcpScheme`, `new`, `alloc_tcp_slot`, … | See source and subsystem guide |
| [`src/net/tcp/state_machine.rs`](../src/net/tcp/state_machine.rs) | 385 | `TcpState`, `TcpControlBlock`, `TcpFlags`, `TcpAction`, `TcpError`, … | No direct declaration/include found |
| [`src/net/udp.rs`](../src/net/udp.rs) | 423 | `PortEntry`, `UdpScheme`, `register_port`, `unregister_port`, `register_ephemeral`, … | See source and subsystem guide |

## Unregistered generic power management

This group contains 1 tracked source file. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/power/mod.rs`](../src/power/mod.rs) | 391 | `PowerState`, `CState`, `AcpiRegisters`, `WakeReason`, `CpufreqGovernor`, … | No direct declaration/include found |

## Processes and scheduler

This group contains 37 tracked source files. Read [the subsystem guide](processes-and-scheduling.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/proc/cgroup.rs`](../src/proc/cgroup.rs) | 570 | `CgroupId`, `CgroupLimits`, `CgroupStat`, `CgroupNode`, `CgroupTable`, … | See source and subsystem guide |
| [`src/proc/clone.rs`](../src/proc/clone.rs) | 1 | No named Rust items extracted | See source and subsystem guide |
| [`src/proc/context.rs`](../src/proc/context.rs) | 115 | `Context`, `UserContext`, `zero`, `restore`, `switch` | See source and subsystem guide |
| [`src/proc/cow_fault.rs`](../src/proc/cow_fault.rs) | 125 | `clone_for_fork`, `handle_cow_fault`, `pte_read` | See source and subsystem guide |
| [`src/proc/creds.rs`](../src/proc/creds.rs) | 409 | `current_pid`, `sys_getuid`, `sys_geteuid`, `sys_getgid`, `sys_getegid`, … | See source and subsystem guide |
| [`src/proc/cwd.rs`](../src/proc/cwd.rs) | 62 | `get_cwd`, `set_cwd`, `resolve`, `canonicalise` | See source and subsystem guide |
| [`src/proc/dynlink.rs`](../src/proc/dynlink.rs) | 277 | `DynExecInfo`, `find_interp`, `load_interp`, `build_auxv`, `map_elf_phdrs`, … | See source and subsystem guide |
| [`src/proc/exec.rs`](../src/proc/exec.rs) | 732 | `ExecPage`, `PreparedImage`, `DeferredImage`, `read_cstr_bytes_safe`, `read_cstr_safe`, … | See source and subsystem guide |
| [`src/proc/exit.rs`](../src/proc/exit.rs) | 161 | `RetiredTask`, `retired_task_count`, `clear_child_tid`, `is_last_live_thread`, `reparent_orphans`, … | See source and subsystem guide |
| [`src/proc/fault_signal.rs`](../src/proc/fault_signal.rs) | 103 | `on_page_fault`, `on_invalid_opcode`, `on_divide_error`, `on_fp_exception`, `on_alignment_check`, … | No direct declaration/include found |
| [`src/proc/fork_syscall.rs`](../src/proc/fork_syscall.rs) | 143 | `clone_user_mappings`, `rollback_user_mappings`, `sys_fork` | See source and subsystem guide |
| [`src/proc/fork.rs`](../src/proc/fork.rs) | 56 | `SignalHandlers`, `default`, `exec_reset` | See source and subsystem guide |
| [`src/proc/futex.rs`](../src/proc/futex.rs) | 848 | `FutexKey`, `Waiter`, `FutexBucket`, `PiRecord`, `new`, … | See source and subsystem guide |
| [`src/proc/ipc.rs`](../src/proc/ipc.rs) | 687 | `IpcKey`, `MsgBuf`, `MsgQueue`, `MsgTable`, `SemSet`, … | See source and subsystem guide |
| [`src/proc/itimer.rs`](../src/proc/itimer.rs) | 336 | `AlarmState`, `read_itimerval`, `write_itimerval`, `remaining_real_ns`, `sys_alarm`, … | See source and subsystem guide |
| [`src/proc/mod.rs`](../src/proc/mod.rs) | 128 | `set_pgid`, `get_pgid`, `setsid`, `get_sid`, `current_creds`, … | See source and subsystem guide |
| [`src/proc/namespace.rs`](../src/proc/namespace.rs) | 454 | `NsId`, `NsSet`, `MountEntry`, `MountNsTable`, `alloc_ns_id`, … | See source and subsystem guide |
| [`src/proc/nanosleep.rs`](../src/proc/nanosleep.rs) | 239 | `now_ns`, `sys_nanosleep`, `sys_clock_nanosleep`, `sys_clock_gettime`, `sys_clock_getres`, … | See source and subsystem guide |
| [`src/proc/net_ns.rs`](../src/proc/net_ns.rs) | 207 | `NetIface`, `NetNs`, `NetNsTable`, `loopback`, `new_with_loopback`, … | See source and subsystem guide |
| [`src/proc/pid_ns.rs`](../src/proc/pid_ns.rs) | 304 | `PidNsEntry`, `new`, `alloc`, `local_of`, `global_of`, … | See source and subsystem guide |
| [`src/proc/pid.rs`](../src/proc/pid.rs) | 25 | `alloc_pid`, `free_pid` | See source and subsystem guide |
| [`src/proc/proc_table.rs`](../src/proc/proc_table.rs) | 92 | `next_pid`, `proc_count`, `find_proc_lock`, `with_proc`, `with_proc_mut`, … | See source and subsystem guide |
| [`src/proc/process_builder.rs`](../src/proc/process_builder.rs) | 610 | `ProcessBuilder`, `Credentials`, `Namespaces`, `ResourceLimit`, `ProcessConfig`, … | See source and subsystem guide |
| [`src/proc/process.rs`](../src/proc/process.rs) | 348 | `State`, `ProcLock`, `Pcb`, `Process`, `to_u8`, … | See source and subsystem guide |
| [`src/proc/ptrace.rs`](../src/proc/ptrace.rs) | 220 | `PtraceState`, `default`, `frame_ptr`, `build_user_regs_pub`, `apply_user_regs_pub`, … | See source and subsystem guide |
| [`src/proc/restart.rs`](../src/proc/restart.rs) | 70 | `RestartBlock`, `set_restart`, `take_restart`, `get_restart`, `clear_restart` | See source and subsystem guide |
| [`src/proc/rlimit.rs`](../src/proc/rlimit.rs) | 165 | `RlimitSet`, `default`, `get`, `set`, `exceeds_nofile`, … | See source and subsystem guide |
| [`src/proc/rusage.rs`](../src/proc/rusage.rs) | 71 | `sys_getrusage` | See source and subsystem guide |
| [`src/proc/sched_helpers.rs`](../src/proc/sched_helpers.rs) | 215 | `CbsUtilBucket`, `new`, `scaled_util`, `cbs_admit`, `cbs_release`, … | See source and subsystem guide |
| [`src/proc/scheduler.rs`](../src/proc/scheduler.rs) | 1182 | `SchedPolicy`, `SchedEntity`, `CfsEntry`, `RtEntry`, `DlEntry`, … | See source and subsystem guide |
| [`src/proc/signal.rs`](../src/proc/signal.rs) | 1170 | `SigInfo`, `AltStack`, `SigAction`, `SigFrameX86`, `SigFrameAarch64`, … | See source and subsystem guide |
| [`src/proc/task_group.rs`](../src/proc/task_group.rs) | 379 | `GroupRq`, `TaskGroup`, `GroupDequeueResult`, `new`, `enqueue`, … | See source and subsystem guide |
| [`src/proc/task_types.rs`](../src/proc/task_types.rs) | 100 | `TaskRunState`, `Task`, `default`, `new`, `mark_live`, … | See source and subsystem guide |
| [`src/proc/thread.rs`](../src/proc/thread.rs) | 129 | `register_thread`, `unregister_thread`, `tgid_of`, `threads_of`, `vma_pid`, … | See source and subsystem guide |
| [`src/proc/time_ns.rs`](../src/proc/time_ns.rs) | 377 | `read_timespec`, `write_timespec`, `mono_ns_for_current`, `cpu_ns`, `sys_clock_gettime`, … | See source and subsystem guide |
| [`src/proc/user_ns.rs`](../src/proc/user_ns.rs) | 438 | `IdMapEntry`, `UserNsData`, `UserNsTable`, `MapWriteError`, `MapKind`, … | See source and subsystem guide |
| [`src/proc/wait.rs`](../src/proc/wait.rs) | 165 | `Child`, `SigInfoWait`, `encode_exit`, `notify_exit`, `children_of`, … | See source and subsystem guide |

## Security

This group contains 24 tracked source files. Read [the subsystem guide](security.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/security/aslr.rs`](../src/security/aslr.rs) | 132 | `AslrLayout`, `aslr_stack_offset`, `aslr_heap_base`, `aslr_mmap_base`, `aslr_vdso_base`, … | See source and subsystem guide |
| [`src/security/bpf.rs`](../src/security/bpf.rs) | 302 | `MapCreateAttr`, `ElemAttr`, `BpfMap`, `copy_attr`, `copy_user_bytes`, … | See source and subsystem guide |
| [`src/security/canary.rs`](../src/security/canary.rs) | 96 | `CanaryExpected`, `init_kernel_canary`, `new_task_canary`, `install_canary_in_tls`, `__stack_chk_fail`, … | See source and subsystem guide |
| [`src/security/capabilities.rs`](../src/security/capabilities.rs) | 400 | `CapabilitySet`, `Capability`, `CapabilityError`, `empty`, `full`, … | No direct declaration/include found |
| [`src/security/capset.rs`](../src/security/capset.rs) | 168 | `CapSet`, `all`, `empty`, `full`, `has`, … | See source and subsystem guide |
| [`src/security/cgroups/cpu.rs`](../src/security/cgroups/cpu.rs) | 88 | `CpuCg`, `default`, `read`, `write`, `throttle`, … | See source and subsystem guide |
| [`src/security/cgroups/memory.rs`](../src/security/cgroups/memory.rs) | 90 | `MemCg`, `default`, `read`, `write`, `charge`, … | See source and subsystem guide |
| [`src/security/cgroups/mod.rs`](../src/security/cgroups/mod.rs) | 299 | `CgroupId`, `Cgroup`, `CgroupTree`, `alloc_cg_id`, `new_root`, … | See source and subsystem guide |
| [`src/security/cgroups/pids.rs`](../src/security/cgroups/pids.rs) | 60 | `PidsCg`, `default`, `read`, `write`, `can_fork`, … | See source and subsystem guide |
| [`src/security/dac.rs`](../src/security/dac.rs) | 303 | `DacModule`, `has_cap`, `in_group`, `check_read`, `check_write`, … | See source and subsystem guide |
| [`src/security/landlock.rs`](../src/security/landlock.rs) | 254 | `RulesetAttr`, `PathBeneathAttr`, `Ruleset`, `Domain`, `LandlockLsm`, … | See source and subsystem guide |
| [`src/security/lsm.rs`](../src/security/lsm.rs) | 327 | `LsmVerdict`, `Mode`, `LsmCtx`, `Hook`, `LsmHooks`, … | See source and subsystem guide |
| [`src/security/mod.rs`](../src/security/mod.rs) | 39 | `Cap` | Cfg-name mentions: `cgroups` |
| [`src/security/namespace/user.rs`](../src/security/namespace/user.rs) | 431 | `IdMapping`, `UserNamespace`, `Capability`, `NamespaceError`, `ProcessCredentials`, … | No direct declaration/include found |
| [`src/security/ns/isolation.rs`](../src/security/ns/isolation.rs) | 281 | `NamespaceType`, `IsolationStats`, `enable_isolation_enforcement`, `is_enforcement_active`, `record_violation`, … | See source and subsystem guide |
| [`src/security/ns/mnt_ns.rs`](../src/security/ns/mnt_ns.rs) | 164 | `MountEntry`, `MntNs`, `new_init`, `copy_of`, `mount`, … | See source and subsystem guide |
| [`src/security/ns/mod.rs`](../src/security/ns/mod.rs) | 113 | `NsSet`, `alloc_ns_id`, `init_ns`, `clone_with_flags`, `init` | See source and subsystem guide |
| [`src/security/ns/net_ns.rs`](../src/security/ns/net_ns.rs) | 208 | `NetIface`, `Route4`, `NetNs`, `loopback`, `new_init`, … | See source and subsystem guide |
| [`src/security/ns/pid_ns.rs`](../src/security/ns/pid_ns.rs) | 87 | `PidNs`, `new_init`, `new_child`, `alloc_pid`, `free_pid`, … | See source and subsystem guide |
| [`src/security/ns/user_ns.rs`](../src/security/ns/user_ns.rs) | 174 | `IdMapEntry`, `UserNs`, `new_init`, `new_child`, `write_uid_map`, … | See source and subsystem guide |
| [`src/security/ns/uts_ns.rs`](../src/security/ns/uts_ns.rs) | 102 | `Utsname`, `UtsNs`, `default_init`, `new_init`, `copy_of`, … | See source and subsystem guide |
| [`src/security/pti.rs`](../src/security/pti.rs) | 192 | `PtiCr3Pair`, `kernel_cr3_val`, `user_cr3_val`, `cpu_has_pcid`, `enable_pcide`, … | See source and subsystem guide |
| [`src/security/seccomp.rs`](../src/security/seccomp.rs) | 698 | `SeccompData`, `SockFilter`, `SeccompVerdict`, `SeccompFilter`, `FilterChain`, … | See source and subsystem guide |
| [`src/security/smep_smap.rs`](../src/security/smep_smap.rs) | 161 | `cpuid_cr4_features`, `cpuid_smep_smap`, `enforce`, `stac`, `clac`, … | See source and subsystem guide |

## Multiprocessing

This group contains 4 tracked source files. Read [the subsystem guide](synchronization.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/smp/hotplug.rs`](../src/smp/hotplug.rs) | 579 | `CpuState`, `StateCallback`, `CallbackId`, `HotplugError`, `HotplugResult`, … | See source and subsystem guide |
| [`src/smp/ipi.rs`](../src/smp/ipi.rs) | 226 | `IpiKind`, `ShootdownReq`, `set_pending`, `send`, `send_to`, … | See source and subsystem guide |
| [`src/smp/mod.rs`](../src/smp/mod.rs) | 171 | `CoreType`, `CpuInfo`, `register_cpu`, `register_cpu_typed`, `cpu_info`, … | See source and subsystem guide |
| [`src/smp/percpu.rs`](../src/smp/percpu.rs) | 198 | `PercpuBlock`, `zeroed`, `cpu_count`, `init`, `current_cpu_id`, … | See source and subsystem guide |

## Kernel synchronization

This group contains 9 tracked source files. Read [the subsystem guide](synchronization.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/sync/advanced.rs`](../src/sync/advanced.rs) | 552 | `TicketLock`, `TicketLockGuard`, `RwLock`, `RwLockReadGuard`, `Target`, … | No direct declaration/include found |
| [`src/sync/futex.rs`](../src/sync/futex.rs) | 217 | `FutexWaiter`, `futex_wait`, `futex_wake`, `futex_requeue`, `futex_wake_op`, … | See source and subsystem guide |
| [`src/sync/mod.rs`](../src/sync/mod.rs) | 29 | No named Rust items extracted | See source and subsystem guide |
| [`src/sync/mutex.rs`](../src/sync/mutex.rs) | 1 | No named Rust items extracted | See source and subsystem guide |
| [`src/sync/poll_source.rs`](../src/sync/poll_source.rs) | 141 | `PollSource`, `AlwaysReady`, `poll`, `wait_queue`, `wait_on`, … | See source and subsystem guide |
| [`src/sync/rwlock.rs`](../src/sync/rwlock.rs) | 1 | No named Rust items extracted | See source and subsystem guide |
| [`src/sync/semaphore.rs`](../src/sync/semaphore.rs) | 40 | `Semaphore`, `new`, `try_down`, `down`, `up` | See source and subsystem guide |
| [`src/sync/spinlock.rs`](../src/sync/spinlock.rs) | 285 | `SpinLock`, `SpinLockGuard`, `Target`, `IrqSpinLock`, `IrqSpinLockGuard`, … | See source and subsystem guide |
| [`src/sync/wait_queue.rs`](../src/sync/wait_queue.rs) | 258 | `ReadyMask`, `WakeReason`, `Waiter`, `Forwarder`, `WaitQueue`, … | See source and subsystem guide |

## System calls

This group contains 19 tracked source files. Read [the subsystem guide](syscalls.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/syscall/dispatcher_context.rs`](../src/syscall/dispatcher_context.rs) | 49 | `SyscallContext`, `new`, `a0`, `a1`, `a2`, … | See source and subsystem guide |
| [`src/syscall/driver.rs`](../src/syscall/driver.rs) | 330 | `DriverSysError`, `DriverLease`, `as_isize`, `current_pid`, `current_caps`, … | See source and subsystem guide |
| [`src/syscall/errno.rs`](../src/syscall/errno.rs) | 83 | `eperm`, `enoent`, `esrch`, `eio`, `eacces`, … | See source and subsystem guide |
| [`src/syscall/fault.rs`](../src/syscall/fault.rs) | 75 | `FaultErrno`, `fmt`, `check_resource`, `check_fdtable` | No direct declaration/include found; Cfg-name mentions: `fault-inject` |
| [`src/syscall/fs.rs`](../src/syscall/fs.rs) | 139 | `sys_open`, `sys_read`, `sys_write`, `sys_close`, `sys_seek`, … | No direct declaration/include found |
| [`src/syscall/kmtest.rs`](../src/syscall/kmtest.rs) | 91 | `sys_kmtest_list`, `sys_kmtest_run`, `run_range` | Cfg-name mentions: `kmtest` |
| [`src/syscall/mod.rs`](../src/syscall/mod.rs) | 708 | `SyscallAbi`, `stubs_at_path`, `arg_u32`, `arg_i32`, `sys_epoll_create1`, … | Cfg-name mentions: `debug`, `kmtest` |
| [`src/syscall/musl_compat.rs`](../src/syscall/musl_compat.rs) | 226 | `RobustListHead`, `Rlimit`, `sys_arch_prctl`, `sys_set_tid_address`, `sys_set_robust_list`, … | No direct declaration/include found; Cfg-name mentions: `boot_debug` |
| [`src/syscall/net.rs`](../src/syscall/net.rs) | 385 | `SocketFileLike`, `sockaddr_in`, `in_addr`, `sys_socket`, `sys_bind`, … | No direct declaration/include found |
| [`src/syscall/nr.rs`](../src/syscall/nr.rs) | 148 | No named Rust items extracted | See source and subsystem guide |
| [`src/syscall/openat2_mincore.rs`](../src/syscall/openat2_mincore.rs) | 83 | `sys_openat2_impl`, `sys_mincore` | Textually included |
| [`src/syscall/p0_gaps.rs`](../src/syscall/p0_gaps.rs) | 27 | `sys_sched_setparam_impl`, `sys_sched_getscheduler_impl`, `sys_sched_getparam_impl`, `sys_sched_setscheduler_impl` | Textually included |
| [`src/syscall/posix_full.rs`](../src/syscall/posix_full.rs) | 331 | `copy_bytes_to_user`, `read_user_str`, `sys_getrlimit_impl`, `sys_setrlimit_impl`, `sys_prlimit64_impl`, … | Textually included |
| [`src/syscall/profile.rs`](../src/syscall/profile.rs) | 236 | `SyscallStat`, `Sink`, `new`, `read_cycles`, `syscall_profile_enter`, … | 512-entry optional syscall counter table; Cfg-name mentions: `syscall-trace`, `trace` |
| [`src/syscall/routers.rs`](../src/syscall/routers.rs) | 627 | `dispatch_filesystem`, `read_sigaction_from_user`, `write_sigaction_to_user`, `dispatch_process`, `dispatch_memory`, … | Cfg-name mentions: `kmtest` |
| [`src/syscall/sched.rs`](../src/syscall/sched.rs) | 533 | `SchedParam`, `SchedAttr`, `DeadlineSnapshot`, `read_sched_param`, `write_sched_param`, … | No direct declaration/include found |
| [`src/syscall/scheme.rs`](../src/syscall/scheme.rs) | 145 | `SchemeSysError`, `SchemeOwner`, `as_isize`, `current_pid`, `current_caps`, … | See source and subsystem guide |
| [`src/syscall/signal_nr.rs`](../src/syscall/signal_nr.rs) | 41 | No named Rust items extracted | See source and subsystem guide |
| [`src/syscall/stubs.rs`](../src/syscall/stubs.rs) | 781 | `resolve_at_path_for_stubs`, `copy_to_user`, `copy_from_user`, `sys_getcwd_impl`, `sys_chdir_impl`, … | Textually included |

## Time and timers

This group contains 6 tracked source files. Read [the subsystem guide](firmware-and-platforms.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/time/clock.rs`](../src/time/clock.rs) | 165 | `clock_gettime`, `clock_settime`, `clock_getres`, `get_process_cputime`, `get_thread_cputime`, … | See source and subsystem guide |
| [`src/time/hpet.rs`](../src/time/hpet.rs) | 109 | `init`, `is_ready`, `read_ns`, `set_oneshot`, `acpi_hpet_base`, … | See source and subsystem guide |
| [`src/time/mod.rs`](../src/time/mod.rs) | 235 | `ClockSource`, `Timespec`, `Timeval`, `set_clocksource`, `clocksource`, … | See source and subsystem guide |
| [`src/time/timer.rs`](../src/time/timer.rs) | 249 | `TimerCallback`, `TimerEntry`, `Wheel`, `ItimerVal`, `ItimerState`, … | See source and subsystem guide |
| [`src/time/timerfd.rs`](../src/time/timerfd.rs) | 214 | `TimerFdId`, `TimerFd`, `ItimerSpec`, `new`, `settime`, … | See source and subsystem guide |
| [`src/time/tsc.rs`](../src/time/tsc.rs) | 218 | `calibrate`, `read_ns`, `freq_hz`, `rdtsc`, `is_invariant`, … | See source and subsystem guide |

## Terminals

This group contains 7 tracked source files. Read [the subsystem guide](drivers-and-devices.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`src/tty/ldisc.rs`](../src/tty/ldisc.rs) | 106 | `LdiscAction`, `OutputBytes`, `process_input`, `process_output` | See source and subsystem guide |
| [`src/tty/mod.rs`](../src/tty/mod.rs) | 299 | `PtyRegistry`, `ConsoleOutput`, `SerialConsole`, `TtySide`, `TtyHandle`, … | See source and subsystem guide |
| [`src/tty/pts_fs.rs`](../src/tty/pts_fs.rs) | 131 | `PtsMasterData`, `PtsFd`, `init`, `register_slave`, `unregister_slave`, … | See source and subsystem guide |
| [`src/tty/pty.rs`](../src/tty/pty.rs) | 446 | `PtyInner`, `PtyPair`, `new`, `get_termios`, `set_termios`, … | See source and subsystem guide |
| [`src/tty/serial_ldisc.rs`](../src/tty/serial_ldisc.rs) | 304 | `Termios`, `Winsize`, `TtyState`, `serial_write`, `serial_read`, … | No direct declaration/include found |
| [`src/tty/serial.rs`](../src/tty/serial.rs) | 306 | `Termios`, `Winsize`, `TtyState`, `serial_write`, `serial_read`, … | See source and subsystem guide |
| [`src/tty/termios.rs`](../src/tty/termios.rs) | 199 | `Termios`, `Winsize`, `cooked_default`, `raw_default`, `is_canonical`, … | See source and subsystem guide |

## Portable workspace crates

This group contains 9 tracked source files. Read [the subsystem guide](architecture.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`crates/fs-ext4/src/lib.rs`](../crates/fs-ext4/src/lib.rs) | 454 | `Ext4FileSystem`, `Ext4FileOps`, `default`, `new`, `with_mount_point`, … | See source and subsystem guide |
| [`crates/fs-fat32/src/lib.rs`](../crates/fs-fat32/src/lib.rs) | 431 | `Fat32FileSystem`, `Fat32FileOps`, `default`, `new`, `with_mount_point`, … | See source and subsystem guide |
| [`crates/fs-tmpfs/src/lib.rs`](../crates/fs-tmpfs/src/lib.rs) | 80 | `TmpfsFileSystem`, `default`, `new`, `name`, `open`, … | See source and subsystem guide |
| [`crates/load-balancer/src/lib.rs`](../crates/load-balancer/src/lib.rs) | 590 | `LoadBalancerError`, `LoadBalancerResult`, `WorkFlags`, `WorkItem`, `CpuWorkQueue`, … | See source and subsystem guide |
| [`crates/mm-core/src/lib.rs`](../crates/mm-core/src/lib.rs) | 257 | `MmError`, `Pfn`, `Vpn`, `PageFlags`, `ProtFlags`, … | See source and subsystem guide |
| [`crates/scheme-api/src/lib.rs`](../crates/scheme-api/src/lib.rs) | 210 | `SchemeFileId`, `IpcEndpoint`, `DriverHandle`, `OpenFlags`, `SeekWhence`, … | Cfg-name mentions: `std` |
| [`crates/smp-core/src/lib.rs`](../crates/smp-core/src/lib.rs) | 590 | `CpuFlags`, `CpuError`, `CpuResult`, `CpuData`, `CpuRegistry`, … | See source and subsystem guide |
| [`crates/sync-primitives/src/lib.rs`](../crates/sync-primitives/src/lib.rs) | 632 | `QspinlockNode`, `Qspinlock`, `RcuReadGuard`, `RcuProtected`, `TicketSpinlock`, … | See source and subsystem guide |
| [`crates/vfs-core/src/lib.rs`](../crates/vfs-core/src/lib.rs) | 318 | `VfsError`, `Stat`, `OpenFlags`, `FileHandle`, `DirEntry`, … | See source and subsystem guide |

## Freestanding userspace

This group contains 11 tracked source files. Read [the subsystem guide](userspace.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`userspace/bench/build.rs`](../userspace/bench/build.rs) | 14 | `main` | See source and subsystem guide |
| [`userspace/bench/src/main.rs`](../userspace/bench/src/main.rs) | 405 | `Timespec`, `sys_write`, `sys_getpid`, `sys_clock_gettime`, `write_all`, … | See source and subsystem guide |
| [`userspace/drivers/virtio_net/main.rs`](../userspace/drivers/virtio_net/main.rs) | 512 | `VirtqDesc`, `VirtqAvail`, `VirtqUsedElem`, `VirtqUsed`, `NetDriver`, … | See source and subsystem guide |
| [`userspace/hello/src/main.rs`](../userspace/hello/src/main.rs) | 183 | `sys_write`, `sys_exit`, `write_all`, `hello_main`, `_start`, … | See source and subsystem guide |
| [`userspace/init/src/main.rs`](../userspace/init/src/main.rs) | 713 | `sys_write`, `sys_read`, `sys_open`, `sys_close`, `sys_brk`, … | See source and subsystem guide |
| [`userspace/kmtest/src/main.rs`](../userspace/kmtest/src/main.rs) | 395 | `sys_kmtest_list`, `sys_kmtest_run`, `sys_exit_group`, `sys_write`, `write_all`, … | See source and subsystem guide |
| [`userspace/libc-shim/src/lib.rs`](../userspace/libc-shim/src/lib.rs) | 354 | `shim_syscall`, `write`, `read`, `open`, `close`, … | Cfg-name mentions: `provide_start` |
| [`userspace/raw-syscall/src/lib.rs`](../userspace/raw-syscall/src/lib.rs) | 12 | No named Rust items extracted | See source and subsystem guide |
| [`userspace/smoke/src/checks.rs`](../userspace/smoke/src/checks.rs) | 167 | `run_tests` | See source and subsystem guide |
| [`userspace/smoke/src/main.rs`](../userspace/smoke/src/main.rs) | 619 | `sys_write`, `sys_open`, `sys_lseek`, `sys_unlink`, `sys_rename`, … | See source and subsystem guide |
| [`userspace/smoke/tests/assertions.rs`](../userspace/smoke/tests/assertions.rs) | 354 | `Mock`, `default`, `run_with`, `check`, `test_errno_checks`, … | All 11 direct host tests fail in audit |

## Parser fuzz harnesses

This group contains 4 tracked source files. Read [the subsystem guide](code_coverage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`fuzz/fuzz_targets/cpio_newc.rs`](../fuzz/fuzz_targets/cpio_newc.rs) | 27 | No named Rust items extracted | See source and subsystem guide |
| [`fuzz/fuzz_targets/elf64.rs`](../fuzz/fuzz_targets/elf64.rs) | 19 | No named Rust items extracted | See source and subsystem guide |
| [`fuzz/fuzz_targets/ext4_metadata.rs`](../fuzz/fuzz_targets/ext4_metadata.rs) | 32 | `is_present`, `read_sectors` | See source and subsystem guide |
| [`fuzz/fuzz_targets/vfs_path.rs`](../fuzz/fuzz_targets/vfs_path.rs) | 46 | `FuzzFs`, `exists`, `is_dir`, `is_symlink`, `readlink_raw` | See source and subsystem guide |

## Root integration tests

This group contains 2 tracked source files. Read [the subsystem guide](code_coverage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`tests/host/exit_lifecycle.rs`](../tests/host/exit_lifecycle.rs) | 244 | `Mutex`, `Arch`, `Scheduled`, `State`, `Sched`, … | See source and subsystem guide |
| [`tests/host/pmm_init.rs`](../tests/host/pmm_init.rs) | 25 | `Mutex`, `Once`, `InterruptGuard`, `new`, `lock`, … | See source and subsystem guide |

## Direct Rust host harness

This group contains 1 tracked source file. Read [the subsystem guide](code_coverage.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`scripts/ci/tests/fault_engine.rs`](../scripts/ci/tests/fault_engine.rs) | 101 | `unarmed_point_never_fires`, `immediate_failure_is_one_shot`, `delayed_failure_respects_success_budget`, `explicit_disarm_prevents_failure`, `persistent_failure_continues_until_disarmed`, … | See source and subsystem guide |

## Build automation

This group contains 1 tracked source file. Read [the subsystem guide](build-reference.md) for implementation boundaries and validation scope.

| Source file | Lines | Representative declarations | Structural notes |
| --- | --- | --- | --- |
| [`xtask/src/main.rs`](../xtask/src/main.rs) | 2089 | `Arch`, `Boot`, `BuildProfile`, `BuildOpts`, `GptHeader`, … | See source and subsystem guide |

## Build, workflow, and validation inputs

These 89 additional tracked files influence configuration, build/runtime orchestration, and evidence. They are not included in the 510-file Rust/C/assembly count above.

| File | Role |
| --- | --- |
| [`.cargo/config.toml`](../.cargo/config.toml) | Build/toolchain/workspace configuration |
| [`.github/workflows/allocator-repair.yml`](../.github/workflows/allocator-repair.yml) | CI workflow |
| [`.github/workflows/boot-perf.yml`](../.github/workflows/boot-perf.yml) | CI workflow |
| [`.github/workflows/boot-smoke.yml`](../.github/workflows/boot-smoke.yml) | CI workflow |
| [`.github/workflows/build.yml`](../.github/workflows/build.yml) | CI workflow |
| [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | CI workflow |
| [`.github/workflows/exit-lifecycle.yml`](../.github/workflows/exit-lifecycle.yml) | CI workflow |
| [`.github/workflows/fault-inject.yml`](../.github/workflows/fault-inject.yml) | CI workflow |
| [`.github/workflows/fmt.yml`](../.github/workflows/fmt.yml) | CI workflow |
| [`.github/workflows/focused-regressions.yml`](../.github/workflows/focused-regressions.yml) | CI workflow |
| [`.github/workflows/full-kernel-smoke.yml`](../.github/workflows/full-kernel-smoke.yml) | CI workflow |
| [`.github/workflows/image-size.yml`](../.github/workflows/image-size.yml) | CI workflow |
| [`.github/workflows/kernel-test.yml`](../.github/workflows/kernel-test.yml) | CI workflow |
| [`.github/workflows/kmtest.yml`](../.github/workflows/kmtest.yml) | CI workflow |
| [`.github/workflows/kstack-ownership.yml`](../.github/workflows/kstack-ownership.yml) | CI workflow |
| [`.github/workflows/labeler.yml`](../.github/workflows/labeler.yml) | CI workflow |
| [`.github/workflows/lint.yml`](../.github/workflows/lint.yml) | CI workflow |
| [`.github/workflows/panic-format.yml`](../.github/workflows/panic-format.yml) | CI workflow |
| [`.github/workflows/parser-fuzz.yml`](../.github/workflows/parser-fuzz.yml) | CI workflow |
| [`.github/workflows/pmm-init.yml`](../.github/workflows/pmm-init.yml) | CI workflow |
| [`.github/workflows/qemu-smoke.yml`](../.github/workflows/qemu-smoke.yml) | CI workflow |
| [`.github/workflows/regression-tests.yml`](../.github/workflows/regression-tests.yml) | CI workflow |
| [`.github/workflows/stale.yml`](../.github/workflows/stale.yml) | CI workflow |
| [`.github/workflows/status-check.yml`](../.github/workflows/status-check.yml) | CI workflow |
| [`.github/workflows/task3-smoke-markers.yml`](../.github/workflows/task3-smoke-markers.yml) | CI workflow |
| [`.github/workflows/task4-fault-sites.yml`](../.github/workflows/task4-fault-sites.yml) | CI workflow |
| [`.github/workflows/workflow-lint.yml`](../.github/workflows/workflow-lint.yml) | CI workflow |
| [`Cargo.lock`](../Cargo.lock) | Dependency lockfile |
| [`Cargo.toml`](../Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/fs-ext4/Cargo.toml`](../crates/fs-ext4/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/fs-fat32/Cargo.toml`](../crates/fs-fat32/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/fs-tmpfs/Cargo.toml`](../crates/fs-tmpfs/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/load-balancer/Cargo.toml`](../crates/load-balancer/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/mm-core/Cargo.toml`](../crates/mm-core/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/scheme-api/Cargo.toml`](../crates/scheme-api/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/smp-core/Cargo.toml`](../crates/smp-core/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/sync-primitives/Cargo.toml`](../crates/sync-primitives/Cargo.toml) | Build/toolchain/workspace configuration |
| [`crates/vfs-core/Cargo.toml`](../crates/vfs-core/Cargo.toml) | Build/toolchain/workspace configuration |
| [`fuzz/Cargo.toml`](../fuzz/Cargo.toml) | Build/toolchain/workspace configuration |
| [`linker/aarch64.ld`](../linker/aarch64.ld) | Target or linker configuration |
| [`linker/riscv64.ld`](../linker/riscv64.ld) | Target or linker configuration |
| [`linker/x86_64.ld`](../linker/x86_64.ld) | Target or linker configuration |
| [`rust-toolchain.toml`](../rust-toolchain.toml) | Build/toolchain/workspace configuration |
| [`scripts/audit_unwrap.sh`](../scripts/audit_unwrap.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/apply-task3-smoke-markers.py`](../scripts/ci/apply-task3-smoke-markers.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/apply-task4-fault-sites.py`](../scripts/ci/apply-task4-fault-sites.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/boot-regression.sh`](../scripts/ci/boot-regression.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/build-userspace.sh`](../scripts/ci/build-userspace.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/build.sh`](../scripts/ci/build.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/check-rust-format.sh`](../scripts/ci/check-rust-format.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/check-stubs.sh`](../scripts/ci/check-stubs.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/fuzz-coverage.sh`](../scripts/ci/fuzz-coverage.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/install-musl-toolchain.sh`](../scripts/ci/install-musl-toolchain.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/panic_output.py`](../scripts/ci/panic_output.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/parse-boot-marks.py`](../scripts/ci/parse-boot-marks.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/parse-boot-marks.sh`](../scripts/ci/parse-boot-marks.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/parse_smoke_log.py`](../scripts/ci/parse_smoke_log.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/run_qemu.sh`](../scripts/ci/run_qemu.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/test-boot-marks.py`](../scripts/ci/test-boot-marks.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/test-exit-lifecycle.sh`](../scripts/ci/test-exit-lifecycle.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/test-kstack-ownership.sh`](../scripts/ci/test-kstack-ownership.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/test-pmm-init.sh`](../scripts/ci/test-pmm-init.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/test-region-heap.sh`](../scripts/ci/test-region-heap.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/tests/test_fuzz_coverage.py`](../scripts/ci/tests/test_fuzz_coverage.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/tests/test_panic_output.py`](../scripts/ci/tests/test_panic_output.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/tests/test_parse_smoke_log.py`](../scripts/ci/tests/test_parse_smoke_log.py) | Build, test, or validation helper; inspect before running |
| [`scripts/ci/verify-production-requirements.sh`](../scripts/ci/verify-production-requirements.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/pack-initramfs.sh`](../scripts/pack-initramfs.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/run_regression_tests.sh`](../scripts/run_regression_tests.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/stability_test.sh`](../scripts/stability_test.sh) | Build, test, or validation helper; inspect before running |
| [`scripts/validate-config.sh`](../scripts/validate-config.sh) | Build, test, or validation helper; inspect before running |
| [`targets/aarch64-kernel.json`](../targets/aarch64-kernel.json) | Target or linker configuration |
| [`targets/aarch64-uefi-loader.json`](../targets/aarch64-uefi-loader.json) | Target or linker configuration |
| [`targets/riscv64-uefi-loader.json`](../targets/riscv64-uefi-loader.json) | Target or linker configuration |
| [`targets/x86_64-kernel.json`](../targets/x86_64-kernel.json) | Target or linker configuration |
| [`userspace/.cargo/config.toml`](../userspace/.cargo/config.toml) | Build/toolchain/workspace configuration |
| [`userspace/Cargo.toml`](../userspace/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/bench/Cargo.toml`](../userspace/bench/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/drivers/virtio_net/Cargo.toml`](../userspace/drivers/virtio_net/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/hello/Cargo.toml`](../userspace/hello/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/init/.cargo/config.toml`](../userspace/init/.cargo/config.toml) | Build/toolchain/workspace configuration |
| [`userspace/init/Cargo.lock`](../userspace/init/Cargo.lock) | Dependency lockfile |
| [`userspace/init/Cargo.toml`](../userspace/init/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/init/linker.ld`](../userspace/init/linker.ld) | Target or linker configuration |
| [`userspace/kmtest/Cargo.toml`](../userspace/kmtest/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/libc-shim/Cargo.toml`](../userspace/libc-shim/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/raw-syscall/Cargo.toml`](../userspace/raw-syscall/Cargo.toml) | Build/toolchain/workspace configuration |
| [`userspace/smoke/Cargo.toml`](../userspace/smoke/Cargo.toml) | Build/toolchain/workspace configuration |
| [`xtask/Cargo.toml`](../xtask/Cargo.toml) | Build/toolchain/workspace configuration |

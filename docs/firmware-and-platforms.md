# Firmware and platforms

Architecture selection begins in `arch/mod.rs`, the firmware entry code, and `src/main.rs`. This guide describes x86_64, AArch64, and RISC-V source contracts at `08a14f6`, with build/boot evidence kept separate.

## Entry and HAL boundaries

| Architecture | Main implementation areas | Observed compile boundary |
| --- | --- | --- |
| x86_64 | UEFI, BootInfo, GDT/IDT, APIC/IOAPIC, paging, SYSCALL entry, user return, context switching | Both slim UEFI checks pass; full graph fails |
| AArch64 | UEFI and bare-metal entry, exception vectors, paging, timer, PSCI/SMP, EL0 entry and SVC | Both UEFI slim checks pass; full and bare-metal runtime not validated here |
| RISC-V | UEFI entry, SBI/SMP, Sv39 paging, traps/timer/controllers, S-mode user transition and ECALL | Both slim checks fail on unresolved `SIZE`; full source also has a missing generic IRQ module |

`arch/api.rs` defines shared traits such as paging, serial, and timer/interrupt abstractions, plus `PageFlags`. `arch::Arch` resolves to the selected HAL; architecture-specific register types and saved trap contexts remain in each target subtree.

The table describes code areas, not interchangeable machine capabilities. Some compatibility HAL methods are partial or no-ops, and default behavior must be checked at the exact caller.

## Firmware handoff

UEFI entry code obtains the memory map and available framebuffer/initramfs/platform information before handing a `BootInfo` value to the selected kernel entry. The minimal common path consumes those diagnostics without initializing the full graph.

`init/` contains BootInfo types, C runtime sources, initramfs handling, ELF/auxv loading, scheme registration, and a service manager. Bare-metal and UEFI paths have different ownership and mapping assumptions; do not use one path's success as evidence for the other.

RISC-V's audited failure is in `arch/riscv64/mem_layout.rs`, where a PTE helper refers to an out-of-scope `SIZE`. It occurs even before the slim profile can be considered a successful compilation.

## ACPI, device tree, and power

- **ACPI**: `firmware/acpi/` contains table discovery/parsing and architecture/power helpers. Tables such as MADT, FADT/FACS, SRAT, and related firmware descriptors have dedicated code; support must still be verified against actual table contents.
- **Device tree**: `firmware/dt.rs` provides FDT access helpers. Some boot/loader branches use `device_tree`, which is not declared as a root feature.
- **ARM power/topology**: `firmware/psci.rs` and `firmware/topology.rs` are AArch64-gated and provide CPU_ON/reset/off and CPU enumeration interfaces.
- **Power management**: Architecture and firmware power paths coexist with the unregistered `src/power/mod.rs`. The latter cannot be treated as a live generic suspend/hibernate subsystem.

No suspend/resume, hotplug, firmware interoperability, or topology-correctness pass was established in this audit. These remain platform-specific runtime acceptance work.

## Interrupts and SMP

x86_64 uses its architecture APIC/IDT implementation rather than a generic `irq/x86_64` subtree. AArch64 GIC lives under `irq/aarch64`, while `irq/riscv64/mod.rs` declares a missing `plic` source file despite architecture-local interrupt code elsewhere.

The common `smp::init` boot sequence currently has an x86_64-specific body: enumerate CPUs, start APs, publish `AP_GO`, and wait for online count. AArch64 and RISC-V also have separate architecture SMP code; its presence does not mean the common helper implements identical bring-up everywhere.

`smp/percpu`, `ipi`, and `hotplug` provide shared state, IPI handlers, and CPU-state management. Scheduler/stack lifetime, TLB shootdown, and stop-machine reclamation must be verified together rather than inferred from an online-CPU counter.

## Time and counters

`time/mod.rs` maintains monotonic/boottime counters and realtime/TAI offsets; the source initializes TAI offset to 37 seconds. That hardcoded value is a code fact, not a live time-standard lookup or evidence of time synchronization.

`clock`, `hpet`, `tsc`, `timer`, and `timerfd` cover clock reads, hardware-clock support, expiry queues, timers, and FD integration. `fs/timerfd.rs` is an additional syscall-facing implementation and remains separate.

Boot markers use raw TSC ticks on x86_64, `cntvct_el0` on AArch64, and the `time` CSR on RISC-V. Do not compare raw counts across architectures or label them nanoseconds without a verified conversion; see [boot performance](boot-perf.md).

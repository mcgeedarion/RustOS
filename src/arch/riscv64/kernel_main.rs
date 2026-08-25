//! RISC-V 64 kernel entry point.
//!
//! Called from `uefi_entry.rs` after the CPU is in supervisor mode with
//! interrupts disabled. Mirrors the sequencing of `x86_64/kernel_main.rs`
//! and `aarch64/kernel_main.rs`.
//!
//! ## Boot sequence
//!
//!   0. hal::init()               — disable IRQs, serial TX available
//!   1. paging::init()            — MMU on, kernel mapped
//!   2. heap_init()               — global allocator
//!   3. pmm::init()               — physical memory manager
//!   4. time::init()              — timer calibration
//!   5. interrupt controller init — PLIC/APLIC setup
//!   6. pci/ecam init             — PCIe base from ACPI (if present)
//!   7. virtio_blk / block init   — storage backend
//!   8. fs::initramfs::mount()    — initrd
//!   9. spawn_init()              — PID 1
//!  10. smp::bring_up_secondaries()
//!  11. idle loop

use crate::init::boot_info::BootInfo;
use crate::proc::exec::spawn_user_process;

const VIRTIO_BLK_MMIO_BASE: usize = 0x1000_1000;

pub fn init(boot_info: &'static BootInfo) -> ! {
    // 0. HAL already initialized in uefi_entry (serial, early setup).
    super::hal::init();

    // 1. MMU / paging.
    super::paging::init(boot_info);
    crate::serial_println!("riscv64: MMU enabled");

    // 2. Heap.
    crate::allocator::heap_init();
    crate::serial_println!("riscv64: heap ready");

    // 3. PMM.
    {
        let regions = super::mem_layout::memory_regions(boot_info);
        unsafe {
            crate::mm::pmm::init_from_regions(&regions);
        }
    }
    crate::mm::memmap::memmap_init();
    crate::serial_println!("riscv64: PMM ready");

    // 4. Timer.
    crate::time::init();
    crate::serial_println!(
        "time: clocksource={:?}",
        crate::time::clocksource()
    );

    // 5. Interrupt controller (PLIC/APLIC or SBI-based).
    crate::irq::riscv64::plic::init();
    crate::serial_println!("riscv64: interrupt controller ready");

    // 6. PCIe ECAM (base from ACPI MCFG — 0 if not present).
    let ecam_base = crate::firmware::acpi::mcfg_base().unwrap_or(0);
    if ecam_base != 0 {
        super::pci::ecam_init(ecam_base);
        crate::serial_println!("riscv64: ECAM base={:#x}", ecam_base);
    }

    // 7. Storage.
    let storage = probe_storage();
    crate::serial_println!("storage: backend={}", storage.name());

    // 8. Filesystems.
    crate::fs::initramfs::mount_initramfs();

    let disk_ok = storage.read_sector(0, &mut [0u8; 512]);
    if disk_ok {
        if crate::fs::ext2::mount() {
            crate::serial_println!("ext2: root mounted at /");
        } else {
            crate::serial_println!("ext2: mount failed - ramfs only");
        }
    } else {
        crate::serial_println!("block: no disk - ramfs only");
    }

    // 9. PID 1.
    const INITS: &[&str] = &[
        "/init",
        "/sbin/init",
        "/bin/sh",
        "/bin/smoke",
        "/bin/kmtest",
        "/bin/bash",
    ];
    let mut spawned = false;
    for path in INITS {
        if spawn_user_process(path, &[path], &[]) {
            crate::serial_println!("init: PID 1 from {}", path);
            // Emit BOOT_INIT_EXEC immediately after spawn_user_process() succeeds.
            crate::boot_mark!("BOOT_INIT_EXEC");
            spawned = true;
            break;
        }
    }
    if !spawned {
        crate::serial_println!("init: no init binary found - idle");
    }

    // 10. SMP.
    super::smp::bring_up_secondaries();

    // 11. Enable IRQs and idle.
    unsafe {
        super::hal::interrupts_enable();
    }
    crate::serial_println!("riscv64: kernel_main: idle");
    
    // CI boot sentinel — printed exactly once, just before the idle loop.
    crate::serial_println!("RUSTOS_BOOT_OK v1");
    
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nostack, nomem));
        }
        crate::proc::scheduler::schedule();
    }
}

enum StorageBackend {
    VirtioBlk,
    None,
}

impl StorageBackend {
    fn name(&self) -> &'static str {
        match self {
            Self::VirtioBlk => "virtio-blk",
            Self::None => "none",
        }
    }

    fn read_sector(&self, lba: u64, buf: &mut [u8]) -> bool {
        match self {
            Self::VirtioBlk => crate::block::virtio_blk::read_sector(lba, buf),
            Self::None => false,
        }
    }
}

fn probe_storage() -> StorageBackend {
    // On QEMU virt riscv64 the primary block device is virtio-blk over MMIO.
    crate::block::virtio_blk::virtio_blk_init(VIRTIO_BLK_MMIO_BASE);
    crate::serial_println!("block: virtio-blk @ {:#x}", VIRTIO_BLK_MMIO_BASE);
    StorageBackend::VirtioBlk
}

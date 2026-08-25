//! RISC-V 64 SMP (Symmetric Multi-Processing) support.
//!
//! On RISC-V systems, secondary CPUs (harts) are typically brought up via:
//! - SBI (Supervisor Binary Interface) HSM extension
//! - Device tree / ACPI MADT parsing for hart IDs
//! - Inter-processor interrupts (IPIs) via PLIC/IMSIC

use core::sync::atomic::{AtomicUsize, Ordering};

/// Stack top for the next secondary being started.
pub static SECONDARY_STACK_TOP: AtomicUsize = AtomicUsize::new(0);

/// Flag indicating if a secondary CPU has started.
static SECONDARY_STARTED: AtomicUsize = AtomicUsize::new(0);

/// Bring up all secondary CPUs (harts).
pub fn bring_up_secondaries() {
    // Get list of secondary hart IDs from device tree or ACPI
    let secondaries = crate::firmware::topology::secondary_mpidrs();
    if secondaries.is_empty() {
        crate::serial_println!("riscv64: no secondary harts found");
        return;
    }

    for hart_id in secondaries {
        let stack_top = match crate::mm::pmm::alloc_pages(4) {
            Some(pa) => pa + 4 * 4096,
            None => {
                crate::serial_println!(
                    "riscv64: out of memory for secondary stack hart={}",
                    hart_id
                );
                continue;
            }
        };

        SECONDARY_STACK_TOP.store(stack_top, Ordering::Release);
        SECONDARY_STARTED.store(0, Ordering::Release);

        // Use SBI HSM extension to start the secondary hart
        unsafe {
            if let Err(e) = crate::firmware::sbi::hart_start(hart_id, secondary_entry as usize) {
                crate::serial_println!(
                    "riscv64: SBI hart_start failed for hart={} err={:?}",
                    hart_id,
                    e
                );
                continue;
            }
        }

        // Wait for secondary to signal it has started
        let timeout = 1_000_000u64;
        let start = super::hal::read_time();
        while SECONDARY_STARTED.load(Ordering::Acquire) == 0 {
            if super::hal::read_time().wrapping_sub(start) > timeout {
                crate::serial_println!(
                    "riscv64: timeout waiting for secondary hart={}",
                    hart_id
                );
                break;
            }
            core::hint::spin_loop();
        }
    }
}

/// Entry point for secondary CPUs.
/// This is called on each secondary hart after it starts.
#[no_mangle]
extern "C" fn secondary_entry() -> ! {
    // Signal that we've started
    SECONDARY_STARTED.store(1, Ordering::Release);
    SECONDARY_STACK_TOP.store(0, Ordering::Release);

    // Initialize per-CPU state
    unsafe {
        // Enable FPU
        super::hal::fp_init();
        
        // Set up interrupt handling
        super::interrupts::init_percpu();
    }

    // Mark this CPU as online and start scheduling
    crate::proc::scheduler::secondary_online();
    crate::proc::scheduler::run()
}

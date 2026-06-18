/* aarch64 bare-metal kernel linker script.
 *
 * Entry: _start in src/arch/aarch64/boot.rs.
 * UEFI builds use targets/aarch64-uefi-loader.json and enter efi_main directly;
 * this script is used by targets/aarch64-kernel.json for the bare-metal path.
 *
 * Kernel virtual base must match va48::KERNEL_BASE in mem_layout.rs.
 * The early boot code starts with MMU/caches off, sets up a temporary stack,
 * clears .bss, and then paging::init_mmu maps the image at this higher-half VA
 * before kernel_main_aarch64() runs.
 */
OUTPUT_FORMAT("elf64-littleaarch64")
OUTPUT_ARCH(aarch64)
ENTRY(_start)

PROVIDE(_kernel_start = 0xffff000000000000);

/* Initramfs location symbols — default to 0 (no initramfs).
 * A bootloader/UEFI stub may overwrite these before jumping to kernel_main. */
PROVIDE(INITRAMFS_BASE = 0);
PROVIDE(INITRAMFS_SIZE = 0);

SECTIONS {
  . = 0xffff000000000000;

  /* Keep the early assembly entry first even when --gc-sections is enabled. */
  .text.boot ALIGN(4096) : {
    KEEP(*(.text.boot))
  }

  .text ALIGN(4096) : {
    KEEP(*(.text._start))
    *(.text .text.*)
  }

  .rodata ALIGN(4096) : { *(.rodata .rodata.*) }

  /* C/C++ global ctor/dtor arrays; iterated by run_init_array() at boot. */
  .init_array ALIGN(8) : {
    __init_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
    KEEP(*(.init_array))
    __init_array_end = .;
  }

  .fini_array ALIGN(8) : {
    __fini_array_start = .;
    KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
    KEEP(*(.fini_array))
    __fini_array_end = .;
  }

  .data ALIGN(4096) : { *(.data .data.*) }

  /* Kernel test registry — populated at link time by #[kernel_test] statics. */
  .kmtest_registry ALIGN(8) : {
    __kmtest_start = .;
    KEEP(*(.kmtest_registry))
    __kmtest_end = .;
  }

  .bss ALIGN(4096) (NOLOAD) : {
    __bss_start = .;
    *(.bss .bss.*)
    *(COMMON)
    . = ALIGN(16);
    __bss_end = .;
  }

  /* Boot stack: 64 KiB, 16-byte aligned for the AArch64 ABI. */
  .stack ALIGN(16) (NOLOAD) : {
    __boot_stack_bottom = .;
    . += 64K;
    . = ALIGN(16);
    __boot_stack_top = .;
  }

  . = ALIGN(4096);
  _end = .;

  /DISCARD/ : { *(.comment) *(.eh_frame) *(.note.GNU-stack) }
}

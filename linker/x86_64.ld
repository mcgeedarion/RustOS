/* x86_64 UEFI kernel linker script.
 *
 * Entry: uefi_start (PE32+ UEFI application).
 *
 * Load address: 0x400000 physical.  UEFI firmware identity-maps all RAM
 * before ExitBootServices, so this must be a reachable physical address.
 * The high-half virtual mapping (0xFFFFFFFF80000000) is established by
 * the kernel's own page-table setup after ExitBootServices returns.
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(uefi_start)

SECTIONS {
    . = 0x400000;
    _kernel_start = .;

    .text ALIGN(4096) : {
        KEEP(*(.text.uefi_start))
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

    /* Kernel test registry — populated at link time by #[kernel_test] statics.
     * Only present when the kernel is compiled with --features kmtest. */
    .kmtest_registry ALIGN(8) : {
        __kmtest_start = .;
        KEEP(*(.kmtest_registry))
        __kmtest_end = .;
    }

    .bss ALIGN(4096) (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Boot stack — 32 KiB, 16-byte aligned.
     * uefi_start switches to this stack immediately on entry via:
     *   lea rsp, [rip + __boot_stack_top]
     * NOLOAD: occupies address space but is not written to the ELF file. */
    .stack ALIGN(16) (NOLOAD) : {
        __boot_stack_bottom = .;
        . += 0x8000;   /* 32 KiB */
        . = ALIGN(16);
        __boot_stack_top = .;
    }

    . = ALIGN(4096);
    _end = .;

    /* Initramfs location symbols — default to 0 (no initramfs).
     * The UEFI stub sets these before jumping to kernel_main. */
    PROVIDE(__initramfs_start = 0);
    PROVIDE(__initramfs_end   = 0);

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame .eh_frame.*)
        *(.note .note.*)
    }
}

/* x86_64 bare-metal ELF kernel linker script.
 *
 * Entry: uefi_start (shared early entry symbol used by the boot handoff).
 *
 * Load address: 0x400000 physical.  The early boot environment must load the
 * ELF image at a reachable identity-mapped physical address.
 * The high-half virtual mapping (0xFFFFFFFF80000000) is established by
 * the kernel's own page-table setup after ExitBootServices returns.
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(uefi_start)

PHDRS {
    text PT_LOAD FILEHDR PHDRS FLAGS(5); /* R + X */
    data PT_LOAD FLAGS(6); /* R + W */
}

SECTIONS {
    . = 0x400000 + SIZEOF_HEADERS;
    _kernel_start = .;

    . = ALIGN(4096);
    .text : {
        KEEP(*(.text.uefi_start))
        KEEP(*(.text._start))
        *(.text .text.*)
    } :text

    . = ALIGN(4096);
    .rodata : { *(.rodata .rodata.*) } :text

    /* C/C++ global ctor/dtor arrays; iterated by run_init_array() at boot. */
    . = ALIGN(8);
    .init_array : {
        __init_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)))
        KEEP(*(.init_array))
        __init_array_end = .;
    } :data

    . = ALIGN(8);
    .fini_array : {
        __fini_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.fini_array.*)))
        KEEP(*(.fini_array))
        __fini_array_end = .;
    } :data

    . = ALIGN(4096);
    .data : { *(.data .data.*) } :data

    /* Kernel test registry — populated at link time by #[kernel_test] statics.
     * Only present when the kernel is compiled with --features kmtest. */
    . = ALIGN(8);
    .kmtest_registry : {
        __kmtest_start = .;
        KEEP(*(.kmtest_registry))
        __kmtest_end = .;
    } :data

    . = ALIGN(4096);
    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    /* Boot stack — 32 KiB, 16-byte aligned.
     * uefi_start switches to this stack immediately on entry via:
     *   lea rsp, [rip + __boot_stack_top]
     * NOLOAD: occupies address space but is not written to the ELF file. */
    . = ALIGN(16);
    __boot_stack_bottom = .;
    . += 0x8000;   /* 32 KiB */
    . = ALIGN(16);
    __boot_stack_top = .;

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
        *(.debug_abbrev .debug_abbrev.*)
        *(.debug_aranges .debug_aranges.*)
        *(.debug_frame .debug_frame.*)
        *(.debug_info .debug_info.*)
        *(.debug_line .debug_line.*)
        *(.debug_str .debug_str.*)
        *(.debug_ranges .debug_ranges.*)
        *(.debug_rnglists .debug_rnglists.*)
        *(.debug_loc .debug_loc.*)
        *(.debug_loclists .debug_loclists.*)
    }
}

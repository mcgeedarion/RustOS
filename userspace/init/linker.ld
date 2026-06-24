/* userspace/init/linker.ld
 *
 * Minimal linker script for the no-libc Rust init binary.
 * Produces a static ELF64 with:
 *   - _start at the ELF entry point (0x400000 base, matches Linux defaults)
 *   - .text before .rodata before .data before .bss
 *   - No dynamic section, no PLT, no GOT relocs
 *
 * The kernel ELF loader maps PT_LOAD segments directly at their
 * p_vaddr. This script ensures /init loads cleanly under rustos.
 *
 * Usage (rustflags in .cargo/config.toml):
 *   "-C", "link-arg=-T/path/to/linker.ld"
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

BASE = 0x400000;

SECTIONS
{
    . = BASE;

    .text : {
        *(.text._start)
        *(.text.init_main)
        *(.text*)
    }

    . = ALIGN(4096);
    .rodata : { *(.rodata*) }

    . = ALIGN(4096);
    .data : { *(.data*) }

    . = ALIGN(4096);
    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
        *(.gnu.hash)
    }
}

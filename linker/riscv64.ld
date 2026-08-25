/* RISC-V 64-bit Linker Script for Bare-Metal Rust OS */

OUTPUT_ARCH(riscv)
ENTRY(_start)

/* Memory Layout Configuration */
/* Adjust BASE_ADDRESS as needed for your platform (e.g., QEMU virt, SiFive, etc.) */
/* 0x80200000 is the typical entry point for RISC-V 64-bit bare-metal systems */
BASE_ADDRESS = 0x80200000;

MEMORY
{
    RAM (rwx) : ORIGIN = BASE_ADDRESS, LENGTH = 128M
}

SECTIONS
{
    /* Load address and entry point */
    . = BASE_ADDRESS;

    /* Text section: code and read-only data */
    .text : ALIGN(4K)
    {
        *(.text.entry)
        *(.text .text.*)
    } > RAM

    /* Read-only data section */
    .rodata : ALIGN(4K)
    {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    } > RAM

    /* Initialized data section */
    .data : ALIGN(4K)
    {
        *(.data .data.*)
        *(.sdata .sdata.*)
    } > RAM

    /* Uninitialized data section (zero-initialized at runtime) */
    .bss : ALIGN(4K)
    {
        _bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        _bss_end = .;
    } > RAM

    /* Init array section (for constructors) */
    .init_array : ALIGN(8)
    {
        _init_array_start = .;
        KEEP(*(.init_array))
        KEEP(*(.ctors))
        _init_array_end = .;
    } > RAM

    /* Stack allocation */
    /* Allocate stack space with proper alignment (16-byte for RISC-V) */
    .stack (NOLOAD) : ALIGN(16)
    {
        _stack_start = .;
        . += 0x20000; /* 128KB stack - adjust as needed */
        _stack_end = .;
    } > RAM

    /* Discard unnecessary sections */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
        *(.note.GNU-stack)
        *(.gnu.hash)
        *(.interp)
        *(.dynsym)
        *(.dynstr)
        *(.dynamic)
        *(.got)
        *(.got.plt)
        *(.plt)
        *(.rel.dyn)
        *(.rel.plt)
    }
}

/* Provide symbols for use in Rust code */
PROVIDE(_heap_start = _bss_end);
PROVIDE(_heap_end = _stack_start);

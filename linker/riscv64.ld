OUTPUT_FORMAT("elf64-littleriscv")
OUTPUT_ARCH(riscv)
ENTRY(_start)

/*
 * Initramfs location symbols — default to 0 (no initramfs).
 * OpenSBI / bootloader overwrites these with the physical address and byte
 * length of the CPIO archive before jumping to kernel_main.
 */
PROVIDE(INITRAMFS_BASE = 0);
PROVIDE(INITRAMFS_SIZE = 0);

PHDRS {
  text   PT_LOAD FLAGS(5); /* R + X */
  rodata PT_LOAD FLAGS(4); /* R */
  data   PT_LOAD FLAGS(6); /* R + W */
}

SECTIONS {
  . = 0x80200000;
  _kernel_start = .;

  /*
   * .text.boot must come first so that _start (the OpenSBI entry point) is
   * the very first instruction executed.  Mark it with
   * #[link_section = ".text.boot"] in Rust.
   */
  .text.boot ALIGN(4096) : {
    KEEP(*(.text.boot))
  } :text

  .text ALIGN(4096) : {
    *(.text .text.*)
  } :text

  .rodata ALIGN(4096) : {
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
  } :rodata

  .init_array ALIGN(8) : {
    __init_array_start = .;
    KEEP(*(.init_array .init_array.*))
    __init_array_end = .;
  } :rodata

  .fini_array ALIGN(8) : {
    __fini_array_start = .;
    KEEP(*(.fini_array .fini_array.*))
    __fini_array_end = .;
  } :rodata

  .eh_frame ALIGN(8) : {
    *(.eh_frame .eh_frame.*)
  } :rodata

  .data ALIGN(4096) : {
    *(.data .data.*)
    *(.sdata .sdata.*)
    *(.got .got.*)
  } :data

  /*
   * Kernel test registry — populated at link time by #[kernel_test] statics.
   * Only present when the kernel is compiled with --features kmtest.
   */
  .kmtest_registry ALIGN(8) : {
    __kmtest_start = .;
    KEEP(*(.kmtest_registry))
    __kmtest_end = .;
  } :data

  .bss ALIGN(4096) : {
    __bss_start = .;
    *(.bss .bss.*)
    *(.sbss .sbss.*)
    *(COMMON)
    __bss_end = .;
  } :data

  . = ALIGN(4096);
  _kernel_end = .;
  _end = .;

  /DISCARD/ : {
    *(.comment)
    *(.note.GNU-stack)
  }
}

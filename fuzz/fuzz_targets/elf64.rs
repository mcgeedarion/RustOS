#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/exec/elf.rs"]
mod production_elf;

fuzz_target!(|data: &[u8]| {
    if let Ok(header) = production_elf::parse_elf_header(data) {
        if let Some(phdrs) = production_elf::parse_phdrs_with_hdr(data, &header) {
            let _ = production_elf::load_elf_into(0, data, &header, &phdrs);
            let _ = production_elf::end_of_bss(&phdrs, 0);
            let _ = production_elf::end_of_bss(&phdrs, 0x1000);
        }
    }
});

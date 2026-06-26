//! 64-bit ELF loader.
//!
//! Parses an ELF64 executable and maps its PT_LOAD segments into a new
//! address space.  Called from `exec.rs`.

use crate::mm::vma::{MappingFlags, VmaKind};
use crate::proc::process::Process;
use alloc::vec::Vec;
use core::mem;

// ELF machine types we accept
const EM_X86_64:  u16 = 62;
const EM_AARCH64: u16 = 183;

// ELF class / data / version constants
const ELFCLASS64:  u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT:  u8 = 1;

// e_type values
const ET_EXEC: u16 = 2;
const ET_DYN:  u16 = 3;

// Program header types
const PT_LOAD:    u32 = 1;
const PT_INTERP:  u32 = 3;
const PT_PHDR:    u32 = 6;
const PT_TLS:     u32 = 7;

// Program header flags
const PF_X: u32 = 0x1;
const PF_W: u32 = 0x2;
const PF_R: u32 = 0x4;

/// Errors returned by the ELF loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    Not64Bit,
    WrongEndian,
    BadVersion,
    NotExecutableOrDyn,
    WrongArch,
    OverlappingSegments,
    MmapFailed,
    InterpNotFound,
    BadPhdrTable,
}

/// Minimal ELF64 file header (52 bytes we care about).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Ehdr {
    e_ident:     [u8; 16],
    e_type:      u16,
    e_machine:   u16,
    e_version:   u32,
    e_entry:     u64,
    e_phoff:     u64,
    e_shoff:     u64,
    e_flags:     u32,
    e_ehsize:    u16,
    e_phentsize: u16,
    e_phnum:     u16,
    e_shentsize: u16,
    e_shnum:     u16,
    e_shstrndx:  u16,
}

/// ELF64 program header.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Elf64Phdr {
    p_type:   u32,
    p_flags:  u32,
    p_offset: u64,
    p_vaddr:  u64,
    p_paddr:  u64,
    p_filesz: u64,
    p_memsz:  u64,
    p_align:  u64,
}

/// Result of a successful ELF load.
pub struct LoadedElf {
    /// Virtual address of the first instruction.
    pub entry:       u64,
    /// Load bias applied to all PT_LOAD segments (0 for ET_EXEC).
    pub load_bias:   u64,
    /// Virtual address of the program-header table in the loaded image.
    pub phdr_vaddr:  u64,
    /// Number of program headers.
    pub phdr_count:  u16,
    /// Path of the PT_INTERP interpreter, if present.
    pub interp_path: Option<alloc::string::String>,
    /// TLS template info (vaddr, filesz, memsz) if PT_TLS present.
    pub tls:         Option<(u64, u64, u64)>,
}

/// Load an ELF64 image into `proc`'s address space.
///
/// `data` is the raw file bytes (already read into kernel memory).
/// `load_base` is the preferred base for `ET_DYN`; ignored for `ET_EXEC`.
pub fn load_elf64(
    proc: &mut Process,
    data: &[u8],
    load_base: u64,
) -> Result<LoadedElf, ElfError> {
    // ── header validation ──────────────────────────────────────────────────
    if data.len() < mem::size_of::<Elf64Ehdr>() {
        return Err(ElfError::TooSmall);
    }
    // SAFETY: we just checked the length.
    let ehdr: &Elf64Ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

    if &ehdr.e_ident[0..4] != b"\x7fELF" {
        return Err(ElfError::BadMagic);
    }
    if ehdr.e_ident[4] != ELFCLASS64 {
        return Err(ElfError::Not64Bit);
    }
    if ehdr.e_ident[5] != ELFDATA2LSB {
        return Err(ElfError::WrongEndian);
    }
    if ehdr.e_ident[6] != EV_CURRENT {
        return Err(ElfError::BadVersion);
    }
    if ehdr.e_type != ET_EXEC && ehdr.e_type != ET_DYN {
        return Err(ElfError::NotExecutableOrDyn);
    }

    // Accept only the architectures we support.
    match ehdr.e_machine {
        EM_X86_64  => {
            #[cfg(not(target_arch = "x86_64"))]
            return Err(ElfError::WrongArch);
        }
        EM_AARCH64 => {
            #[cfg(not(target_arch = "aarch64"))]
            return Err(ElfError::WrongArch);
        }
        _ => return Err(ElfError::WrongArch),
    }

    // ── program header table ───────────────────────────────────────────────
    let phentsize = ehdr.e_phentsize as usize;
    let phnum     = ehdr.e_phnum    as usize;
    let phoff     = ehdr.e_phoff    as usize;

    if phentsize < mem::size_of::<Elf64Phdr>() {
        return Err(ElfError::BadPhdrTable);
    }
    let phdr_end = phoff.checked_add(phentsize.checked_mul(phnum).ok_or(ElfError::BadPhdrTable)?)
        .ok_or(ElfError::BadPhdrTable)?;
    if phdr_end > data.len() {
        return Err(ElfError::BadPhdrTable);
    }

    // ── compute load bias for ET_DYN ───────────────────────────────────────
    let bias: u64 = if ehdr.e_type == ET_DYN { load_base } else { 0 };

    // ── first pass: collect PT_LOAD / PT_INTERP / PT_TLS / PT_PHDR ────────
    let mut interp_path: Option<alloc::string::String> = None;
    let mut tls:         Option<(u64, u64, u64)>        = None;
    let mut phdr_vaddr:  u64                             = 0;

    let mut load_segments: Vec<Elf64Phdr> = Vec::new();

    for i in 0..phnum {
        let off = phoff + i * phentsize;
        // SAFETY: bounds checked above.
        let phdr: &Elf64Phdr = unsafe { &*(data.as_ptr().add(off) as *const Elf64Phdr) };

        match phdr.p_type {
            PT_LOAD => load_segments.push(*phdr),
            PT_INTERP => {
                let start = phdr.p_offset as usize;
                let end   = start.saturating_add(phdr.p_filesz as usize);
                if end > data.len() { return Err(ElfError::InterpNotFound); }
                let bytes = &data[start..end];
                // strip trailing NUL
                let s = bytes.iter().position(|&b| b == 0).map(|n| &bytes[..n]).unwrap_or(bytes);
                interp_path = Some(alloc::string::String::from_utf8_lossy(s).into_owned());
            }
            PT_PHDR => { phdr_vaddr = phdr.p_vaddr + bias; }
            PT_TLS  => { tls = Some((phdr.p_vaddr + bias, phdr.p_filesz, phdr.p_memsz)); }
            _ => {}
        }
    }

    // ── second pass: map PT_LOAD segments ──────────────────────────────────
    for seg in &load_segments {
        let vaddr = (seg.p_vaddr + bias) as usize;
        let memsz = seg.p_memsz  as usize;

        let mut flags = MappingFlags::empty();
        if seg.p_flags & PF_R != 0 { flags |= MappingFlags::READ;  }
        if seg.p_flags & PF_W != 0 { flags |= MappingFlags::WRITE; }
        if seg.p_flags & PF_X != 0 { flags |= MappingFlags::EXEC;  }

        let file_start  = seg.p_offset as usize;
        let file_end    = file_start.saturating_add(seg.p_filesz as usize);
        let file_data   = if file_end <= data.len() { &data[file_start..file_end] } else { &[] };

        proc.mm.map_segment(vaddr, memsz, flags, VmaKind::Anonymous, Some(file_data))
            .map_err(|_| ElfError::MmapFailed)?;
    }

    Ok(LoadedElf {
        entry:      ehdr.e_entry + bias,
        load_bias:  bias,
        phdr_vaddr,
        phdr_count: ehdr.e_phnum,
        interp_path,
        tls,
    })
}

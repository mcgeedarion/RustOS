/// build.rs
use std::path::{Path, PathBuf};
use std::process::Command;

const CRT_DIR: &str = "src/init/crt";
const CRT_SOURCES: &[&str] = &[
    "compiler_rt.c",
    "crt0.c",
    "memcpy.c",
    "memmove.c",
    "memset.c",
];

const CRT_COMPILE_FLAGS: &[&str] = &[
    "-ffreestanding",
    "-nostdlib",
    "-O2",
    "-fno-stack-protector",
    "-fno-builtin",
    "-Wno-builtin-declaration-mismatch",
];

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let boot_minimal = std::env::var("CARGO_FEATURE_BOOT_MINIMAL").is_ok();

    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=RUSTOS_INITRAMFS");
    println!("cargo:rerun-if-env-changed=RUSTOS_INITRAMFS_FINGERPRINT");
    write_initramfs_embed(&out);

    // ----------------------------------------------------------------
    // Bare-metal ELF targets only (x86_64-kernel.json, aarch64-kernel.json)
    // UEFI PE/COFF targets (aarch64-uefi-loader.json) have target_os ==
    // "uefi"; they use the PE/COFF layout built into lld-link and must NOT
    // have an ELF linker script injected.
    // ----------------------------------------------------------------
    if target_os != "uefi" {
        let script = match target_arch.as_str() {
            "x86_64" => "linker/x86_64.ld",
            "aarch64" => "linker/aarch64.ld",
            other => {
                println!("cargo:warning=build.rs: no linker script for arch '{other}'");
                ""
            },
        };
        if !script.is_empty() {
            println!("cargo:rerun-if-changed={script}");
        }

        // CRT stubs are only needed for bare-metal ELF builds.
        if !boot_minimal {
            compile_crt(&target_arch);
        }
    }

    if std::env::var("CARGO_FEATURE_TRACE").is_ok() {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TRACE");
        println!("cargo:rerun-if-env-changed=RUSTFLAGS");

        let rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
        if !rustflags.contains("-Z instrument-functions") {
            println!(
                "cargo:warning=trace feature enabled without RUSTFLAGS='-Z instrument-functions'; ftrace callbacks will not be inserted"
            );
        }
    }
}

fn write_initramfs_embed(out: &Path) {
    let dest = out.join("initramfs_embed.rs");
    let content = match std::env::var("RUSTOS_INITRAMFS") {
        Ok(path) if !path.is_empty() && std::path::Path::new(&path).exists() => {
            println!("cargo:rerun-if-changed={path}");
            let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
            format!("pub static INITRAMFS: &[u8] = include_bytes!(\"{escaped}\");\n")
        },
        _ => "pub static INITRAMFS: &[u8] = &[];\n".to_string(),
    };
    std::fs::write(dest, content).expect("write initramfs_embed.rs");
}

/// Compile C runtime stubs into a static archive `librustos_crt.a`.
fn compile_crt(target_arch: &str) {
    for src in CRT_SOURCES {
        println!("cargo:rerun-if-changed={CRT_DIR}/{src}");
    }

    let mut build = cc::Build::new();
    configure_crt_compiler(&mut build, target_arch);

    for flag in CRT_COMPILE_FLAGS {
        build.flag(flag);
    }

    for src in CRT_SOURCES {
        build.file(format!("{CRT_DIR}/{src}"));
    }

    build.compile("rustos_crt");
}

/// Select a usable C compiler for freestanding CRT objects when cross-building.
///
/// `cc` will honor explicit `CC_<target>`, `TARGET_CC`, and `CC` environment
/// variables before this helper runs. When none are present for non-host kernel
/// targets, prefer Clang with an explicit target triple so host `cc` is not
/// invoked with incompatible `-march`/`-mabi` flags.
fn configure_crt_compiler(build: &mut cc::Build, target_arch: &str) {
    if explicit_cc_override_is_set(target_arch) {
        return;
    }

    match target_arch {
        "aarch64" if command_exists("clang") => {
            build.compiler("clang");
            build.flag("--target=aarch64-none-elf");
        },
        // Use LLVM/clang's integrated assembler for RISC-V instead of relying on
        // a separately installed GNU `riscv64-unknown-elf-as`/`riscv64-unknown-elf-gcc`
        // toolchain. clang targets riscv64-unknown-elf directly and assembles with
        // the LLVM integrated assembler, so no external binutils are required.
        "riscv64" if command_exists("clang") => {
            build.compiler("clang");
            build.flag("--target=riscv64-unknown-elf");
            build.flag("-march=rv64gc");
            build.flag("-mabi=lp64d");
        },
        _ => {},
    }
}

fn explicit_cc_override_is_set(target_arch: &str) -> bool {
    let target = std::env::var("TARGET").unwrap_or_default();
    let normalized_target = target.replace('-', "_");
    let upper_target = normalized_target.to_ascii_uppercase();

    let mut vars = vec![
        "CC".to_string(),
        "TARGET_CC".to_string(),
        format!("CC_{target_arch}"),
    ];

    if !normalized_target.is_empty() {
        vars.push(format!("CC_{normalized_target}"));
    }
    if !upper_target.is_empty() {
        vars.push(format!("CC_{upper_target}"));
    }

    vars.into_iter().any(|var| std::env::var_os(var).is_some())
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

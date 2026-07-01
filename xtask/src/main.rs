//! cargo xtask — build automation for RustOS.
//!
//! Canonical build/run contract:
//!   aarch64: uefi | baremetal
//!   x86_64:  uefi
//!
//! Canonical ESP staging path:
//!   target/esp/<arch>/EFI/BOOT/BOOT*.EFI
//!
//! Golden-path developer on-ramp (Phase 1):
//!   cargo xtask run --arch x86_64
//!
//! That single command:
//!   1. Builds the kernel (x86_64-unknown-uefi, --features uefi_boot)
//!   2. Stages the EFI binary into target/esp/x86_64/EFI/BOOT/BOOTX64.EFI
//!   3. Assembles a FAT16 ESP disk image at boot-x86_64.img
//!   4. Auto-downloads OVMF_CODE.fd into .ovmf/ if no system firmware found
//!   5. Launches qemu-system-x86_64 with serial output on stdout
//!
//! Phase 2 userspace on-ramp:
//!   cargo xtask build-init
//!
//! That command:
//!   1. Adds the x86_64-unknown-linux-musl rustup target if absent
//!   2. Compiles userspace/init (pure no-libc Rust) as a static ELF
//!   3. Stages the binary as /init in a minimal initramfs tree
//!   4. Packs initramfs.cpio (newc format) at the workspace root

use anyhow::{anyhow, bail, Context, Result};
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{exit, Command},
};

const OS_RELEASE_CONTENT: &[u8] =
    b"NAME=RustOS\nID=rustos\nVERSION=0.1.0\nPRETTY_NAME=\"RustOS 0.1.0\"\n";

/// Fedora mirror for a known-good OVMF build. Only fetched when no system
/// OVMF is present and OVMF_CODE is not set in the environment.
/// The file is cached at .ovmf/OVMF_CODE.fd after the first download.
const OVMF_DOWNLOAD_URL: &str =
    "https://dl.fedoraproject.org/pub/fedora/linux/releases/40/Everything/x86_64/os/Packages/e/\
     edk2-ovmf-20231122-6.fc40.noarch.rpm";

/// Well-known system paths where distros install OVMF.
const OVMF_SYSTEM_CANDIDATES: &[&str] = &[
    "/usr/share/OVMF/OVMF_CODE.fd",
    "/usr/share/OVMF/OVMF_CODE_4M.fd",
    "/usr/share/ovmf/OVMF.fd",
    "/usr/share/qemu/OVMF.fd",
    "/usr/share/edk2/x64/OVMF_CODE.fd",
    "/usr/share/edk2-ovmf/OVMF_CODE.fd",
    "/opt/homebrew/share/qemu/edk2-x86_64-code.fd", // macOS Homebrew
    "/usr/local/share/qemu/edk2-x86_64-code.fd",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arch {
    AArch64,
    X86_64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boot {
    Uefi,
    Baremetal,
}

/// Build profile used for kernel compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildProfile {
    /// Dev/debug build: -O0 + full debug symbols, `cfg(debug_assertions)` on.
    Dev,
    /// Optimised developer build: [profile.release] + default debug features.
    Release,
    /// Lean boot image: [profile.release-boot] + release-boot feature set.
    ReleaseBoot,
}

#[derive(Debug, Clone)]
struct BuildOpts {
    arch: Arch,
    boot: Boot,
    profile: BuildProfile,
    initrd: bool,
    features: Option<String>,
}

impl Default for BuildOpts {
    fn default() -> Self {
        Self {
            arch: Arch::X86_64,
            boot: Boot::Uefi,
            profile: BuildProfile::Dev,
            initrd: false,
            features: None,
        }
    }
}

fn log(msg: impl AsRef<str>) {
    eprintln!("[xtask] {}", msg.as_ref());
}

fn arch_str(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "aarch64",
        Arch::X86_64 => "x86_64",
    }
}

fn boot_str(boot: Boot) -> &'static str {
    match boot {
        Boot::Uefi => "uefi",
        Boot::Baremetal => "baremetal",
    }
}

fn profile_str(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Dev => "debug",
        BuildProfile::Release => "release",
        BuildProfile::ReleaseBoot => "release-boot",
    }
}

fn validate_contract(arch: Arch, boot: Boot) -> Result<()> {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi | Boot::Baremetal) => Ok(()),
        (Arch::X86_64, Boot::Uefi) => Ok(()),
        _ => bail!(
            "unsupported build contract: {} --boot {}",
            arch_str(arch),
            boot_str(boot)
        ),
    }
}

fn target_json(root: &Path, arch: Arch, boot: Boot) -> PathBuf {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi) => PathBuf::from("aarch64-unknown-uefi"),
        (Arch::AArch64, Boot::Baremetal) => root.join("targets/aarch64-kernel.json"),
        (Arch::X86_64, Boot::Uefi) => PathBuf::from("x86_64-unknown-uefi"),
        _ => unreachable!("validate_contract must run before target_json"),
    }
}

fn target_dir_name(arch: Arch, boot: Boot) -> &'static str {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi) => "aarch64-unknown-uefi",
        (Arch::AArch64, Boot::Baremetal) => "aarch64-kernel",
        (Arch::X86_64, Boot::Uefi) => "x86_64-unknown-uefi",
        _ => unreachable!("validate_contract must run before target_dir_name"),
    }
}

fn efi_boot_filename(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "BOOTAA64.EFI",
        Arch::X86_64 => "BOOTX64.EFI",
    }
}

fn image_name(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "boot-aarch64.img",
        Arch::X86_64 => "boot-x86_64.img",
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has no parent directory")
        .to_path_buf()
}

fn build_output_path(root: &Path, opts: &BuildOpts) -> PathBuf {
    root.join("target")
        .join(target_dir_name(opts.arch, opts.boot))
        .join(profile_str(opts.profile))
}

fn artifact_path(root: &Path, opts: &BuildOpts) -> Option<PathBuf> {
    let base = build_output_path(root, opts).join("rustos");
    let efi = base.with_extension("efi");
    if opts.boot == Boot::Uefi && efi.exists() {
        Some(efi)
    } else if base.exists() {
        Some(base)
    } else {
        None
    }
}

fn esp_boot_dir(root: &Path, arch: Arch) -> PathBuf {
    root.join("target/esp")
        .join(arch_str(arch))
        .join("EFI/BOOT")
}

fn cargo() -> Command {
    Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn run(cmd: &mut Command) -> Result<()> {
    log(format!("running: {:?}", cmd));
    let status = cmd.status().context("failed to spawn command")?;
    if !status.success() {
        bail!("command failed with {status}");
    }
    Ok(())
}

fn which_first(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        Command::new("sh")
            .args(["-c", &format!("command -v {name} >/dev/null 2>&1")])
            .status()
            .ok()
            .filter(|status| status.success())
            .map(|_| (*name).to_string())
    })
}

fn require_tool(names: &[&str], install_hint: &str) {
    if which_first(names).is_none() {
        eprintln!("[xtask] ERROR: none of {:?} found on PATH", names);
        eprintln!("[xtask] Install with: {install_hint}");
        exit(1);
    }
}

fn has_feature(opts: &BuildOpts, feature: &str) -> bool {
    opts.features
        .as_deref()
        .map(|features| {
            features
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == feature)
        })
        .unwrap_or(false)
}

fn initrd_path(root: &Path) -> PathBuf {
    root.join("initramfs.cpio")
}

fn ensure_initrd(root: &Path, opts: &BuildOpts) -> Result<Option<PathBuf>> {
    if !opts.initrd {
        return Ok(None);
    }

    if has_feature(opts, "userspace_boot") {
        build_init(root, opts.arch)?;
    }

    let initrd = initrd_path(root);
    if !initrd.exists() {
        bail!(
            "initrd requested but {} does not exist; run `cargo xtask build-init --arch {}`",
            initrd.display(),
            arch_str(opts.arch)
        );
    }

    Ok(Some(initrd))
}

fn parse_build_args(args: &[String]) -> BuildOpts {
    let mut opts = BuildOpts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--arch" => {
                i += 1;
                opts.arch = match args.get(i).map(String::as_str) {
                    Some("aarch64") => Arch::AArch64,
                    Some("x86_64") => Arch::X86_64,
                    other => {
                        eprintln!("[xtask] unknown --arch: {:?}", other);
                        exit(1);
                    },
                };
            },
            "--boot" => {
                i += 1;
                opts.boot = match args.get(i).map(String::as_str) {
                    Some("uefi") => Boot::Uefi,
                    Some("baremetal") | Some("bare-metal") => Boot::Baremetal,
                    other => {
                        eprintln!("[xtask] unknown --boot: {:?}", other);
                        exit(1);
                    },
                };
            },
            "--features" => {
                i += 1;
                opts.features = args.get(i).cloned();
            },
            "--debug" => opts.profile = BuildProfile::Dev,
            "--profile" => {
                i += 1;
                opts.profile = match args.get(i).map(String::as_str) {
                    Some("dev") | Some("debug") => BuildProfile::Dev,
                    Some("release") => BuildProfile::Release,
                    Some("release-boot") => BuildProfile::ReleaseBoot,
                    other => {
                        eprintln!("[xtask] unknown --profile: {:?}", other);
                        exit(1);
                    },
                };
            },
            "--initrd" => opts.initrd = true,
            other => {
                eprintln!("[xtask] unknown argument: {other}");
                exit(1);
            },
        }
        i += 1;
    }
    opts
}

fn add_build_std_flags(cmd: &mut Command) {
    cmd.args([
        "-Z",
        "build-std=core,alloc,compiler_builtins",
        "-Z",
        "build-std-features=compiler-builtins-mem",
        "-Z",
        "json-target-spec",
    ]);
}

fn kernel_cargo_command(root: &Path, opts: &BuildOpts, subcommand: &str) -> Result<Command> {
    validate_contract(opts.arch, opts.boot)?;

    let mut cmd = cargo();
    cmd.current_dir(root)
        .arg(subcommand)
        .arg("--target")
        .arg(target_json(root, opts.arch, opts.boot));
    if opts.initrd {
        let initrd = initrd_path(root);
        cmd.env("RUSTOS_INITRAMFS", &initrd);
        if let Ok(meta) = fs::metadata(&initrd) {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            cmd.env(
                "RUSTOS_INITRAMFS_FINGERPRINT",
                format!("{}:{modified}", meta.len()),
            );
        }
    }
    add_build_std_flags(&mut cmd);

    match opts.profile {
        BuildProfile::Dev => {
            // default: debug profile, default feature set
        },
        BuildProfile::Release => {
            cmd.arg("--release");
        },
        BuildProfile::ReleaseBoot => {
            cmd.args(["--profile", "release-boot"]);
            // Lean feature set: no default debug / test / profiling features.
            cmd.arg("--no-default-features");
            cmd.arg("--features").arg("release-boot");
        },
    }

    if !matches!(opts.profile, BuildProfile::ReleaseBoot) {
        match &opts.features {
            Some(features) => {
                let requested: Vec<&str> = features
                    .split(',')
                    .map(str::trim)
                    .filter(|feature| !feature.is_empty())
                    .collect();

                let lean_boot = requested
                    .iter()
                    .any(|feature| matches!(*feature, "boot_minimal" | "uefi_boot"));
                if lean_boot {
                    cmd.arg("--no-default-features");
                }

                let mut effective_features = features.to_string();
                if requested.contains(&"uefi_boot") && !requested.contains(&"boot_minimal") {
                    effective_features.push_str(",boot_minimal");
                }

                cmd.arg("--features").arg(effective_features);
            },
            None => {
                // Keep the default developer boot path on the known-good
                // first-stage profile while the full kernel module graph is
                // still being stabilised.
                cmd.arg("--no-default-features");
                cmd.arg("--features").arg("boot_minimal");
            },
        }
    }

    Ok(cmd)
}

fn build_kernel(root: &Path, opts: &BuildOpts) -> Result<()> {
    let mut cmd = kernel_cargo_command(root, opts, "build")?;
    run(&mut cmd)
}

fn check_kernel(root: &Path, opts: &BuildOpts) -> Result<()> {
    let mut cmd = kernel_cargo_command(root, opts, "check")?;
    run(&mut cmd)
}

fn ci_local(root: &Path) -> Result<()> {
    log("ci-local: checking canonical boot-minimal x86_64 build");
    let opts = BuildOpts::default();
    check_kernel(root, &opts)?;

    log("ci-local: checking module hygiene");
    lint_modules(root)?;

    log("ci-local: checking documented stub guards");
    run(Command::new("bash").arg(root.join("scripts/ci/check-stubs.sh")))?;

    log("ci-local: validating roadmap documents");
    validate_roadmap_docs(root)?;
    Ok(())
}

fn validate_file_contains(root: &Path, relative: &str, required: &[&str]) -> Result<()> {
    let path = root.join(relative);
    let content = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let missing = required
        .iter()
        .filter(|needle| !content.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        log(format!("{relative}: OK"));
        Ok(())
    } else {
        bail!(
            "{relative} is missing required topic(s): {}",
            missing.join(", ")
        )
    }
}

fn validate_roadmap_docs(root: &Path) -> Result<()> {
    validate_status_doc(root)?;
    validate_file_contains(
        root,
        "docs/syscalls.md",
        &[
            "write",
            "open",
            "close",
            "fork",
            "execve",
            "exit",
            "wait4",
            "EFAULT-safe",
        ],
    )?;
    validate_file_contains(
        root,
        "docs/milestones.md",
        &[
            "M1",
            "M2",
            "M3",
            "M4",
            "M5",
            "BOOT_MINIMAL_OK",
            "FULL_OS_USERSPACE_OK",
        ],
    )?;
    validate_file_contains(
        root,
        "docs/architecture.md",
        &[
            "Primary architecture: x86_64",
            "Secondary architecture: aarch64",
            "Code organisation rules",
        ],
    )?;
    validate_file_contains(
        root,
        "docs/fault_inject.md",
        &[
            "FAULT_PMM_ALLOC",
            "FAULT_VMM_MAP",
            "FAULT_SYSCALL_RESOURCE",
            "fault-inject",
        ],
    )?;
    Ok(())
}

fn validate_status_doc(root: &Path) -> Result<()> {
    validate_file_contains(
        root,
        "docs/status.md",
        &[
            "boot_minimal",
            "userspace_boot",
            "syscall",
            "fault-inject",
            "Wayland",
            "x86_64",
            "aarch64",
        ],
    )
}

fn stage_esp(root: &Path, opts: &BuildOpts) -> Result<PathBuf> {
    let boot_dir = esp_boot_dir(root, opts.arch);
    fs::create_dir_all(&boot_dir)
        .with_context(|| format!("create ESP boot dir {}", boot_dir.display()))?;

    let artifact = artifact_path(root, opts)
        .ok_or_else(|| anyhow!("kernel artifact not found — did `build_kernel` succeed?"))?;

    let dest = boot_dir.join(efi_boot_filename(opts.arch));
    fs::copy(&artifact, &dest)
        .with_context(|| format!("copy {} → {}", artifact.display(), dest.display()))?;

    log(format!(
        "staged {} → {}",
        artifact.display(),
        dest.display()
    ));
    Ok(root.join("target/esp").join(arch_str(opts.arch)))
}

fn build_fat_image(root: &Path, esp_dir: &Path, arch: Arch) -> Result<PathBuf> {
    let img = root.join(image_name(arch));
    if which_first(&["mformat"]).is_none() || which_first(&["mcopy"]).is_none() {
        log("mtools not found; using built-in FAT16 ESP writer");
        return build_fat_image_builtin(&img, esp_dir, arch);
    }

    // 64 MiB FAT image
    let size_kb = 65536u32;
    run(Command::new("mformat")
        .args(["-i", img.to_str().unwrap(), "-C", "-F"])
        .args(["-T", &size_kb.to_string()])
        .args(["::"]))?;

    // Copy entire EFI/ tree
    let efi_src = esp_dir.join("EFI");
    run(Command::new("mcopy")
        .args(["-i", img.to_str().unwrap(), "-s"])
        .arg(&efi_src)
        .arg("::"))?;

    log(format!("built FAT image: {}", img.display()));
    Ok(img)
}

fn build_fat_image_builtin(img: &Path, esp_dir: &Path, arch: Arch) -> Result<PathBuf> {
    const BYTES_PER_SECTOR: usize = 512;
    const SECTORS_PER_CLUSTER: usize = 4;
    const RESERVED_SECTORS: usize = 1;
    const FAT_COUNT: usize = 2;
    const ROOT_ENTRIES: usize = 512;
    const ROOT_DIR_SECTORS: usize = ROOT_ENTRIES * 32 / BYTES_PER_SECTOR;
    const SECTORS_PER_FAT: usize = 256;
    const TOTAL_SECTORS: usize = 131_072; // 64 MiB
    const CLUSTER_SIZE: usize = BYTES_PER_SECTOR * SECTORS_PER_CLUSTER;

    let efi_path = esp_dir.join("EFI/BOOT").join(efi_boot_filename(arch));
    let efi = fs::read(&efi_path)
        .with_context(|| format!("read staged EFI binary {}", efi_path.display()))?;
    let file_clusters = efi.len().div_ceil(CLUSTER_SIZE);
    if file_clusters == 0 {
        bail!("staged EFI binary is empty: {}", efi_path.display());
    }

    let first_data_sector = RESERVED_SECTORS + (FAT_COUNT * SECTORS_PER_FAT) + ROOT_DIR_SECTORS;
    let mut image = vec![0u8; TOTAL_SECTORS * BYTES_PER_SECTOR];

    // BIOS Parameter Block for a FAT16 ESP-style image.
    image[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    image[3..11].copy_from_slice(b"RUSTOS  ");
    put_u16(&mut image, 11, BYTES_PER_SECTOR as u16);
    image[13] = SECTORS_PER_CLUSTER as u8;
    put_u16(&mut image, 14, RESERVED_SECTORS as u16);
    image[16] = FAT_COUNT as u8;
    put_u16(&mut image, 17, ROOT_ENTRIES as u16);
    put_u16(&mut image, 19, 0);
    image[21] = 0xF8;
    put_u16(&mut image, 22, SECTORS_PER_FAT as u16);
    put_u16(&mut image, 24, 32);
    put_u16(&mut image, 26, 64);
    put_u32(&mut image, 28, 0);
    put_u32(&mut image, 32, TOTAL_SECTORS as u32);
    image[36] = 0x80;
    image[38] = 0x29;
    put_u32(&mut image, 39, 0x5255_5354);
    image[43..54].copy_from_slice(b"RUSTOS ESP ");
    image[54..62].copy_from_slice(b"FAT16   ");
    image[510] = 0x55;
    image[511] = 0xAA;

    let file_start_cluster = 4usize;
    let last_file_cluster = file_start_cluster + file_clusters - 1;
    for fat_index in 0..FAT_COUNT {
        let fat_base = (RESERVED_SECTORS + fat_index * SECTORS_PER_FAT) * BYTES_PER_SECTOR;
        put_u16(&mut image, fat_base, 0xFFF8);
        put_u16(&mut image, fat_base + 2, 0xFFFF);
        put_u16(&mut image, fat_base + 2 * 2, 0xFFFF); // EFI directory
        put_u16(&mut image, fat_base + 3 * 2, 0xFFFF); // BOOT directory
        for cluster in file_start_cluster..=last_file_cluster {
            let next = if cluster == last_file_cluster {
                0xFFFF
            } else {
                (cluster + 1) as u16
            };
            put_u16(&mut image, fat_base + cluster * 2, next);
        }
    }

    let root_dir = (RESERVED_SECTORS + FAT_COUNT * SECTORS_PER_FAT) * BYTES_PER_SECTOR;
    write_dir_entry(
        &mut image[root_dir..root_dir + 32],
        b"EFI     ",
        b"   ",
        0x10,
        2,
        0,
    );

    let efi_dir = cluster_offset(first_data_sector, 2);
    write_dir_entry(
        &mut image[efi_dir..efi_dir + 32],
        b".       ",
        b"   ",
        0x10,
        2,
        0,
    );
    write_dir_entry(
        &mut image[efi_dir + 32..efi_dir + 64],
        b"..      ",
        b"   ",
        0x10,
        0,
        0,
    );
    write_dir_entry(
        &mut image[efi_dir + 64..efi_dir + 96],
        b"BOOT    ",
        b"   ",
        0x10,
        3,
        0,
    );

    let boot_dir = cluster_offset(first_data_sector, 3);
    write_dir_entry(
        &mut image[boot_dir..boot_dir + 32],
        b".       ",
        b"   ",
        0x10,
        3,
        0,
    );
    write_dir_entry(
        &mut image[boot_dir + 32..boot_dir + 64],
        b"..      ",
        b"   ",
        0x10,
        2,
        0,
    );
    write_dir_entry(
        &mut image[boot_dir + 64..boot_dir + 96],
        match arch {
            Arch::AArch64 => b"BOOTAA64",
            Arch::X86_64 => b"BOOTX64 ",
        },
        b"EFI",
        0x20,
        file_start_cluster as u16,
        efi.len() as u32,
    );

    let file_offset = cluster_offset(first_data_sector, file_start_cluster);
    image[file_offset..file_offset + efi.len()].copy_from_slice(&efi);

    fs::write(img, image).with_context(|| format!("write FAT image {}", img.display()))?;
    log(format!("built FAT image: {}", img.display()));
    Ok(img.to_path_buf())
}

fn cluster_offset(first_data_sector: usize, cluster: usize) -> usize {
    const BYTES_PER_SECTOR: usize = 512;
    const SECTORS_PER_CLUSTER: usize = 4;
    (first_data_sector + (cluster - 2) * SECTORS_PER_CLUSTER) * BYTES_PER_SECTOR
}

fn write_dir_entry(
    entry: &mut [u8],
    name: &[u8; 8],
    ext: &[u8; 3],
    attr: u8,
    first_cluster: u16,
    size: u32,
) {
    entry[..8].copy_from_slice(name);
    entry[8..11].copy_from_slice(ext);
    entry[11] = attr;
    entry[26..28].copy_from_slice(&first_cluster.to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

// ── OVMF firmware resolution (x86_64) ─────────────────────────────────────

fn find_system_ovmf() -> Option<PathBuf> {
    OVMF_SYSTEM_CANDIDATES
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(PathBuf::from)
}

fn ovmf_cache_path(root: &Path) -> PathBuf {
    root.join(".ovmf/OVMF_CODE.fd")
}

fn ensure_ovmf(root: &Path) -> Result<PathBuf> {
    if let Ok(env_path) = env::var("OVMF_CODE") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
        log(format!(
            "OVMF_CODE={env_path} does not exist — searching system paths"
        ));
    }

    if let Some(p) = find_system_ovmf() {
        log(format!("found system OVMF: {}", p.display()));
        return Ok(p);
    }

    let cache = ovmf_cache_path(root);
    if cache.exists() {
        log(format!("using cached OVMF: {}", cache.display()));
        return Ok(cache);
    }

    // Download
    log("OVMF not found — downloading from Fedora mirrors (one-time)...");
    let rpm = root.join(".ovmf/ovmf.rpm");
    fs::create_dir_all(root.join(".ovmf")).context("create .ovmf dir")?;

    run(Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&rpm)
        .arg(OVMF_DOWNLOAD_URL))?;

    // Extract the .fd file from the RPM (requires rpm2cpio + cpio, or bsdtar)
    if which_first(&["rpm2cpio"]).is_some() {
        run(Command::new("sh")
            .current_dir(root.join(".ovmf"))
            .arg("-c")
            .arg("rpm2cpio ovmf.rpm | cpio -idm --quiet"))?;
        // Locate OVMF_CODE.fd inside the extracted tree
        let extracted = root.join(".ovmf/usr/share/edk2/x64/OVMF_CODE.fd");
        if extracted.exists() {
            fs::rename(&extracted, &cache).context("move OVMF_CODE.fd")?;
            return Ok(cache);
        }
    }

    bail!(
        "Could not extract OVMF from RPM.\n\
         Install OVMF via your package manager, or set OVMF_CODE=/path/to/OVMF_CODE.fd"
    )
}

// ── QEMU launch ────────────────────────────────────────────────────────────

fn launch_qemu_x86_64(
    _root: &Path,
    img: &Path,
    ovmf: &Path,
    initrd: Option<&Path>,
    debug_port: Option<u16>,
) -> Result<()> {
    require_tool(&["qemu-system-x86_64"], "apt install qemu-system-x86");

    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args([
        "-machine",
        "q35",
        "-cpu",
        "qemu64,+xsave,+avx",
        "-m",
        "256M",
        "-drive",
        &format!("if=pflash,format=raw,readonly=on,file={}", ovmf.display()),
        "-drive",
        &format!("if=virtio,format=raw,file={}", img.display()),
        "-serial",
        "stdio",
        "-display",
        "none",
        "-no-reboot",
        "-no-shutdown",
    ]);

    if let Some(initrd) = initrd {
        cmd.arg("-initrd").arg(initrd);
    }

    if let Some(port) = debug_port {
        cmd.args(["-s", "-S", "-gdb", &format!("tcp::{port}")]);
    }

    run(&mut cmd)
}

fn launch_qemu_aarch64(
    _root: &Path,
    img: &Path,
    initrd: Option<&Path>,
    debug_port: Option<u16>,
) -> Result<()> {
    require_tool(&["qemu-system-aarch64"], "apt install qemu-system-arm");

    // Locate AArch64 UEFI firmware
    let fw_candidates = [
        "/usr/share/AAVMF/AAVMF_CODE.fd",
        "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
        "/usr/share/qemu/edk2-aarch64-code.fd",
    ];
    let fw = fw_candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(PathBuf::from)
        .or_else(|| env::var("QEMU_EFI").ok().map(PathBuf::from))
        .ok_or_else(|| {
            anyhow!(
                "AArch64 UEFI firmware not found.\n\
                 Install with: apt install qemu-efi-aarch64\n\
                 or set QEMU_EFI=/path/to/QEMU_EFI.fd"
            )
        })?;

    let mut cmd = Command::new("qemu-system-aarch64");
    cmd.args([
        "-machine",
        "virt",
        "-cpu",
        "cortex-a57",
        "-m",
        "512M",
        "-bios",
        fw.to_str().unwrap(),
        "-drive",
        &format!("if=none,id=esp,format=raw,file={}", img.display()),
        "-device",
        "virtio-blk-device,drive=esp",
        "-serial",
        "stdio",
        "-display",
        "none",
        "-no-reboot",
        "-no-shutdown",
    ]);

    if let Some(initrd) = initrd {
        cmd.arg("-initrd").arg(initrd);
    }

    if let Some(port) = debug_port {
        cmd.args(["-s", "-S", "-gdb", &format!("tcp::{port}")]);
    }

    run(&mut cmd)
}

fn smoke_marker_regex() -> &'static str {
    "BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|entering cpu_idle"
}

fn run_smoke(root: &Path, opts: &BuildOpts) -> Result<()> {
    let initrd = ensure_initrd(root, opts)?;
    build_kernel(root, opts)?;
    let esp = stage_esp(root, opts)?;
    let img = build_fat_image(root, &esp, opts.arch)?;
    let log_path = root.join(format!("target/smoke-{}.log", arch_str(opts.arch)));
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    match opts.arch {
        Arch::X86_64 => {
            require_tool(&["qemu-system-x86_64"], "apt install qemu-system-x86");
            let ovmf = ensure_ovmf(root)?;
            let mut cmd = Command::new("timeout");
            cmd.arg("60").arg("qemu-system-x86_64").args([
                "-machine",
                "q35",
                "-cpu",
                "qemu64,+xsave,+avx",
                "-m",
                "256M",
                "-drive",
                &format!("if=pflash,format=raw,readonly=on,file={}", ovmf.display()),
                "-drive",
                &format!("if=virtio,format=raw,file={}", img.display()),
                "-serial",
                &format!("file:{}", log_path.display()),
                "-display",
                "none",
                "-no-reboot",
                "-no-shutdown",
            ]);
            if let Some(initrd) = initrd.as_deref() {
                cmd.arg("-initrd").arg(initrd);
            }
            run_allow_timeout(&mut cmd)?;
        },
        Arch::AArch64 => {
            require_tool(&["qemu-system-aarch64"], "apt install qemu-system-arm");
            let fw_candidates = [
                "/usr/share/AAVMF/AAVMF_CODE.fd",
                "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
                "/usr/share/qemu/edk2-aarch64-code.fd",
            ];
            let fw = fw_candidates
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(PathBuf::from)
                .or_else(|| env::var("QEMU_EFI").ok().map(PathBuf::from))
                .ok_or_else(|| {
                    anyhow!(
                        "AArch64 UEFI firmware not found; install qemu-efi-aarch64 or set QEMU_EFI"
                    )
                })?;
            let mut cmd = Command::new("timeout");
            cmd.arg("45").arg("qemu-system-aarch64").args([
                "-machine",
                "virt",
                "-cpu",
                "cortex-a57",
                "-m",
                "512M",
                "-bios",
                fw.to_str().unwrap(),
                "-drive",
                &format!("if=none,id=esp,format=raw,file={}", img.display()),
                "-device",
                "virtio-blk-device,drive=esp",
                "-serial",
                &format!("file:{}", log_path.display()),
                "-display",
                "none",
                "-no-reboot",
                "-no-shutdown",
            ]);
            if let Some(initrd) = initrd.as_deref() {
                cmd.arg("-initrd").arg(initrd);
            }
            run_allow_timeout(&mut cmd)?;
        },
    }

    let serial = fs::read_to_string(&log_path)
        .with_context(|| format!("read smoke log {}", log_path.display()))?;
    if serial.contains("BOOT_MINIMAL_OK")
        || serial.contains("FULL_OS_USERSPACE_OK")
        || serial.contains("entering cpu_idle")
    {
        log(format!("smoke marker found in {}", log_path.display()));
        Ok(())
    } else {
        bail!(
            "smoke marker not found in {}; expected {}",
            log_path.display(),
            smoke_marker_regex()
        )
    }
}

fn run_allow_timeout(cmd: &mut Command) -> Result<()> {
    log(format!("running: {:?}", cmd));
    let status = cmd.status().context("failed to spawn command")?;
    if status.success() || status.code() == Some(124) {
        Ok(())
    } else {
        bail!("command failed with {status}")
    }
}

// ── initramfs / userspace build ────────────────────────────────────────────

fn build_init(root: &Path, arch: Arch) -> Result<()> {
    let target = match arch {
        Arch::X86_64 => "x86_64-unknown-none",
        Arch::AArch64 => "aarch64-unknown-none",
    };

    // Ensure the freestanding userspace target is installed.
    run(Command::new("rustup").args(["target", "add", target]))?;

    // Build userspace/init
    let mut cmd = cargo();
    cmd.current_dir(root.join("userspace/init"))
        .args(["build", "--release", "--target", target]);
    run(&mut cmd)?;

    // Stage as /init in a minimal initramfs tree
    let init_src = root
        .join("userspace/init/target")
        .join(target)
        .join("release/init");
    let initramfs_root = root.join("initramfs_root");
    fs::create_dir_all(&initramfs_root).context("create initramfs_root")?;
    let init_dst = initramfs_root.join("init");
    fs::copy(&init_src, &init_dst)
        .with_context(|| format!("copy {} → {}", init_src.display(), init_dst.display()))?;

    // Write a minimal /etc/os-release
    let etc = initramfs_root.join("etc");
    fs::create_dir_all(&etc).context("create initramfs_root/etc")?;
    fs::write(etc.join("os-release"), OS_RELEASE_CONTENT).context("write os-release")?;

    // Pack initramfs.cpio
    let cpio_out = root.join("initramfs.cpio");
    pack_cpio_newc(&initramfs_root, &cpio_out)?;

    log(format!("initramfs packed: {}", cpio_out.display()));
    Ok(())
}

fn pack_cpio_newc(root: &Path, out: &Path) -> Result<()> {
    let mut entries = Vec::new();
    collect_initramfs_entries(root, root, &mut entries)?;
    entries.sort();

    let mut archive = Vec::new();
    for rel in entries {
        let path = root.join(&rel);
        let meta = fs::metadata(&path).with_context(|| format!("metadata {}", path.display()))?;
        let name = rel.to_string_lossy().replace('\\', "/");
        let mode = meta.permissions().mode();
        if meta.is_dir() {
            write_cpio_entry(&mut archive, &name, mode | 0o040000, &[])?;
        } else if meta.is_file() {
            let data = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            write_cpio_entry(&mut archive, &name, mode | 0o100000, &data)?;
        }
    }
    write_cpio_entry(&mut archive, "TRAILER!!!", 0, &[])?;
    fs::write(out, archive).with_context(|| format!("write {}", out.display()))?;
    Ok(())
}

fn collect_initramfs_entries(root: &Path, dir: &Path, entries: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", dir.display()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("strip prefix {}", path.display()))?
            .to_path_buf();
        entries.push(rel.clone());
        if entry
            .file_type()
            .with_context(|| format!("file type {}", path.display()))?
            .is_dir()
        {
            collect_initramfs_entries(root, &path, entries)?;
        }
    }
    Ok(())
}

fn write_cpio_entry(archive: &mut Vec<u8>, name: &str, mode: u32, data: &[u8]) -> Result<()> {
    let namesize = name
        .len()
        .checked_add(1)
        .ok_or_else(|| anyhow!("cpio name too long"))?;
    let filesize = data.len();
    let header = format!(
        "070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{devmajor:08x}{devminor:08x}{rdevmajor:08x}{rdevminor:08x}{namesize:08x}{check:08x}",
        ino = 0u32,
        mode = mode,
        uid = 0u32,
        gid = 0u32,
        nlink = 1u32,
        mtime = 0u32,
        filesize = filesize,
        devmajor = 0u32,
        devminor = 0u32,
        rdevmajor = 0u32,
        rdevminor = 0u32,
        namesize = namesize,
        check = 0u32,
    );
    archive.extend_from_slice(header.as_bytes());
    archive.extend_from_slice(name.as_bytes());
    archive.push(0);
    pad_to_4(archive);
    archive.extend_from_slice(data);
    pad_to_4(archive);
    Ok(())
}

fn pad_to_4(bytes: &mut Vec<u8>) {
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
}

// ── lint-modules subcommand ────────────────────────────────────────────────

fn lint_modules(root: &Path) -> Result<()> {
    // Walk src/ looking for files that use `use super::` across module
    // boundaries (a canary for circular deps or incorrect visibility).
    // This is intentionally lightweight — a full linter would use syn.
    let src = root.join("src");
    let mut violations = Vec::new();

    fn walk(dir: &Path, violations: &mut Vec<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, violations)?;
            } else if path.extension().map_or(false, |e| e == "rs") {
                let content = fs::read_to_string(&path)?;
                for (i, line) in content.lines().enumerate() {
                    // Flag `pub(crate) use` that crosses a top-level module
                    // boundary — a heuristic only.
                    if line.contains("pub(crate) use crate::") && !line.contains(" as sys_") {
                        violations.push(format!(
                            "{}:{}: suspicious pub(crate) use across crate root",
                            path.display(),
                            i + 1
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    walk(&src, &mut violations).context("walking src/")?;

    if violations.is_empty() {
        log("lint-modules: OK");
        Ok(())
    } else {
        for v in &violations {
            eprintln!("[xtask] lint: {v}");
        }
        bail!("{} module hygiene violation(s)", violations.len());
    }
}

// ── FAT helpers (used by build_fat_image) ──────────────────────────────────

// ── main ────────────────────────────────────────────────────────────────────

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_help();
        return;
    }

    let subcommand = args.remove(0);
    let root = workspace_root();

    match subcommand.as_str() {
        "build" => {
            let opts = parse_build_args(&args);
            if let Err(e) = build_kernel(&root, &opts) {
                eprintln!("[xtask] build failed: {e:#}");
                exit(1);
            }
        },

        "check" => {
            let opts = parse_build_args(&args);
            if let Err(e) = check_kernel(&root, &opts) {
                eprintln!("[xtask] check failed: {e:#}");
                exit(1);
            }
        },

        "image" => {
            let opts = parse_build_args(&args);
            if let Err(e) = (|| -> Result<()> {
                build_kernel(&root, &opts)?;
                let esp = stage_esp(&root, &opts)?;
                build_fat_image(&root, &esp, opts.arch)?;
                Ok(())
            })() {
                eprintln!("[xtask] image failed: {e:#}");
                exit(1);
            }
        },

        "run" => {
            let opts = parse_build_args(&args);
            if let Err(e) = (|| -> Result<()> {
                let initrd = ensure_initrd(&root, &opts)?;
                build_kernel(&root, &opts)?;
                let esp = stage_esp(&root, &opts)?;
                let img = build_fat_image(&root, &esp, opts.arch)?;
                match opts.arch {
                    Arch::X86_64 => {
                        let ovmf = ensure_ovmf(&root)?;
                        launch_qemu_x86_64(&root, &img, &ovmf, initrd.as_deref(), None)?;
                    },
                    Arch::AArch64 => {
                        launch_qemu_aarch64(&root, &img, initrd.as_deref(), None)?;
                    },
                }
                Ok(())
            })() {
                eprintln!("[xtask] run failed: {e:#}");
                exit(1);
            }
        },

        "smoke" => {
            let opts = parse_build_args(&args);
            if let Err(e) = run_smoke(&root, &opts) {
                eprintln!("[xtask] smoke failed: {e:#}");
                exit(1);
            }
        },

        "debug" => {
            let opts = parse_build_args(&args);
            if let Err(e) = (|| -> Result<()> {
                let initrd = ensure_initrd(&root, &opts)?;
                build_kernel(&root, &opts)?;
                let esp = stage_esp(&root, &opts)?;
                let img = build_fat_image(&root, &esp, opts.arch)?;
                match opts.arch {
                    Arch::X86_64 => {
                        let ovmf = ensure_ovmf(&root)?;
                        launch_qemu_x86_64(&root, &img, &ovmf, initrd.as_deref(), Some(1234))?;
                    },
                    Arch::AArch64 => {
                        launch_qemu_aarch64(&root, &img, initrd.as_deref(), Some(1234))?;
                    },
                }
                Ok(())
            })() {
                eprintln!("[xtask] debug failed: {e:#}");
                exit(1);
            }
        },

        "build-init" => {
            let arch = args
                .iter()
                .position(|a| a == "--arch")
                .and_then(|i| args.get(i + 1))
                .map(|a| match a.as_str() {
                    "aarch64" => Arch::AArch64,
                    "x86_64" => Arch::X86_64,
                    other => {
                        eprintln!("[xtask] unknown --arch: {other}");
                        exit(1);
                    },
                })
                .unwrap_or(Arch::X86_64);

            if let Err(e) = build_init(&root, arch) {
                eprintln!("[xtask] build-init failed: {e:#}");
                exit(1);
            }
        },

        "ci-local" => {
            if let Err(e) = ci_local(&root) {
                eprintln!("[xtask] ci-local failed: {e:#}");
                exit(1);
            }
        },

        "status-check" => {
            if let Err(e) = validate_status_doc(&root) {
                eprintln!("[xtask] status-check failed: {e:#}");
                exit(1);
            }
        },

        "roadmap-check" => {
            if let Err(e) = validate_roadmap_docs(&root) {
                eprintln!("[xtask] roadmap-check failed: {e:#}");
                exit(1);
            }
        },

        "lint-modules" => {
            if let Err(e) = lint_modules(&root) {
                eprintln!("[xtask] lint-modules failed: {e:#}");
                exit(1);
            }
        },

        "help" | "--help" | "-h" => print_help(),

        other => {
            eprintln!("[xtask] unknown subcommand: {other}");
            print_help();
            exit(1);
        },
    }
}

fn print_help() {
    eprintln!(
        r#"cargo xtask — RustOS build automation

SUBCOMMANDS:
  build    [--arch <arch>] [--boot <boot>] [--profile <p>] [--features <f>]
             Compile the kernel only.

  check    [--arch <arch>] [--boot <boot>] [--profile <p>] [--features <f>]
             Type-check the kernel using the same target/feature handling as build.

  image    [--arch <arch>] [--boot <boot>] [--profile <p>] [--features <f>]
             Build kernel + stage ESP + assemble FAT disk image.

  run      [--arch <arch>] [--boot <boot>] [--debug] [--initrd]
             Build, image, and launch under QEMU.

  smoke   [--arch <arch>] [--boot <boot>] [--profile <p>] [--features <f>] [--initrd]
             Build, boot under QEMU with serial captured, and assert a boot marker.

  debug    [--arch <arch>] [--boot <boot>] [--debug]
             Like `run` but starts QEMU with GDB server on tcp::1234.

  build-init [--arch <arch>]
             Build userspace/init and pack initramfs.cpio.

  lint-modules
             Check module hygiene (pub(crate) use heuristics).

  status-check
             Validate docs/status.md covers required roadmap topics.

  roadmap-check
             Validate status, syscall, milestone, architecture, and fault docs.

  ci-local
             Run the fast local CI gate: check, lint-modules, stub guard, roadmap-check.

  help       Print this message.

ARCHITECTURES (--arch):
  aarch64   AArch64 — UEFI (default boot) or baremetal
  x86_64    x86-64  — UEFI only

BOOT MODES (--boot):
  uefi        UEFI PE/COFF image (default)
  baremetal   Bare-metal ELF (aarch64 only)

PROFILES (--profile):
  dev / debug    Debug build with full symbols (default)
  release        Optimised release build
  release-boot   Lean boot image profile

ENVIRONMENT:
  OVMF_CODE   Path to OVMF firmware for x86_64 UEFI boot
  QEMU_EFI    Path to AArch64 UEFI firmware (QEMU_EFI.fd / AAVMF_CODE.fd)
  CARGO       Override cargo binary (default: cargo)
  QEMU        Override QEMU binary
"#
    );
}

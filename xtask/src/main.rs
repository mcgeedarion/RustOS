//! cargo xtask — build automation for RustOS.
//!
//! Canonical build/run contract:
//!   aarch64: uefi | baremetal
//!   riscv64: uefi | sbi
//!   x86_64:  uefi
//!
//! Canonical ESP staging path:
//!   target/esp/<arch>/EFI/BOOT/BOOT*.EFI

use anyhow::{anyhow, bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{exit, Command},
};

const OS_RELEASE_CONTENT: &[u8] =
    b"NAME=RustOS\nID=rustos\nVERSION=0.1.0\nPRETTY_NAME=\"RustOS 0.1.0\"\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arch {
    AArch64,
    RiscV64,
    X86_64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boot {
    Uefi,
    Sbi,
    Baremetal,
}

#[derive(Debug, Clone)]
struct BuildOpts {
    arch: Arch,
    boot: Boot,
    debug: bool,
    initrd: bool,
    features: Option<String>,
}

impl Default for BuildOpts {
    fn default() -> Self {
        Self {
            arch: Arch::X86_64,
            boot: Boot::Uefi,
            debug: false,
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
        Arch::RiscV64 => "riscv64",
        Arch::X86_64 => "x86_64",
    }
}

fn boot_str(boot: Boot) -> &'static str {
    match boot {
        Boot::Uefi => "uefi",
        Boot::Sbi => "sbi",
        Boot::Baremetal => "baremetal",
    }
}

fn validate_contract(arch: Arch, boot: Boot) -> Result<()> {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi | Boot::Baremetal) => Ok(()),
        (Arch::RiscV64, Boot::Uefi | Boot::Sbi) => Ok(()),
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
        (Arch::RiscV64, Boot::Uefi) => root.join("targets/riscv64-uefi-loader.json"),
        (Arch::RiscV64, Boot::Sbi) => PathBuf::from("riscv64gc-unknown-none-elf"),
        // Use the upstream built-in target. The custom JSON spec (with
        // `is-like-windows`/`is-like-msvc`) triggers `compiler_builtins`
        // assembly errors under current nightly.
        (Arch::X86_64, Boot::Uefi) => PathBuf::from("x86_64-unknown-uefi"),
        _ => unreachable!("validate_contract must run before target_json"),
    }
}

fn target_dir_name(arch: Arch, boot: Boot) -> &'static str {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi) => "aarch64-unknown-uefi",
        (Arch::AArch64, Boot::Baremetal) => "aarch64-kernel",
        (Arch::RiscV64, Boot::Uefi) => "riscv64-uefi-loader",
        (Arch::RiscV64, Boot::Sbi) => "riscv64gc-unknown-none-elf",
        (Arch::X86_64, Boot::Uefi) => "x86_64-unknown-uefi",
        _ => unreachable!("validate_contract must run before target_dir_name"),
    }
}

fn efi_boot_filename(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "BOOTAA64.EFI",
        Arch::RiscV64 => "BOOTRISCV64.EFI",
        Arch::X86_64 => "BOOTX64.EFI",
    }
}

fn image_name(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "boot-aarch64.img",
        Arch::RiscV64 => "boot-riscv64.img",
        Arch::X86_64 => "boot-x86_64.img",
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has no parent directory")
        .to_path_buf()
}

fn profile(opts: &BuildOpts) -> &'static str {
    if opts.debug {
        "debug"
    } else {
        "release"
    }
}

fn build_output_path(root: &Path, opts: &BuildOpts) -> PathBuf {
    root.join("target")
        .join(target_dir_name(opts.arch, opts.boot))
        .join(profile(opts))
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

fn parse_build_args(args: &[String]) -> BuildOpts {
    let mut opts = BuildOpts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--arch" => {
                i += 1;
                opts.arch = match args.get(i).map(String::as_str) {
                    Some("aarch64") => Arch::AArch64,
                    Some("riscv64") => Arch::RiscV64,
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
                    Some("sbi") => Boot::Sbi,
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
            "--debug" => opts.debug = true,
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

fn build_kernel(root: &Path, opts: &BuildOpts) -> Result<()> {
    validate_contract(opts.arch, opts.boot)?;

    let mut cmd = cargo();
    cmd.current_dir(root)
        .args(["build", "--target"])
        .arg(target_json(root, opts.arch, opts.boot));
    add_build_std_flags(&mut cmd);
    if !opts.debug {
        cmd.arg("--release");
    }
    match &opts.features {
        Some(features) => {
            if features
                .split(',')
                .any(|feature| feature.trim() == "boot_minimal")
            {
                cmd.arg("--no-default-features");
            }
            cmd.arg("--features").arg(features);
        },
        None if opts.boot == Boot::Uefi => {
            cmd.arg("--features").arg("uefi_boot");
        },
        None => {},
    }
    run(&mut cmd)?;

    if opts.boot == Boot::Uefi {
        install_efi(root, opts)?;
    }
    if opts.initrd {
        mkinitramfs(root, opts.arch)?;
    }
    log(format!(
        "built {} {} {}",
        arch_str(opts.arch),
        boot_str(opts.boot),
        profile(opts)
    ));
    Ok(())
}

fn install_efi(root: &Path, opts: &BuildOpts) -> Result<()> {
    let src = artifact_path(root, opts).with_context(|| {
        format!(
            "UEFI artifact not found under {}",
            build_output_path(root, opts).display()
        )
    })?;
    let dest_dir = esp_boot_dir(root, opts.arch);
    fs::create_dir_all(&dest_dir).context("create ESP boot directory")?;
    let dest = dest_dir.join(efi_boot_filename(opts.arch));
    fs::copy(&src, &dest).context("copy EFI artifact into ESP")?;
    log(format!("installed EFI: {}", dest.display()));
    Ok(())
}

fn require_initramfs_tools(arch: Arch) -> Result<()> {
    match arch {
        Arch::X86_64 => require_tool(&["musl-gcc"], "apt install musl-tools"),
        Arch::RiscV64 => require_tool(
            &["riscv64-linux-musl-gcc", "riscv64-unknown-linux-musl-gcc"],
            "install a riscv64 musl cross compiler",
        ),
        Arch::AArch64 => {
            bail!("aarch64 initramfs is disabled until userspace/Makefile supports ARCH=aarch64")
        },
    }
    require_tool(&["cpio"], "apt install cpio");
    require_tool(&["find"], "coreutils should provide find");
    Ok(())
}

fn mkinitramfs(root: &Path, arch: Arch) -> Result<()> {
    require_initramfs_tools(arch)?;
    let arch_name = arch_str(arch);
    let staging = root.join(format!("target/initramfs-staging-{arch_name}"));
    if staging.exists() {
        fs::remove_dir_all(&staging).context("remove old initramfs staging dir")?;
    }
    for dir in [
        "", "bin", "sbin", "usr/bin", "usr/sbin", "lib", "etc", "dev", "proc", "sys", "tmp", "run",
    ] {
        fs::create_dir_all(staging.join(dir)).context("create initramfs subdir")?;
    }
    run(Command::new("make")
        .current_dir(root.join("userspace"))
        .args([
            "-j4",
            &format!("ARCH={arch_name}"),
            &format!("DESTDIR={}", staging.display()),
            "install",
        ]))?;
    fs::write(staging.join("etc/os-release"), OS_RELEASE_CONTENT).context("write os-release")?;
    let cpio_out = root.join("initramfs.cpio");
    run(Command::new("sh").current_dir(&staging).args([
        "-c",
        &format!(
            "find . | sort | cpio --create --format=newc --quiet > {}",
            cpio_out.display()
        ),
    ]))?;
    Ok(())
}

fn image(root: &Path, opts: &BuildOpts) -> Result<()> {
    validate_contract(opts.arch, opts.boot)?;
    if opts.boot != Boot::Uefi {
        bail!("image is only supported for UEFI boots; use `cargo xtask build` for non-UEFI");
    }
    build_kernel(root, opts)?;
    let efi_name = efi_boot_filename(opts.arch);
    let efi_path = esp_boot_dir(root, opts.arch).join(efi_name);
    if !efi_path.exists() {
        bail!("EFI binary not found at {}", efi_path.display());
    }
    let img_path = root.join(image_name(opts.arch));
    if which_first(&["mformat"]).is_some()
        && which_first(&["mmd"]).is_some()
        && which_first(&["mcopy"]).is_some()
    {
        run(Command::new("mformat")
            .args(["-C", "-F", "-h", "64", "-s", "32", "-t", "64", "-i"])
            .arg(&img_path)
            .arg("::"))?;
        run(Command::new("mmd")
            .args(["-i"])
            .arg(&img_path)
            .args(["::/EFI", "::/EFI/BOOT"]))?;
        run(Command::new("mcopy")
            .args(["-i"])
            .arg(&img_path)
            .arg(&efi_path)
            .arg(format!("::/EFI/BOOT/{efi_name}")))?;
    } else {
        log("mtools not found; using built-in FAT16 ESP writer");
        write_fat16_esp(&img_path, &efi_path, efi_name)?;
    }
    log(format!("image ready: {}", img_path.display()));
    Ok(())
}

fn write_fat16_esp(img_path: &Path, efi_path: &Path, efi_name: &str) -> Result<()> {
    const BYTES_PER_SECTOR: usize = 512;
    const TOTAL_SECTORS: usize = 8192; // 4 MiB.
    const RESERVED_SECTORS: usize = 1;
    const FAT_COUNT: usize = 2;
    const ROOT_ENTRIES: usize = 512;
    const SECTORS_PER_FAT: usize = 32;
    const ROOT_DIR_SECTORS: usize = ROOT_ENTRIES * 32 / BYTES_PER_SECTOR;
    const FIRST_DATA_SECTOR: usize =
        RESERVED_SECTORS + FAT_COUNT * SECTORS_PER_FAT + ROOT_DIR_SECTORS;
    const EFI_CLUSTER: u16 = 2;
    const BOOT_CLUSTER: u16 = 3;
    const FILE_FIRST_CLUSTER: u16 = 4;

    let file = fs::read(efi_path).with_context(|| format!("read {}", efi_path.display()))?;
    let file_clusters = file.len().div_ceil(BYTES_PER_SECTOR).max(1);
    let last_file_cluster = FILE_FIRST_CLUSTER as usize + file_clusters - 1;
    let max_cluster = TOTAL_SECTORS - FIRST_DATA_SECTOR + 1;
    if last_file_cluster > max_cluster {
        bail!("EFI binary is too large for the built-in 4 MiB ESP image");
    }

    let mut img = vec![0u8; TOTAL_SECTORS * BYTES_PER_SECTOR];
    write_boot_sector(
        &mut img,
        TOTAL_SECTORS as u16,
        SECTORS_PER_FAT as u16,
        ROOT_ENTRIES as u16,
    );

    let mut fat = vec![0u16; SECTORS_PER_FAT * BYTES_PER_SECTOR / 2];
    fat[0] = 0xfff8;
    fat[1] = 0xffff;
    fat[EFI_CLUSTER as usize] = 0xffff;
    fat[BOOT_CLUSTER as usize] = 0xffff;
    for cluster in FILE_FIRST_CLUSTER as usize..=last_file_cluster {
        fat[cluster] = if cluster == last_file_cluster {
            0xffff
        } else {
            (cluster + 1) as u16
        };
    }
    for fat_index in 0..FAT_COUNT {
        let start = (RESERVED_SECTORS + fat_index * SECTORS_PER_FAT) * BYTES_PER_SECTOR;
        for (i, entry) in fat.iter().enumerate() {
            img[start + i * 2..start + i * 2 + 2].copy_from_slice(&entry.to_le_bytes());
        }
    }

    let root_start = (RESERVED_SECTORS + FAT_COUNT * SECTORS_PER_FAT) * BYTES_PER_SECTOR;
    write_dir_entry(
        &mut img[root_start..root_start + 32],
        "EFI",
        "",
        0x10,
        EFI_CLUSTER,
        0,
    )?;

    let efi_dir_start = cluster_offset(EFI_CLUSTER, FIRST_DATA_SECTOR, BYTES_PER_SECTOR);
    write_dir_entry(
        &mut img[efi_dir_start..efi_dir_start + 32],
        ".",
        "",
        0x10,
        EFI_CLUSTER,
        0,
    )?;
    write_dir_entry(
        &mut img[efi_dir_start + 32..efi_dir_start + 64],
        "..",
        "",
        0x10,
        0,
        0,
    )?;
    write_dir_entry(
        &mut img[efi_dir_start + 64..efi_dir_start + 96],
        "BOOT",
        "",
        0x10,
        BOOT_CLUSTER,
        0,
    )?;

    let boot_dir_start = cluster_offset(BOOT_CLUSTER, FIRST_DATA_SECTOR, BYTES_PER_SECTOR);
    write_dir_entry(
        &mut img[boot_dir_start..boot_dir_start + 32],
        ".",
        "",
        0x10,
        BOOT_CLUSTER,
        0,
    )?;
    write_dir_entry(
        &mut img[boot_dir_start + 32..boot_dir_start + 64],
        "..",
        "",
        0x10,
        EFI_CLUSTER,
        0,
    )?;
    write_file_dir_entries(
        &mut img[boot_dir_start + 64..boot_dir_start + 64 + 32 * 4],
        efi_name,
        FILE_FIRST_CLUSTER,
        file.len() as u32,
    )?;

    let file_start = cluster_offset(FILE_FIRST_CLUSTER, FIRST_DATA_SECTOR, BYTES_PER_SECTOR);
    img[file_start..file_start + file.len()].copy_from_slice(&file);
    fs::write(img_path, img).with_context(|| format!("write {}", img_path.display()))?;
    Ok(())
}

fn write_boot_sector(img: &mut [u8], total_sectors: u16, sectors_per_fat: u16, root_entries: u16) {
    img[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
    img[3..11].copy_from_slice(b"RUSTOS  ");
    img[11..13].copy_from_slice(&512u16.to_le_bytes());
    img[13] = 1;
    img[14..16].copy_from_slice(&1u16.to_le_bytes());
    img[16] = 2;
    img[17..19].copy_from_slice(&root_entries.to_le_bytes());
    img[19..21].copy_from_slice(&total_sectors.to_le_bytes());
    img[21] = 0xf8;
    img[22..24].copy_from_slice(&sectors_per_fat.to_le_bytes());
    img[24..26].copy_from_slice(&32u16.to_le_bytes());
    img[26..28].copy_from_slice(&64u16.to_le_bytes());
    img[36] = 0x80;
    img[38] = 0x29;
    img[39..43].copy_from_slice(&0x5255_5354u32.to_le_bytes());
    img[43..54].copy_from_slice(b"RUSTOS ESP ");
    img[54..62].copy_from_slice(b"FAT16   ");
    img[510] = 0x55;
    img[511] = 0xaa;
}

fn cluster_offset(cluster: u16, first_data_sector: usize, bytes_per_sector: usize) -> usize {
    (first_data_sector + (cluster as usize - 2)) * bytes_per_sector
}

fn write_file_dir_entries(
    entries: &mut [u8],
    filename: &str,
    first_cluster: u16,
    size: u32,
) -> Result<()> {
    if let Ok((stem, ext)) = split_83(filename) {
        return write_dir_entry(&mut entries[0..32], &stem, &ext, 0x20, first_cluster, size);
    }

    let (stem, ext) = short_alias(filename)?;
    let mut short = [b' '; 11];
    short[..stem.len()].copy_from_slice(stem.as_bytes());
    short[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    let checksum = lfn_checksum(&short);
    let utf16: Vec<u16> = filename.encode_utf16().collect();
    let lfn_count = utf16.len().div_ceil(13);
    if entries.len() < (lfn_count + 1) * 32 {
        bail!("not enough directory space for long filename {filename}");
    }

    for i in 0..lfn_count {
        let ordinal = lfn_count - i;
        let start = (ordinal - 1) * 13;
        let end = utf16.len().min(start + 13);
        let mut ord = ordinal as u8;
        if ordinal == lfn_count {
            ord |= 0x40;
        }
        write_lfn_entry(
            &mut entries[i * 32..i * 32 + 32],
            ord,
            &utf16[start..end],
            checksum,
        );
    }
    write_dir_entry(
        &mut entries[lfn_count * 32..lfn_count * 32 + 32],
        &stem,
        &ext,
        0x20,
        first_cluster,
        size,
    )
}

fn short_alias(filename: &str) -> Result<(String, String)> {
    let mut parts = filename.split('.');
    let stem = parts.next().unwrap_or_default();
    let ext = parts.next().unwrap_or_default();
    if parts.next().is_some() || stem.is_empty() || ext.len() > 3 {
        bail!("cannot create short alias for {filename}");
    }
    let clean: String = stem
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(6)
        .collect::<String>()
        .to_ascii_uppercase();
    if clean.is_empty() {
        bail!("cannot create short alias for {filename}");
    }
    Ok((format!("{clean}~1"), ext.to_ascii_uppercase()))
}

fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for byte in short_name {
        sum = ((sum & 1) << 7).wrapping_add(sum >> 1).wrapping_add(*byte);
    }
    sum
}

fn write_lfn_entry(entry: &mut [u8], ordinal: u8, chars: &[u16], checksum: u8) {
    entry.fill(0xff);
    entry[0] = ordinal;
    entry[11] = 0x0f;
    entry[12] = 0;
    entry[13] = checksum;
    entry[26] = 0;
    entry[27] = 0;
    let slots = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    for (i, slot) in slots.iter().enumerate() {
        let value = if i < chars.len() {
            chars[i]
        } else if i == chars.len() {
            0
        } else {
            0xffff
        };
        entry[*slot..*slot + 2].copy_from_slice(&value.to_le_bytes());
    }
}

fn split_83(name: &str) -> Result<(String, String)> {
    let mut parts = name.split('.');
    let stem = parts.next().unwrap_or_default().to_ascii_uppercase();
    let ext = parts.next().unwrap_or_default().to_ascii_uppercase();
    if parts.next().is_some() || stem.is_empty() || stem.len() > 8 || ext.len() > 3 {
        bail!("built-in ESP writer only supports 8.3 filenames; got {name}");
    }
    Ok((stem, ext))
}

fn write_dir_entry(
    entry: &mut [u8],
    name: &str,
    ext: &str,
    attr: u8,
    first_cluster: u16,
    size: u32,
) -> Result<()> {
    entry.fill(0);
    let mut raw_name = [b' '; 8];
    let mut raw_ext = [b' '; 3];
    let name_bytes = name.as_bytes();
    let ext_bytes = ext.as_bytes();
    if name_bytes.len() > 8 || ext_bytes.len() > 3 {
        bail!("invalid 8.3 directory entry: {name}.{ext}");
    }
    raw_name[..name_bytes.len()].copy_from_slice(name_bytes);
    raw_ext[..ext_bytes.len()].copy_from_slice(ext_bytes);
    entry[0..8].copy_from_slice(&raw_name);
    entry[8..11].copy_from_slice(&raw_ext);
    entry[11] = attr;
    entry[26..28].copy_from_slice(&first_cluster.to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
    Ok(())
}

fn smoke(root: &Path) -> Result<()> {
    let script = root.join("scripts/ci/run_qemu.sh");
    if !script.exists() {
        bail!("QEMU runner not found at {}", script.display());
    }
    run(Command::new(&script)
        .current_dir(root)
        .env("ARCH", "x86_64")
        .arg("--boot")
        .arg("uefi")
        .arg("--smoke"))
}

fn print_help() {
    println!(
        "cargo xtask <subcommand> [options]\n\n\
Subcommands:\n\
  build         Compile the kernel\n\
  mkinitramfs   Build userspace and pack initramfs.cpio\n\
  image         Build a FAT ESP disk image for UEFI\n\
  smoke         Run x86_64 UEFI under QEMU\n\
  help          Show this help\n\n\
Build options:\n\
  --arch <aarch64|riscv64|x86_64>\n\
  --boot <uefi|sbi|baremetal>\n\
  --features <features>\n\
  --debug\n\
  --initrd"
    );
}

fn main() {
    let mut args = env::args().skip(1);
    let subcommand = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    let root = workspace_root();
    let result = match subcommand.as_str() {
        "build" => build_kernel(&root, &parse_build_args(&rest)),
        "mkinitramfs" => {
            let opts = parse_build_args(&rest);
            mkinitramfs(&root, opts.arch)
        },
        "image" => image(&root, &parse_build_args(&rest)),
        "smoke" => smoke(&root),
        "help" | "--help" | "-h" | "" => {
            print_help();
            Ok(())
        },
        other => Err(anyhow!(
            "unknown subcommand: {other:?}. Try `cargo xtask help`."
        )),
    };
    if let Err(error) = result {
        eprintln!("[xtask] ERROR: {error:#}");
        exit(1);
    }
}

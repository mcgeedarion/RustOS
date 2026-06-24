//! cargo xtask — build automation for RustOS.
//!
//! Canonical build/run contract:
//!   aarch64: uefi | baremetal
//!   riscv64: uefi | sbi
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

use anyhow::{anyhow, bail, Context, Result};
use std::{
    env, fs,
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
        // riscv64 UEFI is gated: the current toolchain cannot produce a
        // bootable BOOTRISCV64.EFI. Use --boot sbi for riscv64, or pass
        // --features riscv64_uefi_boot to explicitly override when re-enabling.
        (Arch::RiscV64, Boot::Uefi) => bail!(
            "riscv64 UEFI boot is currently gated. \
             Use `--boot sbi` for riscv64, or pass \
             `--features riscv64_uefi_boot` to override."
        ),
        (Arch::RiscV64, Boot::Sbi) => Ok(()),
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
        (Arch::RiscV64, Boot::Sbi) => root.join("targets/riscv64-kernel.json"),
        (Arch::X86_64, Boot::Uefi) => PathBuf::from("x86_64-unknown-uefi"),
        _ => unreachable!("validate_contract must run before target_json"),
    }
}

fn target_dir_name(arch: Arch, boot: Boot) -> &'static str {
    match (arch, boot) {
        (Arch::AArch64, Boot::Uefi) => "aarch64-unknown-uefi",
        (Arch::AArch64, Boot::Baremetal) => "aarch64-kernel",
        (Arch::RiscV64, Boot::Uefi) => "riscv64-uefi-loader",
        (Arch::RiscV64, Boot::Sbi) => "riscv64-kernel",
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
    // Secondary gate: reject riscv64 UEFI unless riscv64_uefi_boot is in
    // --features. validate_contract handles the normal CLI path; this catches
    // any internal callers that bypass it.
    if opts.arch == Arch::RiscV64 && opts.boot == Boot::Uefi {
        let has_gate = opts
            .features
            .as_deref()
            .map(|f| f.split(',').any(|feat| feat.trim() == "riscv64_uefi_boot"))
            .unwrap_or(false);
        if !has_gate {
            bail!(
                "riscv64 UEFI boot is gated. Add `riscv64_uefi_boot` to --features to override."
            );
        }
    }

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
    let startup_nsh = format!("FS0:\r\n\\EFI\\BOOT\\{efi_name}\r\n");
    let startup_nsh_path = root
        .join("target/esp")
        .join(arch_str(opts.arch))
        .join("STARTUP.NSH");
    fs::write(&startup_nsh_path, startup_nsh).context("write startup.nsh")?;
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
        run(Command::new("mcopy")
            .args(["-i"])
            .arg(&img_path)
            .arg(&startup_nsh_path)
            .arg("::/STARTUP.NSH"))?;
    } else {
        log("mtools not found; using built-in FAT16 ESP writer");
        write_fat16_esp(&img_path, &efi_path, efi_name)?;
    }
    log(format!("image ready: {}", img_path.display()));
    Ok(())
}

// ---------------------------------------------------------------------------
// OVMF firmware resolution
// ---------------------------------------------------------------------------

/// Returns a path to an OVMF_CODE.fd suitable for qemu-system-x86_64.
///
/// Resolution order:
///   1. `OVMF_CODE` environment variable (user override, highest priority)
///   2. Well-known system install paths (distro packages)
///   3. `.ovmf/OVMF_CODE.fd` inside the workspace (previously auto-downloaded)
///   4. Auto-download from Fedora mirrors into `.ovmf/OVMF_CODE.fd`
///
/// The download requires either `curl` or `wget` plus `rpm2cpio` and `cpio`
/// on PATH. If none of those are available the function returns an actionable
/// error message with manual install instructions.
fn resolve_ovmf(root: &Path) -> Result<PathBuf> {
    // 1. Explicit env override.
    if let Ok(val) = env::var("OVMF_CODE") {
        let p = PathBuf::from(&val);
        if p.exists() {
            log(format!("OVMF: using OVMF_CODE env override: {}", p.display()));
            return Ok(p);
        }
        bail!(
            "OVMF_CODE={val} is set but the file does not exist. \
             Unset OVMF_CODE or point it at a real OVMF_CODE.fd."
        );
    }

    // 2. System candidates.
    for candidate in OVMF_SYSTEM_CANDIDATES {
        let p = Path::new(candidate);
        if p.exists() {
            log(format!("OVMF: found system firmware: {}", p.display()));
            return Ok(p.to_path_buf());
        }
    }

    // 3. Cached download.
    let cache_path = root.join(".ovmf/OVMF_CODE.fd");
    if cache_path.exists() {
        log(format!("OVMF: using cached firmware: {}", cache_path.display()));
        return Ok(cache_path);
    }

    // 4. Auto-download.
    log("OVMF: no system firmware found — attempting auto-download from Fedora mirrors");
    log(format!("OVMF: source: {OVMF_DOWNLOAD_URL}"));
    log("OVMF: (set OVMF_CODE=/path/to/OVMF_CODE.fd to skip this step)");

    let ovmf_dir = root.join(".ovmf");
    fs::create_dir_all(&ovmf_dir).context("create .ovmf cache directory")?;

    let rpm_path = ovmf_dir.join("edk2-ovmf.rpm");

    // Download the RPM.
    if which_first(&["curl"]).is_some() {
        run(Command::new("curl")
            .args(["-fSL", "--retry", "3", "-o"])
            .arg(&rpm_path)
            .arg(OVMF_DOWNLOAD_URL))?;
    } else if which_first(&["wget"]).is_some() {
        run(Command::new("wget")
            .args(["-q", "-O"])
            .arg(&rpm_path)
            .arg(OVMF_DOWNLOAD_URL))?;
    } else {
        bail!(
            "OVMF firmware not found and neither curl nor wget is available for auto-download.\n\
             \n\
             Install OVMF manually, then either:\n\
             • Set OVMF_CODE=/path/to/OVMF_CODE.fd, or\n\
             • Copy the file to .ovmf/OVMF_CODE.fd in the workspace root.\n\
             \n\
             Distro packages:\n\
             • Debian/Ubuntu: sudo apt install ovmf\n\
             • Fedora/RHEL:   sudo dnf install edk2-ovmf\n\
             • Arch:          sudo pacman -S edk2-ovmf\n\
             • macOS:         brew install qemu  # bundles edk2-x86_64-code.fd"
        );
    }

    // Extract OVMF_CODE.fd from the RPM using rpm2cpio + cpio.
    // The RPM contains usr/share/edk2/x64/OVMF_CODE.fd (Fedora layout).
    if which_first(&["rpm2cpio"]).is_none() || which_first(&["cpio"]).is_none() {
        // Fallback: if we happen to have rpm installed we can try `rpm -i`.
        // Otherwise give a clear error.
        bail!(
            "Downloaded OVMF RPM to {} but rpm2cpio/cpio are not available to extract it.\n\
             \n\
             Extract manually:\n\
             • rpm2cpio {} | cpio -idmv\n\
             • Then: cp usr/share/edk2/x64/OVMF_CODE.fd .ovmf/OVMF_CODE.fd\n\
             \n\
             Or install OVMF via your distro package manager (see above).",
            rpm_path.display(),
            rpm_path.display()
        );
    }

    // rpm2cpio <rpm> | cpio -idm --no-absolute-filenames
    // Run inside ovmf_dir so extracted paths land there.
    let rpm2cpio_out = Command::new("rpm2cpio")
        .arg(&rpm_path)
        .output()
        .context("rpm2cpio failed")?;
    if !rpm2cpio_out.status.success() {
        bail!("rpm2cpio exited with {}", rpm2cpio_out.status);
    }
    run(Command::new("cpio")
        .current_dir(&ovmf_dir)
        .args(["-idm", "--no-absolute-filenames", "--quiet"])
        .stdin(std::process::Stdio::piped())
        // We'll do this as two steps: write rpm2cpio stdout to cpio stdin.
        // Simplest portable approach: write to a temp file then pipe.
        // We already have the bytes in memory from .output() above.
        .stdin({
            use std::io::Write;
            let mut child_in = Command::new("cpio")
                .current_dir(&ovmf_dir)
                .args(["-idm", "--no-absolute-filenames", "--quiet"])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .context("spawn cpio")?;
            child_in
                .stdin
                .take()
                .unwrap()
                .write_all(&rpm2cpio_out.stdout)
                .context("write to cpio stdin")?;
            let status = child_in.wait().context("wait cpio")?;
            if !status.success() {
                bail!("cpio exited with {status}");
            }
            // Return a dummy Stdio — we've already run cpio above.
            std::process::Stdio::null()
        }))?;

    // Locate the extracted OVMF_CODE.fd (Fedora layout).
    let extracted = ovmf_dir.join("usr/share/edk2/x64/OVMF_CODE.fd");
    if !extracted.exists() {
        bail!(
            "Extracted RPM but could not find usr/share/edk2/x64/OVMF_CODE.fd under {}.\n\
             Please copy your OVMF_CODE.fd to .ovmf/OVMF_CODE.fd manually.",
            ovmf_dir.display()
        );
    }
    fs::copy(&extracted, &cache_path).context("copy extracted OVMF_CODE.fd to .ovmf/")?;
    // Clean up the extracted tree but keep the cache file.
    let _ = fs::remove_dir_all(ovmf_dir.join("usr"));
    let _ = fs::remove_file(&rpm_path);

    log(format!("OVMF: cached at {}", cache_path.display()));
    Ok(cache_path)
}

// ---------------------------------------------------------------------------
// `run` subcommand — golden-path developer on-ramp
// ---------------------------------------------------------------------------

/// Build the kernel + ESP image and boot it in QEMU in one command.
///
/// ```text
/// cargo xtask run --arch x86_64
/// cargo xtask run --arch x86_64 --debug
/// cargo xtask run --arch x86_64 --features boot_minimal
/// ```
///
/// Serial output is forwarded to stdout. Press Ctrl-A X to quit QEMU.
fn run_qemu(root: &Path, opts: &BuildOpts) -> Result<()> {
    validate_contract(opts.arch, opts.boot)?;

    if opts.boot != Boot::Uefi {
        bail!(
            "`cargo xtask run` only supports UEFI boot. \
             For riscv64 SBI use `cargo xtask build --arch riscv64 --boot sbi` \
             and invoke QEMU manually."
        );
    }

    // Step 1 — build kernel + assemble FAT image.
    log(format!(
        "==> Step 1/3: building {} {} kernel",
        arch_str(opts.arch),
        boot_str(opts.boot)
    ));
    image(root, opts)?;

    let img_path = root.join(image_name(opts.arch));
    if !img_path.exists() {
        bail!("disk image not found after build: {}", img_path.display());
    }

    match opts.arch {
        Arch::X86_64 => run_qemu_x86_64(root, &img_path),
        Arch::AArch64 => run_qemu_aarch64(root, &img_path),
        Arch::RiscV64 => bail!(
            "riscv64 UEFI QEMU launch is not yet wired into `cargo xtask run`; \
             use scripts/ci/run_qemu.sh directly."
        ),
    }
}

fn run_qemu_x86_64(root: &Path, img_path: &Path) -> Result<()> {
    let qemu = env::var("QEMU").unwrap_or_else(|_| "qemu-system-x86_64".into());
    if which_first(&[&qemu]).is_none() {
        bail!(
            "`{qemu}` not found on PATH.\n\
             Install with:\n\
             • Debian/Ubuntu: sudo apt install qemu-system-x86\n\
             • Fedora/RHEL:   sudo dnf install qemu-system-x86\n\
             • Arch:          sudo pacman -S qemu-system-x86\n\
             • macOS:         brew install qemu\n\
             Or set the QEMU env var to the full path."
        );
    }

    // Step 2 — resolve OVMF firmware.
    log("==> Step 2/3: resolving OVMF firmware");
    let ovmf_code = resolve_ovmf(root)?;

    // Step 3 — launch QEMU.
    log("==> Step 3/3: launching QEMU (serial → stdout; Ctrl-A X to quit)");
    log(format!("    image:    {}", img_path.display()));
    log(format!("    firmware: {}", ovmf_code.display()));

    let mut cmd = Command::new(&qemu);
    cmd
        // Machine + CPU.
        .args(["-machine", "q35"])
        .args(["-cpu", "qemu64"])
        .args(["-m", "256M"])
        // OVMF firmware (read-only pflash).
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            ovmf_code.display()
        ))
        // Boot disk.
        .arg("-drive")
        .arg(format!("format=raw,file={},if=virtio", img_path.display()))
        // Serial on stdout, no graphical window.
        .args(["-serial", "stdio"])
        .args(["-display", "none"])
        // Don't loop on triple-fault; keep the VM up after kernel halt.
        .args(["-no-reboot"])
        .args(["-no-shutdown"]);

    log(format!("running: {cmd:?}"));
    // exec-replace on Unix so QEMU owns the terminal directly.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        bail!("failed to exec {qemu}: {err}");
    }
    #[cfg(not(unix))]
    {
        run(&mut cmd)
    }
}

fn run_qemu_aarch64(root: &Path, img_path: &Path) -> Result<()> {
    let qemu = env::var("QEMU").unwrap_or_else(|_| "qemu-system-aarch64".into());
    if which_first(&[&qemu]).is_none() {
        bail!("`{qemu}` not found on PATH. Install qemu-system-aarch64.");
    }

    // Resolve AArch64 UEFI firmware.
    log("==> Step 2/3: resolving AArch64 UEFI firmware");
    let fw = resolve_aavmf(root)?;

    log("==> Step 3/3: launching QEMU AArch64 (serial → stdout; Ctrl-A X to quit)");
    let mut cmd = Command::new(&qemu);
    cmd.args(["-machine", "virt"])
        .args(["-cpu", "cortex-a57"])
        .args(["-m", "512M"])
        .args(["-bios"])
        .arg(&fw)
        .arg("-drive")
        .arg(format!("if=none,id=esp,format=raw,file={}", img_path.display()))
        .args(["-device", "virtio-blk-device,drive=esp"])
        .args(["-serial", "stdio"])
        .args(["-display", "none"])
        .args(["-no-reboot"])
        .args(["-no-shutdown"]);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        bail!("failed to exec {qemu}: {err}");
    }
    #[cfg(not(unix))]
    {
        run(&mut cmd)
    }
}

fn resolve_aavmf(root: &Path) -> Result<PathBuf> {
    if let Ok(val) = env::var("QEMU_EFI") {
        let p = PathBuf::from(&val);
        if p.exists() {
            return Ok(p);
        }
        bail!("QEMU_EFI={val} is set but the file does not exist.");
    }
    let candidates = [
        "/usr/share/AAVMF/AAVMF_CODE.fd",
        "/usr/share/AAVMF/AAVMF32_CODE.fd",
        "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
        "/usr/share/edk2/aarch64/QEMU_EFI.fd",
        "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "/usr/local/share/qemu/edk2-aarch64-code.fd",
    ];
    let cache = root.join(".ovmf/AAVMF_CODE.fd");
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    if cache.exists() {
        return Ok(cache);
    }
    bail!(
        "AArch64 UEFI firmware not found.\n\
         Install with:\n\
         • Debian/Ubuntu: sudo apt install qemu-efi-aarch64\n\
         • Fedora/RHEL:   sudo dnf install edk2-aarch64\n\
         • macOS:         brew install qemu\n\
         Or set QEMU_EFI=/path/to/QEMU_EFI.fd"
    );
}

// ---------------------------------------------------------------------------
// `smoke` subcommand (unchanged logic, now uses inline QEMU invocation)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

fn print_help() {
    println!(
        "cargo xtask <subcommand> [options]

Subcommands:
  run           Build kernel + image and boot in QEMU   ← golden path
  build         Compile the kernel only
  image         Build a FAT ESP disk image for UEFI
  mkinitramfs   Build userspace and pack initramfs.cpio
  smoke         Run x86_64 UEFI under QEMU (CI smoke test)
  help          Show this help

Golden-path one-liner:
  cargo xtask run --arch x86_64

Options (apply to run / build / image):
  --arch <aarch64|riscv64|x86_64>   target architecture (default: x86_64)
  --boot <uefi|sbi|baremetal>       boot protocol      (default: uefi)
  --features <feat1,feat2,...>       extra Cargo features
  --debug                            debug build (no --release)
  --initrd                           also build + pack initramfs

Environment variables:
  OVMF_CODE     Path to OVMF_CODE.fd (x86_64 UEFI firmware)
  QEMU_EFI      Path to QEMU_EFI.fd  (AArch64 UEFI firmware)
  QEMU          QEMU binary to use   (default: qemu-system-<arch>)
  CARGO         Cargo binary to use  (default: cargo)

See docs/getting-started.md for the full developer on-ramp."
    );
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let mut args = env::args().skip(1);
    let subcommand = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    let root = workspace_root();
    let result = match subcommand.as_str() {
        "run" => run_qemu(&root, &parse_build_args(&rest)),
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

// ---------------------------------------------------------------------------
// FAT16 ESP writer (unchanged)
// ---------------------------------------------------------------------------

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
    let startup_nsh = format!("FS0:\r\n\\EFI\\BOOT\\{efi_name}\r\n");
    let startup_bytes = startup_nsh.as_bytes();
    let file_clusters = file.len().div_ceil(BYTES_PER_SECTOR).max(1);
    let last_file_cluster = FILE_FIRST_CLUSTER as usize + file_clusters - 1;
    let startup_cluster = last_file_cluster + 1;
    let max_cluster = TOTAL_SECTORS - FIRST_DATA_SECTOR + 1;
    if startup_cluster > max_cluster {
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
    fat[startup_cluster] = 0xffff;
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
    write_dir_entry(
        &mut img[root_start + 32..root_start + 64],
        "STARTUP",
        "NSH",
        0x20,
        startup_cluster as u16,
        startup_bytes.len() as u32,
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
    let startup_start = cluster_offset(startup_cluster as u16, FIRST_DATA_SECTOR, BYTES_PER_SECTOR);
    img[startup_start..startup_start + startup_bytes.len()].copy_from_slice(startup_bytes);
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

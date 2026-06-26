{
  description = "rustos — Rust bare-metal OS (AArch64 + x86_64)";

  # IMPORTANT: keep the nightly date here in sync with:
  #   rust-toolchain.toml  (channel = "nightly-YYYY-MM-DD")
  #   Dockerfile           (ARG NIGHTLY_DATE=YYYY-MM-DD)
  # To update inputs: nix flake update
  inputs = {
    nixpkgs.url     = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url    = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        # -----------------------------------------------------------------------
        # Centralized version and toolchain date
        # -----------------------------------------------------------------------
        version = "0.2.0";
        nightlyDate = "2026-06-07";

        rustToolchain = pkgs.rust-bin.nightly.${nightlyDate}.default.override {
          extensions = [
            "rust-src"            # required by -Z build-std
            "llvm-tools-preview"  # cargo-objcopy / llvm-strip
            "rustfmt"
            "clippy"
          ];
          targets = [
            "x86_64-unknown-none"
            # UEFI/kernel JSON targets are local target specs, not rustup targets.
          ];
        };

        ovmfX86_64 = pkgs.OVMF.override {
          arches = [ "X64" ];
        };

        # Helper to resolve firmware paths by architecture
        getOvmfFirmware = { ovmf, arch }:
          if arch == "x86_64"
          then "${ovmf}/FV/OVMF.fd"
          else throw "Unknown architecture: ${arch}";

        # ---------------------------------------------------------------------------
        # Native build / dev tools
        # ---------------------------------------------------------------------------
        nativeDeps = with pkgs; [
          clang_18
          lld_18
          nasm
          qemu

          ovmfX86_64

          git
          gnumake
          python3

          # ---- development tools -------------------------------------------------------

          # cargo-binutils: rust-objdump, rust-nm, rust-size, rust-objcopy
          # Wraps LLVM tools so they respect the active Rust toolchain target.
          cargo-binutils

          # gdb-multiarch: single GDB binary with support for both x86_64 and
          # AArch64. Used with QEMU's -s -S flags for kernel source-level debugging.
          gdb-multiarch

          # mdbook: renders docs/ (Markdown) into a browseable HTML book.
          # Run:  mdbook serve docs/
          mdbook

          # nixpkgs-fmt: canonical Nix formatter; enables `nix fmt` on this flake.
          nixpkgs-fmt
        ];

        # ---------------------------------------------------------------------------
        # Common Rust package builder
        # Eliminates repetition across architecture-specific builds
        # ---------------------------------------------------------------------------
        mkRustosPackage = { pname, target, features ? "", extraBuildFlags ? "" }:
          pkgs.rustPlatform.buildRustPackage {
            inherit pname version;
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ rustToolchain ] ++ nativeDeps;

            buildPhase = ''
              cargo build --release \
                --target ${target} \
                ${pkgs.lib.optionalString (features != "") "--features ${features}"} \
                ${extraBuildFlags} \
                -Z build-std=core,alloc,compiler_builtins \
                -Z build-std-features=compiler-builtins-mem \
                -Z json-target-spec
            '';

            doCheck = false;
          };

      in {
        # ---------------------------------------------------------------------------
        # Formatter
        # ---------------------------------------------------------------------------
        formatter = pkgs.nixpkgs-fmt;

        # ---------------------------------------------------------------------------
        # Dev shell — `nix develop`
        # ---------------------------------------------------------------------------
        devShells.default = pkgs.mkShell {
          name = "rustos-dev";

          buildInputs = [ rustToolchain ] ++ nativeDeps;

          shellHook =
            let
              ovmfX86_64Path = getOvmfFirmware { ovmf = ovmfX86_64; arch = "x86_64"; };
              rustcVersion = "$(rustc --version)";
              commands = [
                "Build default boot ELF: cargo build"
                "Build x86_64 UEFI img: cargo xtask image --arch x86_64 --debug"
                "Build ARM64 UEFI:      cargo build --target targets/aarch64-uefi-loader.json"                
                "Run QEMU:              ./scripts/ci/run_qemu.sh"                "Debug (GDB):           qemu-system-x86_64 -s -S ... then gdb-multiarch"
                "Inspect binary:        rust-objdump -d target/.../rustos"
                "Kernel docs:           mdbook serve docs/"
                "Format this flake:     nix fmt"
              ];
            in
            ''
              export CARGO_TERM_COLOR=always
              export CARGO_INCREMENTAL=0


              # OVMF firmware paths — consumed by run_qemu_*.sh and xtask
              export OVMF_X86_64=${ovmfX86_64Path}

              echo ""
              echo " rustos dev shell — ${rustcVersion}"
              echo ""
              ${pkgs.lib.concatStrings (map (cmd: "  echo \"  ${cmd}\"\n") commands)}
              echo ""
            '';
        };

        # ---------------------------------------------------------------------------
        # Flake checks — `nix flake check`
        # ---------------------------------------------------------------------------
        checks.default = pkgs.runCommand "flake-check"
          {
            nativeBuildInputs = [ pkgs.nixpkgs-fmt ];
          }
          ''
            echo "Checking flake.nix format..."
            nixpkgs-fmt --check ${./flake.nix}
            echo "✓ flake.nix format is valid"
            mkdir -p $out
          '';

        # ---------------------------------------------------------------------------
        # Packages
        # ---------------------------------------------------------------------------


        # `nix build` / `nix build .#x86_64-uefi` — x86_64 UEFI image (default)
        packages.default = packages.x86_64-uefi;
        packages.x86_64-uefi = (mkRustosPackage {
          pname = "rustos-x86_64-uefi";
          target = "targets/x86_64-kernel.json";
          features = "uefi_boot";
          extraBuildFlags = "";
        }).overrideAttrs (oldAttrs: {
          installPhase = ''
            mkdir -p $out/boot
            rust-objcopy --target=efi-app-x86_64 --subsystem=10 \
              target/x86_64-kernel/release/rustos \
              $out/boot/rustos-x86_64.efi

            # Embed the correct OVMF blob alongside the image.
            cp ${getOvmfFirmware { ovmf = ovmfX86_64; arch = "x86_64"; }} $out/boot/OVMF_X86_64.fd
          '';
        });


      }
    );
}

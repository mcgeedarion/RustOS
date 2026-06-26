{
  description = "rustos — Rust bare-metal OS (AArch64 + x86_64)";

  # IMPORTANT: keep the nightly date here in sync with:
  #   rust-toolchain.toml  (channel = "nightly-YYYY-MM-DD")
  #   Dockerfile           (rustup install nightly-YYYY-MM-DD)
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # ── Rust toolchain ─────────────────────────────────────────────
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # ── OVMF firmware (UEFI) ───────────────────────────────────────
        #   AArch64 → pkgs.AAVMF
        #   x86_64  → pkgs.OVMF (ships CODE + VARS)
        ovmfX64    = pkgs.OVMF.override   { arches = [ "X64" ];      };
        ovmfAarch64 = pkgs.AAVMF.override  { arches = [ "AARCH64" ];  };

        getOvmfFirmware = arch: {
          aarch64 = { code = "${ovmfAarch64}/FV/AAVMF_CODE.fd"; vars = "${ovmfAarch64}/FV/AAVMF_VARS.fd"; };
          x86_64  = { code = "${ovmfX64}/FV/OVMF_CODE.fd";     vars = "${ovmfX64}/FV/OVMF_VARS.fd"; };
        }.${arch};

        # ── Native deps ────────────────────────────────────────────────
        nativeDeps = with pkgs; [
          # Bare-metal assembler / archiver for each supported target
          pkgsCross.aarch64-embedded.buildPackages.binutils  # aarch64-none-elf-as, -ar

          # Build tools
          pkg-config
          cmake
          ninja
          mtools          # mformat/mcopy for FAT ESP images
          dosfstools      # mkfs.fat (alternative to mtools)
          cpio
          e2fsprogs       # mkfs.ext2 / tune2fs for initramfs images

          # QEMU system emulators
          qemu

          # Misc dev tools
          just
          git
          curl
          unzip
          bsdtar
        ];

      in {
        # ── Dev shell ─────────────────────────────────────────────────
        devShells.default = pkgs.mkShell {
          name = "rustos-dev";
          packages = [ rustToolchain ] ++ nativeDeps;

          shellHook = ''
            # Firmware paths used by `cargo xtask run` and QEMU scripts
            ovmfX64Path="${ovmfX64}/FV"
            ovmfAarch64Path="${ovmfAarch64}/FV"
            export OVMF_CODE="$ovmfX64Path/OVMF_CODE.fd"
            export OVMF_VARS="$ovmfX64Path/OVMF_VARS.fd"
            export QEMU_EFI="$ovmfAarch64Path/AAVMF_CODE.fd"

            # AArch64 bare-metal assembler / archiver
            export AARCH64_AS=$(which aarch64-none-elf-as 2>/dev/null || echo "")
            export AARCH64_AR=$(which aarch64-none-elf-ar 2>/dev/null || echo "")

            echo ""
            echo "╔══════════════════════════════════════════════════╗"
            echo "║           RustOS development shell               ║"
            echo "╠══════════════════════════════════════════════════╣"
            echo "║  Supported architectures: aarch64, x86_64        ║"
            echo "╠══════════════════════════════════════════════════╣"
            echo "║  Quick commands:                                 ║"
            echo "║    cargo xtask run --arch aarch64                ║"
            echo "║    cargo xtask run --arch x86_64                 ║"
            echo "║    cargo xtask image --arch x86_64               ║"
            echo "╚══════════════════════════════════════════════════╝"
            echo ""
          '';
        };

        # ── CI packages ───────────────────────────────────────────────
        packages.default = self.packages.${system}.rustos-x86-uefi;

        packages.rustos-x86-uefi = pkgs.stdenv.mkDerivation {
          pname = "rustos-x86-uefi";
          version = "0.1.0";
          src = ./.;
          nativeBuildInputs = [ rustToolchain ] ++ nativeDeps;
          buildPhase = ''
            cargo xtask image --arch x86_64
          '';
          installPhase = ''
            mkdir -p $out/boot
            cp boot-x86_64.img $out/boot/
          '';
        };

        packages.rustos-aarch64-uefi = pkgs.stdenv.mkDerivation {
          pname = "rustos-aarch64-uefi";
          version = "0.1.0";
          src = ./.;
          nativeBuildInputs = [ rustToolchain ] ++ nativeDeps;
          buildPhase = ''
            cargo xtask image --arch aarch64
          '';
          installPhase = ''
            mkdir -p $out/boot
            cp boot-aarch64.img $out/boot/
          '';
        };
      }
    );
}

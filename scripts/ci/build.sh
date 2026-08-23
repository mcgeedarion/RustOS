#!/usr/bin/env bash
# scripts/ci/build.sh — Arch-aware kernel build for CI.
#
# Required env:
#   ARCH    aarch64 | x86_64 | riscv64
#
# Optional env:
#   RELEASE    1 => --release  (default: 0)
#   BOOT       uefi | baremetal  (default: uefi)
#   FEATURES   extra --features value appended verbatim
#
# Sets/exports on success:
#   KERNEL_ELF   path to the built kernel artifact. UEFI targets may emit
#                rustos.efi instead of rustos.
#   CARGO_TARGET Rust target triple / JSON path used
#   PROFILE      debug | release
#
# Exit codes:
#   0  success
#   1  build failed or artifact not found
#   2  bad arguments

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

# ── Validate required inputs ────────────────────────────────────────────────────────────────────────

ARCH="${ARCH:-}"
[[ -z "$ARCH" ]] && { echo "[!] ARCH is required (aarch64|x86_64|riscv64)" >&2; exit 2; }

case "$ARCH" in
  aarch64|x86_64|riscv64) ;;
  *) echo "[!] Unsupported ARCH='${ARCH}'" >&2; exit 2 ;;
esac

RELEASE="${RELEASE:-0}"
BOOT="${BOOT:-uefi}"
FEATURES="${FEATURES:-}"

case "$ARCH:$BOOT" in
  aarch64:uefi|aarch64:baremetal|x86_64:uefi) ;;
  *) echo "[!] Unsupported build contract: ARCH=${ARCH} BOOT=${BOOT}" >&2; exit 2 ;;
esac

# ── Per-arch target + artifact path ──────────────────────────────────────────────────────────────────────

case "$ARCH:$BOOT" in
  aarch64:uefi)
    CARGO_TARGET="aarch64-unknown-uefi"
    TARGET_DIR="aarch64-unknown-uefi"
    ;;
  aarch64:baremetal)
    CARGO_TARGET="${ROOT_DIR}/targets/aarch64-kernel.json"
    TARGET_DIR="aarch64-kernel"
    ;;
  x86_64:uefi)
    CARGO_TARGET="x86_64-unknown-uefi"
    TARGET_DIR="x86_64-unknown-uefi"
    ;;
esac

PROFILE=$([ "$RELEASE" = 1 ] && echo release || echo debug)
EXTRA_FLAGS=()

pick_kernel_artifact() {
  local base="target/${TARGET_DIR}/${PROFILE}/rustos"

  if [[ "$BOOT" == "uefi" && -f "${base}.efi" ]]; then
    echo "${base}.efi"
    return 0
  fi

  if [[ -f "$base" ]]; then
    echo "$base"
    return 0
  fi

  return 1
}

# ── Append optional extra features ───────────────────────────────────────────────────────────────────────

if [[ -n "$FEATURES" ]]; then
  FEATURES_COMPACT="${FEATURES//[[:space:]]/}"
  case ",${FEATURES_COMPACT}," in
    *,boot_minimal,*)
      EXTRA_FLAGS+=(--no-default-features)
      ;;
  esac
  EXTRA_FLAGS+=(--features "$FEATURES")
fi

# ── Build ──────────────────────────────────────────────────────────────────────────────────────

echo "[build] ARCH=${ARCH}  TARGET=${CARGO_TARGET}  PROFILE=${PROFILE}  BOOT=${BOOT}"

cargo build \
  --target "$CARGO_TARGET" \
  "${EXTRA_FLAGS[@]}" \
  -Z build-std=core,alloc,compiler_builtins \
  -Z build-std-features=compiler-builtins-mem \
  -Z json-target-spec \
  $([ "$RELEASE" = 1 ] && echo --release)

# ── Verify output ────────────────────────────────────────────────────────────────────────────────────

KERNEL_ELF="$(pick_kernel_artifact || true)"
if [[ -z "$KERNEL_ELF" ]]; then
  echo "[!] Kernel artifact not found under target/${TARGET_DIR}/${PROFILE}/" >&2
  exit 1
fi

export KERNEL_ELF CARGO_TARGET PROFILE
echo "[build] OK  ${KERNEL_ELF}  ($(du -sh "$KERNEL_ELF" | cut -f1))"

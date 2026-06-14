#!/usr/bin/env bash
set -euo pipefail

ARCH=${ARCH:-x86_64}
BOOT=uefi
TIMEOUT=${TIMEOUT:-30}
SMOKE=0
SMOKE_MARKER_RE=${SMOKE_MARKER_RE:-'BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|rustos: kernel_main reached'}
SMOKE_MARKER_DESC=${SMOKE_MARKER_DESC:-'BOOT_MINIMAL_OK/FULL_OS_USERSPACE_OK/rustos: kernel_main reached'}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch)
      ARCH=${2:?missing --arch value}; shift 2 ;;
    --boot)
      BOOT=${2:?missing --boot value}; shift 2 ;;
    --timeout)
      TIMEOUT=${2:?missing --timeout value}; shift 2 ;;
    --smoke)
      SMOKE=1; shift ;;
    *)
      echo "run_qemu.sh: unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ "$BOOT" != "uefi" && !( "$ARCH" == "riscv64" && "$BOOT" == "sbi" ) ]]; then
  echo "run_qemu.sh: supported boot contracts are x86_64/aarch64 UEFI and riscv64 SBI" >&2
  exit 2
fi

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
IMG="$ROOT/boot-${ARCH}.img"
KERNEL="$ROOT/target/riscv64-kernel/debug/rustos"

if [[ "$BOOT" == "uefi" && ! -f "$IMG" ]]; then
  echo "run_qemu.sh: image not found: $IMG" >&2
  echo "run_qemu.sh: build it with: cargo xtask image --arch $ARCH --boot uefi --debug --features boot_minimal" >&2
  exit 1
fi
if [[ "$ARCH:$BOOT" == "riscv64:sbi" && ! -f "$KERNEL" ]]; then
  echo "run_qemu.sh: kernel not found: $KERNEL" >&2
  echo "run_qemu.sh: build it with: cargo xtask build --arch riscv64 --boot sbi --debug --features boot_minimal" >&2
  exit 1
fi

case "$ARCH" in
  x86_64)
    QEMU=${QEMU:-qemu-system-x86_64}
    if ! command -v "$QEMU" >/dev/null 2>&1; then
      echo "run_qemu.sh: $QEMU not found on PATH" >&2
      exit 1
    fi
    OVMF_CODE=${OVMF_CODE:-}
    for candidate in \
      /usr/share/OVMF/OVMF_CODE.fd \
      /usr/share/ovmf/OVMF.fd \
      /usr/share/qemu/OVMF.fd; do
      [[ -n "$OVMF_CODE" ]] && break
      [[ -f "$candidate" ]] && OVMF_CODE=$candidate
    done
    if [[ -z "$OVMF_CODE" ]]; then
      echo "run_qemu.sh: OVMF firmware not found; set OVMF_CODE=/path/to/OVMF_CODE.fd" >&2
      exit 1
    fi
    args=(
      -machine q35
      -m 256M
      -cpu qemu64
      -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE"
      -drive format=raw,file="$IMG",if=virtio
      -serial stdio
      -display none
      -no-reboot
      -no-shutdown
    )
    ;;
  aarch64)
    QEMU=${QEMU:-qemu-system-aarch64}
    if ! command -v "$QEMU" >/dev/null 2>&1; then
      echo "run_qemu.sh: $QEMU not found on PATH" >&2
      exit 1
    fi
    QEMU_EFI=${QEMU_EFI:-}
    for candidate in \
      /usr/share/AAVMF/AAVMF_CODE.fd \
      /usr/share/AAVMF/AAVMF32_CODE.fd \
      /usr/share/qemu-efi-aarch64/QEMU_EFI.fd; do
      [[ -n "$QEMU_EFI" ]] && break
      [[ -f "$candidate" ]] && QEMU_EFI=$candidate
    done
    if [[ -z "$QEMU_EFI" ]]; then
      echo "run_qemu.sh: AArch64 UEFI firmware not found; set QEMU_EFI=/path/to/QEMU_EFI.fd" >&2
      exit 1
    fi
    args=(
      -machine virt
      -cpu cortex-a57
      -m 512M
      -bios "$QEMU_EFI"
      -drive format=raw,file="$IMG",if=virtio
      -serial stdio
      -display none
      -no-reboot
      -no-shutdown
    )
    ;;
  riscv64)
    if [[ "$BOOT" != "sbi" ]]; then
      echo "run_qemu.sh: riscv64 UEFI is not runnable here; use --boot sbi" >&2
      exit 2
    fi
    QEMU=${QEMU:-qemu-system-riscv64}
    if ! command -v "$QEMU" >/dev/null 2>&1; then
      echo "run_qemu.sh: $QEMU not found on PATH" >&2
      exit 1
    fi
    args=(
      -machine virt
      -m 512M
      -kernel "$KERNEL"
      -serial stdio
      -display none
      -no-reboot
      -no-shutdown
    )
    ;;
  *)
    echo "run_qemu.sh: unsupported ARCH=$ARCH" >&2
    exit 2
    ;;
esac

log=$(mktemp)
fifo=
trap 'rm -f "$log" ${fifo:+"$fifo"}' EXIT

if [[ $SMOKE -eq 1 ]]; then
  fifo=$(mktemp -u)
  mkfifo "$fifo"

  "$QEMU" "${args[@]}" >"$fifo" 2>&1 &
  qemu_pid=$!

  marker=0
  deadline=$((SECONDS + TIMEOUT))
  exec 3<"$fifo"
  while (( SECONDS < deadline )); do
    if IFS= read -r -t 1 line <&3; then
      printf '%s\n' "$line" | tee -a "$log"
      if [[ "$line" =~ $SMOKE_MARKER_RE ]]; then
        marker=1
        break
      fi
      continue
    fi

    if ! kill -0 "$qemu_pid" 2>/dev/null; then
      break
    fi
  done
  exec 3<&-

  if [[ $marker -eq 1 ]]; then
    kill "$qemu_pid" 2>/dev/null || true
    wait "$qemu_pid" 2>/dev/null || true
    exit 0
  fi

  if kill -0 "$qemu_pid" 2>/dev/null; then
    kill "$qemu_pid" 2>/dev/null || true
  fi
  wait "$qemu_pid" 2>/dev/null
  status=$?
  if ! grep -Eq "$SMOKE_MARKER_RE" "$log"; then
    echo "run_qemu.sh: smoke marker $SMOKE_MARKER_DESC not observed" >&2
    exit 1
  fi
  exit "$status"
fi
trap 'rm -f "$log"' EXIT

set +e
timeout "$TIMEOUT" "$QEMU" "${args[@]}" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

if [[ $SMOKE -eq 1 ]] && ! grep -Eq "$SMOKE_MARKER_RE" "$log"; then
  echo "run_qemu.sh: smoke marker $SMOKE_MARKER_DESC not observed" >&2
  exit 1
fi

# QEMU often times out in no-shutdown smoke mode after the kernel parks in idle.
if [[ $status -eq 124 && $SMOKE -eq 1 ]]; then
  exit 0
fi
exit "$status"

#!/usr/bin/env bash
set -euo pipefail

ARCH=${ARCH:-x86_64}
BOOT=uefi
TIMEOUT=${TIMEOUT:-30}
SMOKE=0
TEMP_FW_VARS=""
SMOKE_MARKER_RE=${SMOKE_MARKER_RE:-'BOOT_MINIMAL_OK|FULL_OS_USERSPACE_OK|entering cpu_idle'}

usage() {
    echo "Usage: ARCH=<arch> [TIMEOUT=<s>] [SMOKE=1] $0 [--arch <arch>] [--boot uefi] [--timeout <s>] [--smoke|--test]"
    echo "  ARCH    : x86_64 | aarch64 | riscv64"
    echo "  TIMEOUT : seconds before QEMU is killed (default: 30)"
    echo "  SMOKE   : set to 1 to grep serial output for boot sentinel"
    echo "  --test  : CI alias for --smoke"
    exit 1
}

positional=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            ;;
        --arch)
            [[ $# -ge 2 ]] || { echo "ERROR: --arch requires a value" >&2; usage; }
            ARCH=$2
            shift 2
            ;;
        --boot)
            [[ $# -ge 2 ]] || { echo "ERROR: --boot requires a value" >&2; usage; }
            BOOT=$2
            shift 2
            ;;
        --timeout)
            [[ $# -ge 2 ]] || { echo "ERROR: --timeout requires a value" >&2; usage; }
            TIMEOUT=$2
            shift 2
            ;;
        --smoke|--test)
            SMOKE=1
            shift
            ;;
        --)
            shift
            break
            ;;
        -*)
            echo "ERROR: unsupported option '$1'" >&2
            usage
            ;;
        *)
            # Preserve the original positional interface for local callers.
            case "$positional" in
                0) ARCH=$1 ;;
                1) TIMEOUT=$1 ;;
                2) SMOKE=$1 ;;
                *) echo "ERROR: too many positional arguments" >&2; usage ;;
            esac
            positional=$((positional + 1))
            shift
            ;;
    esac
done

if [[ "$BOOT" != "uefi" ]]; then
    echo "ERROR: unsupported boot mode '$BOOT'" >&2
    usage
fi

case "$ARCH" in
    x86_64|aarch64|riscv64) ;;
    *) echo "ERROR: unsupported arch '$ARCH'" >&2; usage ;;
esac

IMAGE="boot-${ARCH}.img"
if [[ ! -f "$IMAGE" ]]; then
    echo "ERROR: disk image '$IMAGE' not found. Run 'cargo xtask image --arch $ARCH' first." >&2
    exit 1
fi

SERIAL_LOG=$(mktemp /tmp/rustos-serial-XXXXXX.log)
trap 'rm -f "$SERIAL_LOG" "$TEMP_FW_VARS"' EXIT

case "$ARCH" in
    x86_64)
        FW_KIND="ovmf"
        FW_CODE="${OVMF_CODE:-}"
        FW_VARS="${OVMF_VARS:-}"

        # Locate firmware if not set via environment
        if [[ -z "$FW_CODE" ]]; then
            for candidate in \
                /usr/share/OVMF/OVMF_CODE.fd \
                /usr/share/OVMF/OVMF_CODE_4M.fd \
                /usr/share/ovmf/OVMF.fd \
                /usr/share/qemu/OVMF.fd \
                /usr/share/edk2/x64/OVMF_CODE.fd \
                /opt/homebrew/share/qemu/edk2-x86_64-code.fd; do
                if [[ -f "$candidate" ]]; then
                    FW_CODE="$candidate"
                    break
                fi
            done
        fi

        if [[ -z "$FW_CODE" ]]; then
            echo "ERROR: OVMF firmware not found. Set OVMF_CODE=/path/to/OVMF_CODE.fd" >&2
            exit 1
        fi

        if [[ -z "$FW_VARS" ]]; then
            FW_VARS=$(mktemp /tmp/OVMF_VARS.XXXXXX.fd)
            TEMP_FW_VARS="$FW_VARS"
            # Use a blank vars file (64K) so UEFI can write runtime variables
            dd if=/dev/zero of="$FW_VARS" bs=1k count=64 2>/dev/null
        fi

        QEMU_CMD=(
            qemu-system-x86_64
            -machine q35
            -accel tcg
            -cpu qemu64,+xsave,+avx
            -m 256M
            -drive "if=pflash,format=raw,readonly=on,file=${FW_CODE}"
            -drive "if=pflash,format=raw,file=${FW_VARS}"
            -drive "if=virtio,format=raw,file=${IMAGE}"
            -serial "file:${SERIAL_LOG}"
            -display none
            -no-reboot
            -no-shutdown
        )
        ;;

    aarch64)
        FW_CODE="${QEMU_EFI:-}"

        if [[ -z "$FW_CODE" ]]; then
            for candidate in \
                /usr/share/AAVMF/AAVMF_CODE.fd \
                /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
                /usr/share/qemu/edk2-aarch64-code.fd; do
                if [[ -f "$candidate" ]]; then
                    FW_CODE="$candidate"
                    break
                fi
            done
        fi

        if [[ -z "$FW_CODE" ]]; then
            echo "ERROR: AArch64 UEFI firmware not found. Set QEMU_EFI=/path/to/QEMU_EFI.fd" >&2
            exit 1
        fi

        QEMU_CMD=(
            qemu-system-aarch64
            -machine virt
            -cpu cortex-a57
            -m 512M
            -bios "${FW_CODE}"
            -drive "if=none,id=esp,format=raw,file=${IMAGE}"
            -device virtio-blk-device,drive=esp
            -serial "file:${SERIAL_LOG}"
            -display none
            -no-reboot
            -no-shutdown
        )
        ;;
esac

echo "[run_qemu] launching QEMU for ${ARCH} (timeout ${TIMEOUT}s)..."
timeout "$TIMEOUT" "${QEMU_CMD[@]}" || true

if [[ "$SMOKE" == "1" ]]; then
    echo "[run_qemu] checking serial output for boot sentinel..."
    if grep -qE "$SMOKE_MARKER_RE" "$SERIAL_LOG"; then
        echo "[run_qemu] SMOKE PASS: sentinel found."
        exit 0
    else
        echo "[run_qemu] SMOKE FAIL: sentinel not found in serial output."
        echo "--- serial output ---"
        cat "$SERIAL_LOG"
        exit 1
    fi
fi

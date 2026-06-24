#!/usr/bin/env bash
# parse-boot-marks.sh — extract BOOT_MARK lines from a QEMU serial log and
# print per-milestone tick counts plus inter-milestone deltas.
#
# Usage:
#   ./scripts/ci/parse-boot-marks.sh <qemu-log-file>
#
# Expected input lines (anywhere in the file):
#   BOOT_MARK label=BOOT_ENTRY           ticks=12345678
#   BOOT_MARK label=BOOT_MMU_ON          ticks=12350000
#   BOOT_MARK label=BOOT_INITRAMFS_LOADED ticks=12360000
#   BOOT_MARK label=BOOT_INIT_EXEC       ticks=12380000
#
# Exit codes:
#   0  — all four required milestones found, deltas printed
#   1  — one or more milestones missing (logged to stderr)

set -euo pipefail

LOG="${1:-}"
if [[ -z "$LOG" ]]; then
    echo "Usage: $0 <qemu-log-file>" >&2
    exit 1
fi
if [[ ! -f "$LOG" ]]; then
    echo "ERROR: log file not found: $LOG" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Parse all BOOT_MARK lines into an associative array  label → ticks
# ---------------------------------------------------------------------------
declare -A MARKS

while IFS= read -r line; do
    # Match:  BOOT_MARK label=<LABEL> ticks=<N>
    if [[ "$line" =~ BOOT_MARK[[:space:]]+label=([A-Z0-9_]+)[[:space:]]+ticks=([0-9]+) ]]; then
        label="${BASH_REMATCH[1]}"
        ticks="${BASH_REMATCH[2]}"
        MARKS["$label"]="$ticks"
    fi
done < "$LOG"

# ---------------------------------------------------------------------------
# Required milestones in chronological order
# ---------------------------------------------------------------------------
ORDERED=(
    BOOT_ENTRY
    BOOT_MMU_ON
    BOOT_INITRAMFS_LOADED
    BOOT_INIT_EXEC
)

# ---------------------------------------------------------------------------
# Validate that all four milestones are present
# ---------------------------------------------------------------------------
MISSING=()
for m in "${ORDERED[@]}"; do
    if [[ -z "${MARKS[$m]+x}" ]]; then
        MISSING+=("$m")
    fi
done

if [[ ${#MISSING[@]} -gt 0 ]]; then
    echo "BOOT_PERF ERROR: missing milestones: ${MISSING[*]}" >&2
    echo "" >&2
    echo "Milestones found:" >&2
    for m in "${ORDERED[@]}"; do
        if [[ -n "${MARKS[$m]+x}" ]]; then
            echo "  $m = ${MARKS[$m]} ticks" >&2
        fi
    done
    exit 1
fi

# ---------------------------------------------------------------------------
# Print header and per-milestone absolute tick values
# ---------------------------------------------------------------------------
echo "======================================="
echo "  RustOS boot performance markers"
echo "======================================="
printf "%-30s  %20s  %20s\n" "MILESTONE" "TICKS (abs)" "DELTA (ticks)"
echo "-----------------------------------------------------------------------"

PREV_TICKS=""
for m in "${ORDERED[@]}"; do
    cur="${MARKS[$m]}"
    if [[ -z "$PREV_TICKS" ]]; then
        delta="(baseline)"
    else
        delta=$(( cur - PREV_TICKS ))
    fi
    printf "%-30s  %20s  %20s\n" "$m" "$cur" "$delta"
    PREV_TICKS="$cur"
done

echo "-----------------------------------------------------------------------"
# Total span: BOOT_INIT_EXEC - BOOT_ENTRY
TOTAL=$(( MARKS[BOOT_INIT_EXEC] - MARKS[BOOT_ENTRY] ))
printf "%-30s  %20s  %20s\n" "TOTAL (ENTRY → INIT_EXEC)" "" "$TOTAL"
echo "======================================="

echo ""
echo "BOOT_PERF OK — all 4 milestones present."

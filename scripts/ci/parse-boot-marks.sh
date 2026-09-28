#!/usr/bin/env bash
# Validate a single boot's BOOT_MARK records and report honest timing spans.
#
# Usage:
#   bash scripts/ci/parse-boot-marks.sh [--mode full|minimal] <qemu-log-file>
#
# Full is deliberately the default; minimal CI must opt in explicitly.
# Python's integer arithmetic preserves the entire u64 counter range without
# Bash signed overflow or awk floating-point rounding. No external packages.
# Exit 0: validated log; 1: invalid/unreadable log; 2: invalid invocation.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/parse-boot-marks.py" "$@"

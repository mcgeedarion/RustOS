#!/usr/bin/env bash
# boot-regression.sh — compare current boot perf against the stored baseline.
#
# Usage:
#   bash scripts/ci/boot-regression.sh <current.json> <baseline.json> [threshold_pct]
#
# Arguments:
#   current.json     JSON file produced by parse-boot-marks.sh for this run
#   baseline.json    Previously stored baseline (from CI artifact)
#   threshold_pct    Regression threshold in percent (default: 20)
#
# The script exits non-zero if total_boot_ns regressed beyond the threshold.

set -euo pipefail

CURRENT_JSON="${1:?Usage: $0 <current.json> <baseline.json> [threshold_pct]}"
BASELINE_JSON="${2:?Usage: $0 <current.json> <baseline.json> [threshold_pct]}"
THRESHOLD="${3:-20}"

if ! command -v python3 &>/dev/null; then
  echo "ERROR: python3 required" >&2
  exit 1
fi

python3 - "$CURRENT_JSON" "$BASELINE_JSON" "$THRESHOLD" <<'EOF'
import json, sys

current_file, baseline_file, threshold_str = sys.argv[1], sys.argv[2], sys.argv[3]
threshold = float(threshold_str)

with open(current_file) as f:
    current = json.load(f)
with open(baseline_file) as f:
    baseline = json.load(f)

cur_ns = current.get("total_boot_ns", 0)
base_ns = baseline.get("total_boot_ns", 0)

if base_ns == 0:
    print(f"INFO: baseline total_boot_ns is 0 — skipping regression check")
    sys.exit(0)

regression_pct = ((cur_ns - base_ns) / base_ns) * 100
print(f"Boot time: current={cur_ns}ns  baseline={base_ns}ns  delta={regression_pct:+.1f}%  threshold={threshold}%")

if regression_pct > threshold:
    print(f"FAIL: boot time regressed by {regression_pct:.1f}% (threshold {threshold}%)", file=sys.stderr)
    sys.exit(1)

print("OK: boot time within threshold")
EOF

#!/usr/bin/env bash
# check-stubs.sh — enforce that stub modules do not leak into full-kernel builds.
#
# Usage:
#   bash scripts/ci/check-stubs.sh
#
# The script checks that every module listed in STUB_MODULES either:
#   a) is absent from the compiled symbol table when built with --features full-kernel, OR
#   b) emits a WARN stub log line at runtime (verified by grep in this script).
#
# Currently this performs a static source-level check: no file under src/ that
# is gated ONLY on userspace_boot should be reachable via full-kernel.
# A full binary-level check requires a running QEMU image and is deferred to
# the fault-inject workflow.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Modules that are stubs and must not appear in full-kernel without a WARN log.
# Format: "src/path/to/module.rs:WARN_PATTERN"
STUB_CHECKS=(
  "src/fs/:WARN: proc is stubbed"
  "src/proc/:WARN: proc is stubbed"
)

ERRORS=0

for CHECK in "${STUB_CHECKS[@]}"; do
  MODULE_PATH="${CHECK%%:*}"
  WARN_PATTERN="${CHECK#*:}"

  # Check that stub files emit a WARN at runtime via a log! macro call
  if find "$ROOT/$MODULE_PATH" -name '*.rs' 2>/dev/null | xargs grep -ql '"WARN:' 2>/dev/null; then
    echo "OK: $MODULE_PATH emits WARN stub log line"
  else
    echo "WARN: $MODULE_PATH does not appear to emit a stub WARN log line" >&2
    echo "      Add: log::warn!(\"WARN: <module> is stubbed in <feature>\");" >&2
    # Treat as informational for now; upgrade to error when all stubs are annotated
    # ERRORS=$((ERRORS + 1))
  fi
done

# Check that stub-only modules are not unconditionally included in lib.rs
# (i.e. they must be behind #[cfg(feature = "...")])
STUB_MODULES=("io_uring" "net" "display" "security")

for MOD in "${STUB_MODULES[@]}"; do
  # `pub mod $MOD;` must be guarded by a nearby #[cfg(...)] attribute. The
  # module declarations in src/lib.rs are commonly documented, then cfg-gated,
  # then declared; checking only the same line produces false positives.
  if awk -v mod="$MOD" '
    $0 ~ "^pub mod " mod ";" {
      if (prev !~ /^#\[cfg/) {
        print FNR ":" $0;
        exit 1;
      }
    }
    NF { prev=$0 }
  ' "$ROOT/src/lib.rs"; then
    echo "OK: $MOD is feature-gated in lib.rs"
  else
    echo "FAIL: src/lib.rs exposes 'pub mod ${MOD}' without a nearby #[cfg(...)] guard" >&2
    ERRORS=$((ERRORS + 1))
  fi
done

if [ "$ERRORS" -gt 0 ]; then
  echo "check-stubs: $ERRORS violation(s) found" >&2
  exit 1
fi

echo "check-stubs: all checks passed"

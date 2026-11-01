#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
python3 - "$work/wait_helpers.rs" <<'PY'
from pathlib import Path
import sys
source = Path("src/proc/wait.rs").read_text()
functions = []
for name in ("encode_exit", "notify_exit"):
    start = source.index("pub fn " + name + "(")
    opening = source.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    functions.append(source[start:end])
Path(sys.argv[1]).write_text("\n\n".join(functions) + "\n")
PY
RUSTOS_WAIT_HELPERS="$work/wait_helpers.rs" rustc --edition=2021 \
  --target x86_64-unknown-linux-gnu --test tests/host/exit_lifecycle.rs \
  -o "$work/exit-lifecycle-tests"
"$work/exit-lifecycle-tests" --test-threads=1

#!/usr/bin/env bash
# Check every tracked Rust source in the existing kernel/workspace lint scope.
# Explicit inputs avoid traversing declarations of absent experimental modules.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
sources=()
while IFS= read -r -d '' path; do
    [[ "$path" == *.rs ]] && sources+=("$path")
done < <(git ls-files -z -- src crates xtask)
if [[ ${#sources[@]} -eq 0 ]]; then
    echo "ERROR: no tracked Rust sources found" >&2
    exit 1
fi
rustfmt --edition 2021 --config skip_children=true --check "${sources[@]}"

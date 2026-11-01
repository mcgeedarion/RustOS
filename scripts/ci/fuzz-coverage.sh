#!/usr/bin/env bash
# Advisory corpus source coverage, not kernel execution coverage.
set -uo pipefail
root="$(git rev-parse --show-toplevel)"
cd "$root" || exit 1
report="$root/fuzz-coverage-report"
mkdir -p "$report"
host="$(rustc -vV | awk '/^host:/ {print $2}')"
llvm_bin="$(rustc --print sysroot)/lib/rustlib/$host/bin"
profdata="$llvm_bin/llvm-profdata"
cov="$llvm_bin/llvm-cov"
[[ -x "$profdata" && -x "$cov" ]] || { echo 'llvm-tools-preview is required' >&2; exit 1; }
export RUSTFLAGS="-C instrument-coverage"
status=0
count=0
printf 'target\tstatus\n' > "$report/status.tsv"
shopt -s nullglob
for corpus in fuzz/corpus/*; do
    [[ -d "$corpus" ]] || continue
    target="$(basename "$corpus")"
    count=$((count + 1))
    out="$report/$target"
    mkdir -p "$out"
    if ! timeout 240s cargo fuzz coverage "$target" "$corpus" --sanitizer none 2>&1 | tee "$out/coverage.log"; then
        printf '%s\tcoverage-command-failed\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    mapfile -d '' profiles < <(find "fuzz/coverage/$target" -type f -name '*.profraw' -print0)
    mapfile -d '' binaries < <(find fuzz/target -type f -name "$target" -executable -print0)
    if (( ${#profiles[@]} == 0 || ${#binaries[@]} != 1 )); then
        echo "Cannot resolve raw profiles or unique instrumented binary for $target" >&2
        printf '%s\tmissing-or-ambiguous-coverage-inputs\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    binary="${binaries[0]}"
    if ! "$profdata" merge -sparse "${profiles[@]}" -o "$out/coverage.profdata" \
        || ! "$cov" report "$binary" -instr-profile="$out/coverage.profdata" -show-region-summary > "$out/report.txt" \
        || ! "$cov" export "$binary" -instr-profile="$out/coverage.profdata" > "$out/coverage.json" \
        || ! "$cov" show "$binary" -instr-profile="$out/coverage.profdata" -format=html -output-dir="$out/html"; then
        printf '%s\treport-generation-failed\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    cat "$out/report.txt"
    if ! python3 - "$target" "$out/coverage.json" <<'PY'
import json
import sys
from pathlib import Path
value = json.loads(Path(sys.argv[2]).read_text())
for entry in value["data"]:
    for kind in ("regions", "lines"):
        item = entry["totals"][kind]
        print(f"{sys.argv[1]} {kind}: {item['covered']}/{item['count']} ({item['percent']:.2f}%)")
PY
    then
        printf '%s\tsummary-generation-failed\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    printf '%s\treported\n' "$target" >> "$report/status.tsv"
done
if (( count == 0 )); then
    echo 'No fuzz/corpus target directories found' >&2
    status=1
fi
cat "$report/status.tsv"
exit "$status"

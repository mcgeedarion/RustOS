#!/usr/bin/env bash
# Advisory corpus source coverage, not kernel execution coverage.
set -uo pipefail
root="$(git rev-parse --show-toplevel)" || exit 1
cd "$root" || exit 1
report="$root/fuzz-coverage-report"
mkdir -p "$report"
printf 'target\tstatus\n' > "$report/status.tsv"
setup_failed() {
    printf '__setup__\t%s\n' "$1" >> "$report/status.tsv"
    echo "$1" >&2
    exit 1
}
host="$(rustc -vV | awk '/^host:/ {print $2}')" || setup_failed rustc-version-failed
[[ -n "$host" ]] || setup_failed missing-host-triple
sysroot="$(rustc --print sysroot)" || setup_failed rustc-sysroot-failed
llvm_bin="$sysroot/lib/rustlib/$host/bin"
profdata="$llvm_bin/llvm-profdata"
cov="$llvm_bin/llvm-cov"
[[ -x "$profdata" && -x "$cov" ]] || setup_failed missing-llvm-tools-preview
{
    rustc -vV
    cargo --version
    cargo fuzz --version
    "$profdata" --version
    "$cov" --version
} > "$report/toolchain.txt" 2>&1
export RUSTFLAGS="-C instrument-coverage"
status=0
count=0
shopt -s nullglob
for corpus in fuzz/corpus/*; do
    [[ -d "$corpus" ]] || continue
    target="$(basename "$corpus")"
    count=$((count + 1))
    out="$report/$target"
    build_dir="$root/target/fuzz-coverage/$target"
    raw_dir="$root/fuzz/coverage/$target/raw"
    mkdir -p "$out"
    if ! timeout 240s cargo fuzz coverage "$target" "$corpus" --sanitizer none \
        --target "$host" --target-dir "$build_dir" --llvm-path "$llvm_bin" \
        2>&1 | tee "$out/coverage.log"; then
        printf '%s\tcoverage-command-failed\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    # cargo-fuzz receives an explicit target-dir and host triple above. Do not
    # guess its default coverage layout or pick stale fuzzing binaries.
    binary="$build_dir/$host/release/$target"
    if [[ ! -x "$binary" || ! -d "$raw_dir" ]]; then
        printf '%s\tmissing-coverage-inputs\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
    mapfile -d '' profiles < <(find "$raw_dir" -type f -name '*.profraw' -print0)
    if (( ${#profiles[@]} == 0 )); then
        printf '%s\tmissing-raw-profiles\n' "$target" >> "$report/status.tsv"
        status=1
        continue
    fi
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

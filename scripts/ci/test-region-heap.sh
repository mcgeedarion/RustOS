#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
rustc --edition=2021 --target x86_64-unknown-linux-gnu --test \
  src/allocator/region_heap.rs -o "$work/region-heap-tests"
"$work/region-heap-tests"

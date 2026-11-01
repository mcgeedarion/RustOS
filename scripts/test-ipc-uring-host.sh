#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bin="$(mktemp /tmp/rustos-ipc-uring.XXXXXX)"
trap 'rm -f "$bin"' EXIT
rustc --edition=2021 --test tests/host/ipc_uring.rs -o "$bin"
"$bin" --test-threads=1

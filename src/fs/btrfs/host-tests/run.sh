#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
command -v mkfs.btrfs >/dev/null
command -v btrfs >/dev/null
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/rustos-btrfs-host-target}"
cargo test --manifest-path "$here/Cargo.toml" --target x86_64-unknown-linux-gnu \
  -Z build-std=std,panic_abort -- --nocapture

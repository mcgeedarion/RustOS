#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export PATH="$HOME/.cargo/bin:$PATH"
rustc --edition=2021 --test "$root/tests/host/ext4-write.rs" -o "$tmp/ext4-write-test"
for block_size in 1024 4096; do
    image="$tmp/ext4-$block_size.img"
    truncate -s 32M "$image"
    mke2fs -q -F -t ext4 -b "$block_size" -I 256 \
        -O '^has_journal,^metadata_csum,^64bit,^orphan_file,^uninit_bg' \
        -E lazy_itable_init=0 "$image"
    EXT4_WRITE_IMAGE="$image" "$tmp/ext4-write-test" --test-threads=1
    e2fsck -fn "$image"
    debugfs -R 'cat /final' "$image" 2>/dev/null | cmp - <(printf 'ext4 write regression\n')
done

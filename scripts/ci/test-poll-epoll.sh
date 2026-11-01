#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
rustc --edition=2021 --target x86_64-unknown-linux-gnu --test \
  tests/host/poll_epoll.rs -o "$work/poll-epoll-tests"
"$work/poll-epoll-tests" --test-threads=1

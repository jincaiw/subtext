#!/usr/bin/env bash
# The gates CI runs, in CI's order. One command instead of five.
#
#   tools/check.sh          fmt + test + UI test + clippy
#   tools/check.sh --full   ... plus the Linux/Windows cross checks
#
# The cross checks need `rustup target add x86_64-unknown-linux-gnu
# x86_64-pc-windows-msvc` first, so they are opt-in rather than the default.
set -euo pipefail

cd "$(dirname "$0")/.."

full=0
case "${1:-}" in
    "") ;;
    --full) full=1 ;;
    *) echo "usage: $0 [--full]" >&2; exit 2 ;;
esac

step() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
}

step "fmt"
cargo fmt --all --check

# `--locked` everywhere below, for the same reason CI uses it: the versions the
# lockfile pins are the versions that get tested.
step "test"
cargo test --workspace --locked

# Off by default because it pulls `gpui-fast-*` into the graph. It is the only
# check that catches "the button is there but nothing happens", so it stays.
step "test (ui)"
cargo test -p subtext-app --features ui-tests --test ui --locked

step "clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

# The notices file is generated from the lockfile. If a dependency moved, the
# file a release ships is stale, and stale licence information is worse than
# none — it looks checked.
step "third-party notices"
python3 tools/generate-notices.py --check

if [ "$full" = 1 ]; then
    step "check for linux"
    cargo check --locked --target x86_64-unknown-linux-gnu -p subtext-core

    step "check for windows"
    cargo check --locked --target x86_64-pc-windows-msvc -p subtext-core
fi

printf '\n\033[1;32mall checks passed\033[0m\n'

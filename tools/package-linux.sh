#!/usr/bin/env bash
#
# Package a Linux build of SubText.
#
#     tools/package-linux.sh [binary]     # default: target/release/subtext
#
# The result is dist/SubText-<version>-linux-<arch>/ plus a .tar.gz of it. The
# archive is laid out the way `Resources::discover()` expects: the executable
# at the top and `resources/` beside it, so the syntaxes and themes are found
# without a source tree.
#
# This script does not run `cargo build` itself. Linux binaries are built on a
# Linux host (or in a Linux container), and packaging is the same on either
# side of that build.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BINARY="${1:-$ROOT/target/release/subtext}"
if [ ! -f "$BINARY" ]; then
    echo "error: $BINARY does not exist — build it first, or pass its path" >&2
    exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
# The archive is named for the binary, not for the host doing the packing: a
# Linux build made in a container is usually packed from macOS, where
# `uname -m` would say `arm64` for an `aarch64` ELF and `arm64` at all only by
# coincidence.
ARCH="$(uname -m)"
if command -v file >/dev/null 2>&1; then
    case "$(file -b "$BINARY")" in
        *"x86-64"*) ARCH="x86_64" ;;
        *aarch64*) ARCH="aarch64" ;;
    esac
fi
NAME="SubText-$VERSION-linux-$ARCH"
STAGE="$ROOT/dist/$NAME"

echo "==> assembling $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE/resources"
cp "$BINARY" "$STAGE/subtext"
chmod +x "$STAGE/subtext"

for resource in syntaxes themes queries; do
    if [ -d "$ROOT/resources/$resource" ]; then
        cp -R "$ROOT/resources/$resource" "$STAGE/resources/$resource"
    fi
done
cp "$ROOT/resources/menu.json" "$STAGE/resources/menu.json"
cp "$ROOT/LICENSE" "$STAGE/LICENSE"
cp "$ROOT/CHANGELOG.md" "$STAGE/CHANGELOG.md"
cp "$ROOT/README.md" "$STAGE/README.md"

ARCHIVE="$ROOT/dist/$NAME.tar.gz"
echo "==> archiving $ARCHIVE"
rm -f "$ARCHIVE"
tar -czf "$ARCHIVE" -C "$ROOT/dist" "$NAME"

echo
echo "done."
echo "  directory: $STAGE"
echo "  archive:   $ARCHIVE"

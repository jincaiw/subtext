#!/usr/bin/env bash
#
# Set the workspace version, and keep Cargo.lock in step with it.
#
#     tools/bump-version.sh patch        # 0.5.0 -> 0.5.1
#     tools/bump-version.sh minor        # 0.5.0 -> 0.6.0
#     tools/bump-version.sh major        # 0.5.0 -> 1.0.0
#     tools/bump-version.sh 1.2.3        # exactly this version
#
# The version lives in exactly one place — `[workspace.package] version` in
# Cargo.toml; every crate inherits it with `version.workspace = true`. The
# release workflow runs this before tagging, which is why the auto-release rule
# ("increment the patch component unless a tag is given") is implemented here
# and not in sixteen lines of YAML.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
    sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

if [ $# -ne 1 ]; then
    usage >&2
    exit 2
fi

CURRENT="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [ -z "$CURRENT" ]; then
    echo "error: could not read [workspace.package] version from Cargo.toml" >&2
    exit 1
fi

case "$1" in
    -h|--help) usage; exit 0 ;;
    patch|minor|major)
        # An explicit x.y.z is the only other accepted form, so a typo cannot
        # silently become a version string.
        NEXT="$(python3 - "$CURRENT" "$1" <<'PY'
import sys
major, minor, patch = (int(part) for part in sys.argv[1].split("."))
kind = sys.argv[2]
if kind == "major":
    major, minor, patch = major + 1, 0, 0
elif kind == "minor":
    minor, patch = minor + 1, 0
else:
    patch += 1
print(f"{major}.{minor}.{patch}")
PY
)"
        ;;
    *)
        if ! printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
            echo "error: '$1' is neither patch/minor/major nor a version like 1.2.3" >&2
            exit 2
        fi
        NEXT="$1"
        ;;
esac

if [ "$NEXT" = "$CURRENT" ]; then
    echo "already at $CURRENT"
    exit 0
fi

echo "==> $CURRENT -> $NEXT"
python3 - "$NEXT" <<'PY'
import re
import sys

next_version = sys.argv[1]
path = "Cargo.toml"
text = open(path).read()
replaced, count = re.subn(
    r'(?m)^(version = ")[^"]*(")$',
    lambda match: match.group(1) + next_version + match.group(2),
    text,
    count=1,
)
if count != 1:
    raise SystemExit("error: [workspace.package] version line not found")
open(path, "w").write(replaced)
PY

# Cargo.lock records workspace member versions. `cargo metadata` rewrites it
# with the new number without touching any dependency resolution.
cargo metadata --format-version 1 >/dev/null

echo "==> Cargo.toml and Cargo.lock now say $NEXT"

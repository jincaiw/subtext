#!/bin/sh
# Photograph a panel that lives in a dialog (a sheet over the window).
#
#   tools/peek-overlay.sh <step> [output.png]
#
# The dialog is drawn inside the document window, and `screencapture -l` does
# capture it — verified on Custom Surround and Edit Color Code. (An earlier
# version of this script cropped a full-screen grab instead, because `-l` had
# appeared to miss the dialog; it does not, and a full-screen grab breaks down
# as soon as anything floats over the window.)
#
# For anything that is a real window — the document, Settings, Console, About —
# use `tools/peek-panel.sh` instead.
#
# Debug builds only: `SUBTEXT_PANEL_PEEK` is compiled out of release.
set -eu

if [ $# -lt 1 ]; then
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi

step=$1
out=${2:-/tmp/subtext-overlay-$step.png}
binary=${SUBTEXT_BINARY:-target/debug/subtext}
home=$(mktemp -d)
doc="$home/a.md"

if [ ! -x "$binary" ]; then
    echo "no binary at $binary — run \`cargo build\` first" >&2
    exit 1
fi

printf '# Title\n\nalpha beta gamma\n\ndelta epsilon\n\n## Second\n\nzeta\n' > "$doc"

pkill -f "$(basename "$binary")" 2>/dev/null || true
i=0
while pgrep -f "$(basename "$binary")" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -gt 40 ]; then
        echo "previous instance will not exit" >&2
        exit 1
    fi
    sleep 0.25
done

HOME="$home" SUBTEXT_PANEL_PEEK="$step" "$binary" "$doc" > "$home/log" 2>&1 &
sleep 7

line=$(/tmp/winlist 2>/dev/null | grep -i "\.md" | head -1 || true)
if [ -z "${line:-}" ]; then
    echo "document window not found; log:" >&2
    head -5 "$home/log" >&2
    pkill -f "$(basename "$binary")" 2>/dev/null || true
    exit 1
fi

id=$(echo "$line" | cut -f1)
screencapture -o -x -l "$id" "$out"
pkill -f "$(basename "$binary")" 2>/dev/null || true

if [ ! -s "$out" ]; then
    echo "capture produced nothing (window $id)" >&2
    exit 1
fi

echo "$out"

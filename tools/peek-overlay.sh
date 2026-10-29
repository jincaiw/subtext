#!/bin/sh
# Photograph a panel that lives in a dialog (a sheet over the window).
#
#   tools/peek-overlay.sh <step> [output.png]
#
# `screencapture -l <windowid>` does not capture a dialog: `window.open_dialog`
# draws it on a layer above the window, and `-l` captures the window's own
# backing store. So this photographs the screen and crops to the document
# window.
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

x=$(echo "$line" | cut -f2 | cut -d, -f1)
y=$(echo "$line" | cut -f2 | cut -d, -f2)
w=$(echo "$line" | cut -f3 | cut -d x -f1)
h=$(echo "$line" | cut -f3 | cut -d x -f2)

raw="$home/screen.png"
screencapture -x -o "$raw"
pkill -f "$(basename "$binary")" 2>/dev/null || true

# The PNG is twice the point size on a Retina display; pad by 40 points so the
# window's shadow is not clipped.
px=$((x * 2 - 80)); py=$((y * 2 - 80))
pw=$((w * 2 + 160)); ph=$((h * 2 + 160))
sips -c "$ph" "$pw" --cropOffset "$py" "$px" "$raw" --out "$out" >/dev/null

echo "$out"

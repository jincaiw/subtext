#!/bin/sh
# Photograph one of SubText's panels.
#
#   tools/peek-panel.sh <step> <window-title-substring> [output.png]
#
# `step` is the same number `SUBTEXT_PANEL_SWEEP` walks:
#
#   0  Custom Surround        7..15  Settings: General, Appearance, Window,
#   1  Edit Color Code                Edit, Mode, Format, Snippets,
#   2  Multiple Replace               Shortcuts, Integration
#   3  Warnings inspector    16      About
#   4  Document inspector    17      Console
#   5  Outline inspector     18      Sidebar: Folder Find
#   6  Find & Replace        19      Sidebar: File Browser
#                            20      Create Issue Report (a document)
#                            21      Quick Actions (floating panel)
#                            22      Customize Toolbar
#                            23      Donation
#
# The window is matched by title: "a.md" for the document, "Settings" for the
# settings window, "Console", "About SubText", "Donation".
#
# TWO THINGS THIS SCRIPT EXISTS TO GET RIGHT, both learned the hard way:
#
#  * Wait for the previous instance to really exit. `pkill` returning is not the
#    process being gone, and a stale window photographs identically every time —
#    three different source edits once produced the same image.
#  * `screencapture -l <windowid>` does NOT capture a dialog. `open_dialog` draws
#    on its own layer above the window, and `-l` captures the window's backing
#    store. For steps 0-2 and 22 use `tools/peek-overlay.sh`, which photographs
#    the screen and crops.
#
# Debug builds only: `SUBTEXT_PANEL_PEEK` is compiled out of release.
set -eu

if [ $# -lt 2 ]; then
    sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
fi

step=$1
want=$2
out=${3:-/tmp/subtext-panel-$step.png}
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

id=$(/tmp/winlist 2>/dev/null | grep -i "$want" | head -1 | cut -f1 || true)
if [ -z "${id:-}" ]; then
    echo "window matching '$want' not found; log:" >&2
    head -5 "$home/log" >&2
    pkill -f "$(basename "$binary")" 2>/dev/null || true
    exit 1
fi

screencapture -o -x -l "$id" "$out"
pkill -f "$(basename "$binary")" 2>/dev/null || true

echo "$out"

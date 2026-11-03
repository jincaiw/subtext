#!/bin/sh
# Photograph one of SubText's panels.
#
#   tools/peek-panel.sh <step> <window-title-substring> [output.png]
#
# `step` is the same number `SUBTEXT_PANEL_SWEEP` walks:
#
#   0  Custom Surround        16  About
#   1  Edit Color Code        17  Console
#   2  Multiple Replace       18  Sidebar: Folder Find
#   3  Warnings inspector     19  Sidebar: File Browser
#   4  Document inspector     20  Create Issue Report (a document)
#   5  Outline inspector      21  Quick Actions (floating panel)
#   6  Find & Replace         22  Customize Toolbar
#   7  Settings: General      23  Donation
#   8  Settings: Appearance
#   9  Settings: Window
#  10  Settings: Edit
#  11  Settings: Mode
#  12  Settings: Format
#  13  Settings: Snippets
#  14  Settings: Shortcuts
#  15  Settings: Integration
#
# The window is matched by title: "a.md" for the document, "Console", "About
# SubText", "Donation". The settings window takes the open pane's name as its
# title — upstream names the panes on a toolbar and lets the window title follow
# the selection — so settings steps pass the pane name, e.g.
# `tools/peek-panel.sh 14 Shortcuts`. The `Settings` window no longer exists
# under that title.
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

# `winlist` is a local helper (id, x,y, WxH, owner, title), not part of this
# repository. Override the path with WINLIST when it lives elsewhere, and say
# so plainly instead of sleeping for seven seconds and reporting "not found".
WINLIST="${WINLIST:-/tmp/winlist}"
if [ ! -x "$WINLIST" ]; then
    echo "error: $WINLIST is missing or not executable; set WINLIST=<path> to the window-list helper" >&2
    exit 1
fi

if [ $# -lt 2 ]; then
    sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
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

# Match the owner column as well as the title: this machine also runs CotEditor,
# whose document windows are titled `a.md`, so a bare title grep can photograph
# the wrong application. `winlist` prints
# `id<TAB>x,y<TAB>WxH<TAB>owner<TAB>title`; owner is the process name.
owner=$(basename "$binary")

# Window titles are localized (`Settings` is 设置 on a Chinese system), so the
# English names in the step table above are only the first candidate. The string
# tables next to this script are `English<TAB>translation`; look `want` up in
# each and try the translation too. Matching the English name must keep working
# on every system language, so this adds candidates rather than replacing one.
candidates=$want
for table in "$(dirname "$0")"/../resources/strings/*.tsv; do
    [ -f "$table" ] || continue
    while IFS='	' read -r key value; do
        if [ "$key" = "$want" ] && [ -n "${value:-}" ]; then
            candidates="$candidates
$value"
        fi
    done < "$table"
done

id=
while IFS= read -r candidate; do
    [ -n "$candidate" ] || continue
    id=$("$WINLIST" 2>/dev/null | awk -F'\t' -v o="$owner" -v w="$candidate" '
        index(tolower($4), tolower(o)) && index(tolower($5), tolower(w)) && $3 != "0x0" { print $1; exit }
    ' || true)
    [ -n "$id" ] && break
done <<EOF
$candidates
EOF

if [ -z "${id:-}" ]; then
    echo "window matching '$want' (tried $(echo "$candidates" | tr '\n' '|')) not found; log:" >&2
    head -5 "$home/log" >&2
    pkill -f "$(basename "$binary")" 2>/dev/null || true
    exit 1
fi

screencapture -o -x -l "$id" "$out"
pkill -f "$(basename "$binary")" 2>/dev/null || true

echo "$out"

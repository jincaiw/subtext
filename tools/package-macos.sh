#!/usr/bin/env bash
#
# Build a macOS application bundle for SubText.
#
#     tools/package-macos.sh            # release build, then bundle
#     tools/package-macos.sh --no-build # bundle the binary that is already there
#
# The result is dist/SubText.app, plus dist/SubText-<version>-<arch>.zip.
#
# Why a bundle at all: GPUI's macOS backend is an AppKit application, and AppKit
# only gives an application a menu bar, a Dock icon and the ability to take
# focus if it is launched from a bundle with an Info.plist. A bare Mach-O
# binary works for development but behaves like a background process.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BUNDLE_NAME="SubText"
BINARY_NAME="subtext"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUNDLE_ID="com.subtext.editor"
MIN_MACOS="11.0"

DIST="$ROOT/dist"
APP="$DIST/$BUNDLE_NAME.app"
CONTENTS="$APP/Contents"

BUILD=1
for argument in "$@"; do
    case "$argument" in
        --no-build) BUILD=0 ;;
        -h|--help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $argument" >&2; exit 2 ;;
    esac
done

ARCH="$(uname -m)"
TARGET="$(rustc -vV | sed -n 's/^host: //p')"

if [ "$BUILD" = 1 ]; then
    echo "==> building $BINARY_NAME $VERSION for $TARGET (release)"
    cargo build --release --bin "$BINARY_NAME"
fi

BINARY="$ROOT/target/release/$BINARY_NAME"
if [ ! -f "$BINARY" ]; then
    echo "error: $BINARY does not exist — run without --no-build" >&2
    exit 1
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

# The executable. Resources::discover() looks beside it and then one level up
# in Contents/Resources, so both locations are valid; only the latter is used.
cp "$BINARY" "$CONTENTS/MacOS/$BINARY_NAME"
chmod +x "$CONTENTS/MacOS/$BINARY_NAME"

# The data files. These are CotEditor's, kept as files rather than compiled in,
# so a user can override any of them from their own support directory.
for resource in syntaxes themes queries; do
    if [ -d "$ROOT/resources/$resource" ]; then
        cp -R "$ROOT/resources/$resource" "$CONTENTS/Resources/$resource"
    fi
done
cp "$ROOT/resources/menu.json" "$CONTENTS/Resources/menu.json"

# The licence and the changelog, because the licence asks for it and because a
# user who has the .app has no other way to read them.
cp "$ROOT/LICENSE" "$CONTENTS/Resources/LICENSE"
cp "$ROOT/CHANGELOG.md" "$CONTENTS/Resources/CHANGELOG.md"

if [ -f "$ROOT/dist/icon/SubText.icns" ]; then
    ICON_READY=1
elif [ "$(uname -s)" = "Darwin" ] && command -v python3 >/dev/null 2>&1; then
    # The icon is drawn, not stored: make_icon.py renders it from geometry, so
    # there is no reason to keep a binary blob in the repository.
    echo "==> drawing the icon (tools/make_icon.py)"
    python3 "$ROOT/tools/make_icon.py" >/dev/null
    ICON_READY=1
else
    ICON_READY=0
fi

if [ "$ICON_READY" = 1 ]; then
    cp "$ROOT/dist/icon/SubText.icns" "$CONTENTS/Resources/SubText.icns"
    ICON_ENTRY='    <key>CFBundleIconFile</key>
    <string>SubText</string>'
else
    ICON_ENTRY=''
fi

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$BUNDLE_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$BUNDLE_NAME</string>
    <key>CFBundleExecutable</key>
    <string>$BINARY_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleSignature</key>
    <string>????</string>
$ICON_ENTRY
    <key>LSMinimumSystemVersion</key>
    <string>$MIN_MACOS</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright 2026 The SubText Authors. Apache-2.0.</string>

    <!-- SubText is a plain text editor: it opens anything, and claims nothing
         exclusively. The editor role is Editor, not Viewer, so the Finder's
         "Open With" list puts it where a user would look for it. -->
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Plain Text Document</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSHandlerRank</key>
            <string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.plain-text</string>
                <string>public.text</string>
                <string>public.source-code</string>
                <string>public.data</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# An ad-hoc signature is enough for local use and keeps macOS from treating the
# bundle as damaged. A real release would sign with a Developer ID instead.
if command -v codesign >/dev/null 2>&1; then
    echo "==> ad-hoc signing"
    codesign --force --deep --sign - "$APP" 2>/dev/null \
        || echo "    (codesign failed; the bundle still runs, but Gatekeeper may complain)"
fi

echo "==> verifying"
if [ -d "$CONTENTS/Resources/syntaxes" ]; then
    echo "    syntaxes: $(find "$CONTENTS/Resources/syntaxes" -name '*.cotsyntax' -maxdepth 1 | wc -l | tr -d ' ')"
fi
echo "    themes:   $(find "$CONTENTS/Resources/themes" -name '*.cottheme' -maxdepth 1 | wc -l | tr -d ' ')"
echo "    binary:   $(du -h "$CONTENTS/MacOS/$BINARY_NAME" | cut -f1)"
echo "    plist:    $(plutil -lint "$CONTENTS/Info.plist" >/dev/null 2>&1 && echo ok || echo 'CHECK FAILED')"

ARCHIVE="$DIST/$BUNDLE_NAME-$VERSION-$ARCH.zip"
echo "==> archiving $ARCHIVE"
rm -f "$ARCHIVE"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ARCHIVE"

echo
echo "done."
echo "  bundle:  $APP"
echo "  archive: $ARCHIVE"
echo "  run:     open '$APP'"

#!/usr/bin/env bash
#
# Build a macOS application bundle for SubText.
#
#     tools/package-macos.sh                     # release build for the host, bundle, zip, dmg
#     tools/package-macos.sh --no-build          # package the binary that is already there
#     tools/package-macos.sh --target x86_64-apple-darwin
#     tools/package-macos.sh --target aarch64-apple-darwin --no-build
#
# The result is dist/SubText.app, dist/SubText-<version>-<arch>.zip and
# dist/SubText-<version>-<arch>.dmg.
#
# Signing and notarization are opt-in through the environment; without them the
# bundle is ad-hoc signed, which is enough to run it locally but not enough for
# Gatekeeper on another Mac:
#
#     APPLE_SIGNING_IDENTITY="Developer ID Application: …"
#     APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD   (for notarytool)
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
TARGET=""
while [ $# -gt 0 ]; do
    case "$1" in
        --no-build) BUILD=0; shift ;;
        --target)
            if [ $# -lt 2 ]; then
                echo "error: --target needs a triple, e.g. --target aarch64-apple-darwin" >&2
                exit 2
            fi
            TARGET="$2"; shift 2 ;;
        --target=*) TARGET="${1#--target=}"; shift ;;
        -h|--help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [ -n "$TARGET" ]; then
    BINARY="$ROOT/target/$TARGET/release/$BINARY_NAME"
    case "$TARGET" in
        *aarch64*) ARCH="arm64" ;;
        *x86_64*) ARCH="x86_64" ;;
        *) echo "error: cannot name the architecture of $TARGET" >&2; exit 2 ;;
    esac
    # The update checker matches assets by this suffix; see update.rs.
else
    BINARY="$ROOT/target/release/$BINARY_NAME"
    case "$(uname -m)" in
        arm64|aarch64) ARCH="arm64" ;;
        x86_64) ARCH="x86_64" ;;
        *) echo "error: unsupported host architecture $(uname -m)" >&2; exit 2 ;;
    esac
fi

if [ "$BUILD" = 1 ]; then
    echo "==> building $BINARY_NAME $VERSION for ${TARGET:-the host} (release)"
    if [ -n "$TARGET" ]; then
        cargo build --release --bin "$BINARY_NAME" --target "$TARGET"
    else
        cargo build --release --bin "$BINARY_NAME"
    fi
fi

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
[ -f "$ROOT/THIRD-PARTY-NOTICES.md" ] && cp "$ROOT/THIRD-PARTY-NOTICES.md" "$CONTENTS/Resources/"

if [ -f "$ROOT/dist/icon/SubText.icns" ]; then
    ICON_READY=1
elif command -v python3 >/dev/null 2>&1; then
    # The icon is drawn, not stored: make_icon.py renders it from geometry, so
    # there is no reason to keep a binary blob in the repository.
    echo "==> drawing the icon (tools/make_icon.py)"
    python3 "$ROOT/tools/make_icon.py" >/dev/null
    ICON_READY=1
else
    ICON_READY=0
fi

if [ "$ICON_READY" = 1 ] && [ -f "$ROOT/dist/icon/SubText.icns" ]; then
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

# A Developer ID signature with the hardened runtime is what Gatekeeper wants;
# without an identity, an ad-hoc signature keeps macOS from calling the bundle
# damaged, but it will still be refused on a machine that did not build it.
SIGN_ID="${APPLE_SIGNING_IDENTITY:-}"
if command -v codesign >/dev/null 2>&1; then
    if [ -n "$SIGN_ID" ]; then
        echo "==> signing with $SIGN_ID"
        codesign --force --deep --options runtime --timestamp --sign "$SIGN_ID" "$APP"
        codesign --verify --deep --strict "$APP"
    else
        echo "==> ad-hoc signing (set APPLE_SIGNING_IDENTITY for a distributable build)"
        codesign --force --deep --sign - "$APP" 2>/dev/null \
            || echo "    (codesign failed; the bundle still runs, but Gatekeeper may complain)"
    fi
fi

echo "==> verifying"
if [ -d "$CONTENTS/Resources/syntaxes" ]; then
    echo "    syntaxes: $(find "$CONTENTS/Resources/syntaxes" -name '*.cotsyntax' -maxdepth 1 | wc -l | tr -d ' ')"
fi
echo "    themes:   $(find "$CONTENTS/Resources/themes" -name '*.cottheme' -maxdepth 1 | wc -l | tr -d ' ')"
echo "    binary:   $(du -h "$CONTENTS/MacOS/$BINARY_NAME" | cut -f1)"
echo "    arch:     $(lipo -archs "$CONTENTS/MacOS/$BINARY_NAME" 2>/dev/null || echo unknown)"
echo "    plist:    $(plutil -lint "$CONTENTS/Info.plist" >/dev/null 2>&1 && echo ok || echo 'CHECK FAILED')"

ZIP="$DIST/$BUNDLE_NAME-$VERSION-$ARCH.zip"
DMG="$DIST/$BUNDLE_NAME-$VERSION-$ARCH.dmg"

# The zip first: a notarization ticket is stapled to the bundle, so the archive
# that ships has to be made after the stapling step.
make_zip() {
    echo "==> archiving $ZIP"
    rm -f "$ZIP"
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"
}

make_dmg() {
    local stage="$DIST/dmg-root"
    echo "==> building $DMG"
    rm -rf "$stage" "$DMG"
    mkdir -p "$stage"
    cp -R "$APP" "$stage/"
    ln -s /Applications "$stage/Applications"
    # `hdiutil` intermittently exits 1 right after the bundle is copied — the
    # GitHub runner gives "Resource busy" while the new files are still being
    # indexed, and `-quiet` hides it. Retrying is what turns that into a slower
    # build instead of a failed release; the output is kept for the error.
    local attempt=1
    until hdiutil create -volname "$BUNDLE_NAME" -srcfolder "$stage" -ov \
            -format UDZO "$DMG"; do
        rm -f "$DMG"
        if [ "$attempt" -ge 3 ]; then
            rm -rf "$stage"
            echo "error: hdiutil create failed $attempt times" >&2
            return 1
        fi
        attempt=$((attempt + 1))
        echo "hdiutil failed; retrying ($attempt/3)" >&2
        sleep 5
    done
    rm -rf "$stage"
}

make_zip
make_dmg

if [ -n "$SIGN_ID" ] && [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] \
        && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    echo "==> notarizing (this can take a few minutes)"
    xcrun notarytool submit "$ZIP" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$APP"
    # Rebuild both archives from the stapled bundle, then notarize the DMG too,
    # because a user who downloads the DMG gets the ticket with it.
    make_zip
    make_dmg
    xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$DMG"
    spctl --assess --type execute --verbose=2 "$APP" || true
else
    echo "note: not notarized (set APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_TEAM_ID,"
    echo "      APPLE_APP_PASSWORD to sign and notarize a distributable build)"
fi

echo
echo "done."
echo "  bundle:  $APP"
echo "  archive: $ZIP"
echo "  disk:    $DMG"
echo "  run:     open '$APP'"

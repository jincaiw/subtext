#!/usr/bin/env bash
#
# Package a Linux build of SubText.
#
#     tools/package-linux.sh [binary] [--strict]
#     # default binary: target/release/subtext
#
# The portable archive is always produced:
#     dist/SubText-<version>-linux-<arch>.tar.gz
# and when the matching tool is installed, so are:
#     dist/SubText-<version>-linux-<arch>.deb        (dpkg-deb)
#     dist/SubText-<version>-linux-<arch>.rpm        (rpmbuild)
#     dist/SubText-<version>-linux-<arch>.AppImage   (appimagetool, or $APPIMAGETOOL)
#
# `--strict` makes a missing packaging tool an error. The release workflow uses
# it; a developer machine without rpmbuild can just build the tar.gz.
#
# This script does not run `cargo build` itself. Linux binaries are built on a
# Linux host (or in a Linux container), and packaging is the same on either
# side of that build.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

STRICT=0
BINARY=""
for argument in "$@"; do
    case "$argument" in
        --strict) STRICT=1 ;;
        -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "unknown option: $argument" >&2; exit 2 ;;
        *) BINARY="$argument" ;;
    esac
done
BINARY="${BINARY:-$ROOT/target/release/subtext}"

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
case "$ARCH" in
    x86_64) DEB_ARCH="amd64" ;;
    aarch64) DEB_ARCH="arm64" ;;
    *) echo "error: unsupported architecture $ARCH" >&2; exit 2 ;;
esac

NAME="SubText-$VERSION-linux-$ARCH"
STAGE="$ROOT/dist/$NAME"
DIST="$ROOT/dist"
MAINTAINER="The SubText Authors <maintainers@subtext.invalid>"
HOMEPAGE="https://github.com/jincaiw/subtext"

need() {
    if ! command -v "$1" >/dev/null 2>&1; then
        if [ "$STRICT" = 1 ]; then
            echo "error: $1 is required for a complete Linux release" >&2
            exit 1
        fi
        echo "note: $1 is not installed; skipping $2" >&2
        return 1
    fi
    return 0
}

# --- the icon -------------------------------------------------------------

ICON_DIR="$ROOT/dist/icon"
ICON_PNG="$ICON_DIR/SubText.iconset/icon_256x256.png"
if [ ! -f "$ICON_PNG" ]; then
    if command -v python3 >/dev/null 2>&1; then
        echo "==> drawing the icon (tools/make_icon.py)"
        python3 "$ROOT/tools/make_icon.py" >/dev/null
    else
        echo "error: the Linux packages need a 256x256 icon, and python3 is missing" >&2
        exit 1
    fi
fi
if [ ! -f "$ICON_PNG" ]; then
    echo "error: tools/make_icon.py did not produce $ICON_PNG" >&2
    exit 1
fi

# --- the portable archive -------------------------------------------------

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
if [ -f "$ROOT/THIRD-PARTY-NOTICES.md" ]; then
    cp "$ROOT/THIRD-PARTY-NOTICES.md" "$STAGE/"
fi

ARCHIVE="$DIST/$NAME.tar.gz"
echo "==> archiving $ARCHIVE"
rm -f "$ARCHIVE"
tar -czf "$ARCHIVE" -C "$DIST" "$NAME"

# --- a shared system tree -------------------------------------------------
#
# `/usr/bin/subtext` finds its data at `/usr/share/subtext/resources`
# (`Resources::discover` looks one level above the executable), which is the
# same layout in a .deb, an .rpm and an AppImage.

DESKTOP_ENTRY='[Desktop Entry]
Type=Application
Name=SubText
GenericName=Text Editor
Comment=A plain text editor, reimplemented from CotEditor
Exec=subtext %F
TryExec=subtext
Icon=subtext
Terminal=false
Categories=Utility;TextEditor;
MimeType=text/plain;
StartupNotify=true
StartupWMClass=subtext
'

fill_tree() {
    local root="$1"
    mkdir -p "$root/usr/bin" \
             "$root/usr/share/subtext/resources" \
             "$root/usr/share/applications" \
             "$root/usr/share/icons/hicolor/256x256/apps" \
             "$root/usr/share/doc/subtext"
    install -m 0755 "$BINARY" "$root/usr/bin/subtext"
    for resource in syntaxes themes queries; do
        if [ -d "$ROOT/resources/$resource" ]; then
            cp -R "$ROOT/resources/$resource" "$root/usr/share/subtext/resources/"
        fi
    done
    cp "$ROOT/resources/menu.json" "$root/usr/share/subtext/resources/menu.json"
    install -m 0644 "$ICON_PNG" "$root/usr/share/icons/hicolor/256x256/apps/subtext.png"
    printf '%s' "$DESKTOP_ENTRY" > "$root/usr/share/applications/subtext.desktop"
    install -m 0644 "$ROOT/LICENSE" "$root/usr/share/doc/subtext/LICENSE"
    install -m 0644 "$ROOT/README.md" "$root/usr/share/doc/subtext/README.md"
    install -m 0644 "$ROOT/CHANGELOG.md" "$root/usr/share/doc/subtext/CHANGELOG.md"
    if [ -f "$ROOT/THIRD-PARTY-NOTICES.md" ]; then
        install -m 0644 "$ROOT/THIRD-PARTY-NOTICES.md" "$root/usr/share/doc/subtext/"
    fi
}

# --- .deb -----------------------------------------------------------------

if need dpkg-deb "the .deb"; then
    DEB_ROOT="$DIST/deb-root"
    rm -rf "$DEB_ROOT"
    fill_tree "$DEB_ROOT"
    mkdir -p "$DEB_ROOT/DEBIAN"
    SIZE_KB="$(du -sk "$DEB_ROOT/usr" | cut -f1)"
    cat > "$DEB_ROOT/DEBIAN/control" <<CONTROL
Package: subtext
Version: $VERSION
Section: editors
Priority: optional
Architecture: $DEB_ARCH
Maintainer: $MAINTAINER
Installed-Size: $SIZE_KB
Homepage: $HOMEPAGE
Depends: libc6 (>= 2.31), libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libxcb1, libxcb-render0, libxcb-shape0, libxcb-xfixes0, libfontconfig1, libfreetype6, libvulkan1
Description: A plain text editor built with GPUI
 SubText is a faithful reimplementation of CotEditor on GPUI Kit: the same
 menus, shortcuts, syntax definitions and themes, on macOS, Linux and Windows.
CONTROL
    DEB="$DIST/$NAME.deb"
    echo "==> building $DEB"
    rm -f "$DEB"
    dpkg-deb --build --root-owner-group "$DEB_ROOT" "$DEB" >/dev/null
    rm -rf "$DEB_ROOT"
fi

# --- .rpm -----------------------------------------------------------------

if need rpmbuild "the .rpm"; then
    RPM_ROOT="$DIST/rpmbuild"
    RPM_TREE="$DIST/rpm-tree"
    rm -rf "$RPM_ROOT" "$RPM_TREE"
    fill_tree "$RPM_TREE"
    # rpmbuild expects the whole topdir even though this spec only uses SPECS
    # and the install tree.
    mkdir -p "$RPM_ROOT"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    cat > "$RPM_ROOT/SPECS/subtext.spec" <<SPEC
Name:           subtext
Version:        $VERSION
Release:        1
Summary:        A plain text editor built with GPUI
License:        Apache-2.0
URL:            $HOMEPAGE
BuildArch:      $ARCH

%description
SubText is a faithful reimplementation of CotEditor on GPUI Kit: the same
menus, shortcuts, syntax definitions and themes, on macOS, Linux and Windows.

%install
mkdir -p %{buildroot}
cp -R "$RPM_TREE"/usr %{buildroot}/

%files
/usr/bin/subtext
/usr/share/subtext
/usr/share/applications/subtext.desktop
/usr/share/icons/hicolor/256x256/apps/subtext.png
/usr/share/doc/subtext/LICENSE
/usr/share/doc/subtext/README.md
/usr/share/doc/subtext/CHANGELOG.md
/usr/share/doc/subtext/THIRD-PARTY-NOTICES.md

%changelog
SPEC
    RPM="$DIST/$NAME.rpm"
    echo "==> building $RPM"
    rm -f "$RPM"
    rpmbuild -bb "$RPM_ROOT/SPECS/subtext.spec" \
        --define "_topdir $RPM_ROOT" \
        --define "_rpmdir $DIST" \
        --define "_build_id_links none" >/dev/null
    # rpmbuild writes into $DIST/<arch>/; move it to the predictable name.
    mv "$DIST/$ARCH/subtext-$VERSION-1.$ARCH.rpm" "$RPM"
    rmdir "$DIST/$ARCH" 2>/dev/null || true
    rm -rf "$RPM_ROOT" "$RPM_TREE"
fi

# --- AppImage -------------------------------------------------------------

APPIMAGETOOL="${APPIMAGETOOL:-}"
if [ -z "$APPIMAGETOOL" ] && command -v appimagetool >/dev/null 2>&1; then
    APPIMAGETOOL="$(command -v appimagetool)"
fi
if [ -n "$APPIMAGETOOL" ] && [ -x "$APPIMAGETOOL" ]; then
    APPDIR="$DIST/SubText.AppDir"
    rm -rf "$APPDIR"
    fill_tree "$APPDIR"
    ln -s usr/bin/subtext "$APPDIR/AppRun"
    cp "$APPDIR/usr/share/applications/subtext.desktop" "$APPDIR/subtext.desktop"
    cp "$ICON_PNG" "$APPDIR/subtext.png"
    APPIMAGE="$DIST/$NAME.AppImage"
    echo "==> building $APPIMAGE"
    rm -f "$APPIMAGE"
    # --appimage-extract-and-run works on machines without FUSE, which is what
    # GitHub's runners have.
    ARCH="$ARCH" "$APPIMAGETOOL" --appimage-extract-and-run --no-appstream \
        "$APPDIR" "$APPIMAGE" >/dev/null
    rm -rf "$APPDIR"
else
    if [ "$STRICT" = 1 ]; then
        echo "error: appimagetool is required for a complete Linux release" >&2
        exit 1
    fi
    echo "note: appimagetool is not installed; skipping the .AppImage" >&2
fi

echo
echo "done."
echo "  directory: $STAGE"
echo "  archive:   $ARCHIVE"
for extra in "$DIST/$NAME.deb" "$DIST/$NAME.rpm" "$DIST/$NAME.AppImage"; do
    if [ -f "$extra" ]; then
        echo "  package:   $extra"
    fi
done

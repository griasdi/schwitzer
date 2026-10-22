#!/bin/sh
# Packs target/release/schwitzer[.exe] into dist/ under the names the app's
# updater (crates/schwitzer-app/src/update.rs) looks for, and the server built for
# musl under the name the server's updater (crates/schwitzer-server/src/update.rs) looks for:
#
#   linux-x86_64, linux-aarch64   schwitzer-<version>-<platform>.tar.gz with `schwitzer`,
#                                 schwitzer-<version>-linux-<arch>.AppImage
#   macos-aarch64                 schwitzer-<version>-macos-aarch64.tar.gz with `Schwitzer.app`
#   windows-x86_64                schwitzer-<version>-windows-x86_64.zip with `schwitzer.exe`,
#                                 schwitzer-<version>-windows-x86_64-setup.exe (Inno Setup)
#   server-linux-x86_64,          schwitzer-server-<version>-linux-<arch>.tar.gz with
#   server-linux-aarch64          `schwitzer-server` (static, musl), from
#                                 target/<arch>-unknown-linux-musl/release
#
# Usage: package.sh <platform> <version>
set -eu

platform=$1
version=$2
root=$(cd "$(dirname "$0")/.." && pwd)
name=schwitzer-$version-$platform
mkdir -p "$root/dist"

# Downloads $2 to $1 unless $1 is there with sha256 $3, and checks the digest.
fetch() {
    if [ -f "$1" ] && printf '%s  %s\n' "$3" "$1" | sha256sum -c --quiet >/dev/null 2>&1; then
        return
    fi
    curl -fsSL -o "$1.part" "$2"
    printf '%s  %s\n' "$3" "$1.part" | sha256sum -c --quiet
    mv "$1.part" "$1"
}

# Builds dist/$name.AppImage for the architecture $1 (x86_64 or aarch64). No update
# information: the app updates the AppImage itself (src/update.rs).
appimage() {
    case $1 in
    x86_64)
        tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
        runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
        ;;
    aarch64)
        tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
        runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
        ;;
    esac
    tools=$root/target/appimage-tools
    mkdir -p "$tools"
    tool=$tools/appimagetool-1.9.1-$1.AppImage
    runtime=$tools/runtime-20251108-$1
    fetch "$tool" "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$1.AppImage" "$tool_sha"
    fetch "$runtime" "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$1" "$runtime_sha"
    chmod +x "$tool"

    stage=$(mktemp -d)
    appdir=$stage/AppDir
    mkdir -p "$appdir/usr/bin" "$appdir/usr/share/icons/hicolor/256x256/apps"
    cp "$root/target/release/schwitzer" "$appdir/usr/bin/schwitzer"
    ln -s usr/bin/schwitzer "$appdir/AppRun"
    cp "$root/deploy/schwitzer.desktop" "$appdir/schwitzer.desktop"
    png=$root/crates/schwitzer-app/assets/logo/schwitzer.png
    cp "$png" "$appdir/schwitzer.png"
    ln -s schwitzer.png "$appdir/.DirIcon"
    cp "$png" "$appdir/usr/share/icons/hicolor/256x256/apps/schwitzer.png"
    # Extract-and-run: CI runners have no FUSE. The pinned runtime spares appimagetool its own download.
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$1 "$tool" --no-appstream --runtime-file "$runtime" "$appdir" "$root/dist/$name.AppImage"
    rm -rf "$stage"
}

case $platform in
linux-x86_64 | linux-aarch64)
    tar -czf "$root/dist/$name.tar.gz" -C "$root/target/release" schwitzer
    appimage "${platform#linux-}"
    ;;
macos-aarch64)
    # The arm64 linker signs the binary ad hoc. No bundle seal: the updater swaps the
    # binary inside, which would break it.
    stage=$(mktemp -d)
    mkdir -p "$stage/Schwitzer.app/Contents/MacOS"
    mkdir -p "$stage/Schwitzer.app/Contents/Resources"
    cp "$root/crates/schwitzer-app/assets/logo/schwitzer.icns" "$stage/Schwitzer.app/Contents/Resources/schwitzer.icns"
    cp "$root/target/release/schwitzer" "$stage/Schwitzer.app/Contents/MacOS/schwitzer"
    cat >"$stage/Schwitzer.app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>schwitzer</string>
    <key>CFBundleIconFile</key>
    <string>schwitzer</string>
    <key>CFBundleIdentifier</key>
    <string>net.griasdi.schwitzer</string>
    <key>CFBundleName</key>
    <string>Schwitzer</string>
    <key>CFBundleDisplayName</key>
    <string>Schwitzer</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
    tar -czf "$root/dist/$name.tar.gz" -C "$stage" Schwitzer.app
    rm -rf "$stage"
    ;;
windows-x86_64)
    # 7z is a native Windows program under Git Bash: relative paths only.
    (cd "$root/target/release" && 7z a -tzip "../../dist/$name.zip" schwitzer.exe >/dev/null)
    iscc=${ISCC:-}
    if [ -z "$iscc" ]; then
        iscc=$(command -v iscc || command -v ISCC.exe || true)
    fi
    if [ -z "$iscc" ]; then
        for candidate in \
            "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" \
            "/c/Program Files/Inno Setup 6/ISCC.exe" \
            "/c/Program Files (x86)/Inno Setup 7/ISCC.exe" \
            "/c/Program Files/Inno Setup 7/ISCC.exe"; do
            if [ -f "$candidate" ]; then
                iscc=$candidate
                break
            fi
        done
    fi
    if [ -z "$iscc" ]; then
        echo "package.sh: Inno Setup not found: choco install innosetup, or set ISCC" >&2
        exit 1
    fi
    # MSYS_NO_PATHCONV: Git Bash would take the /Q, /D and /F switches for paths.
    (cd "$root" && MSYS_NO_PATHCONV=1 "$iscc" /Q "/DVersion=$version" "/F$name-setup" deploy/schwitzer.iss)
    ;;
server-linux-x86_64 | server-linux-aarch64)
    arch=${platform#server-linux-}
    tar -czf "$root/dist/schwitzer-server-$version-linux-$arch.tar.gz" \
        -C "$root/target/$arch-unknown-linux-musl/release" schwitzer-server
    ;;
*)
    echo "package.sh: unknown platform $platform" >&2
    exit 1
    ;;
esac

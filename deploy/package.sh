#!/bin/sh
# Packs target/release/schwitzer[.exe] into dist/ under the names the app's
# updater (crates/schwitzer-app/src/update.rs) looks for:
#
#   linux-x86_64, linux-aarch64   schwitzer-<version>-<platform>.tar.gz with `schwitzer`
#   macos-aarch64                 schwitzer-<version>-macos-aarch64.tar.gz with `Schwitzer.app`
#   windows-x86_64                schwitzer-<version>-windows-x86_64.zip with `schwitzer.exe`
#
# Usage: package.sh <platform> <version>
set -eu

platform=$1
version=$2
root=$(cd "$(dirname "$0")/.." && pwd)
name=schwitzer-$version-$platform
mkdir -p "$root/dist"

case $platform in
linux-x86_64 | linux-aarch64)
    tar -czf "$root/dist/$name.tar.gz" -C "$root/target/release" schwitzer
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
    ;;
*)
    echo "package.sh: unknown platform $platform" >&2
    exit 1
    ;;
esac

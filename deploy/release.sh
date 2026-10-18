#!/bin/sh
# Releases the desktop app: sets the workspace version (a "Release <version>"
# commit when it changes), prints the release notes and pushes the tag
# v<version>. The tag starts .github/workflows/release.yml, which builds the
# app for every platform and publishes the GitHub release the app updates
# from.
#
# Run it on main, with HEAD pushed and no uncommitted changes to tracked files.
#
# Usage: release.sh <version>
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

say() { printf '[release] %s\n' "$*"; }
die() {
    say "$*"
    exit 1
}

[ $# -eq 1 ] || die "usage: release.sh <version>"
version=$1
expr "$version" : '[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$' >/dev/null ||
    die "$version is not a version like 1.2.3"

[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || die "not on main"
git diff --quiet HEAD -- || die "tracked files have uncommitted changes"
git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
    die "HEAD is not origin/main: push or pull first"

tag=v$version
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag exists"
if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
    die "tag $tag exists on origin"
fi

# The first `version =` line is the one in [workspace.package].
current=$(awk -F'"' '/^version = /{print $2; exit}' Cargo.toml)
if [ "$current" != "$version" ]; then
    newest=$(printf '%s\n%s\n' "$current" "$version" | sort -V | tail -n 1)
    [ "$newest" = "$version" ] || die "$version is older than $current"
    awk -v v="$version" '!d && /^version = "/ {print "version = \"" v "\""; d=1; next} {print}' \
        Cargo.toml >Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
    cargo update --workspace
    git commit -q -m "Release $version" -- Cargo.toml Cargo.lock
    git push -q origin main
    say "version $current -> $version committed and pushed"
fi

prev=$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || true)
say "notes:"
deploy/changelog.sh "${prev:+$prev..}HEAD"

git tag -a "$tag" -m "Schwitzer $version"
git push -q origin "$tag"
say "$tag pushed; the release workflow builds it: https://github.com/griasdi/schwitzer/actions"

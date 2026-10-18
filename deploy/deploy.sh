#!/bin/sh
# Deploys the committed HEAD of this checkout (Linux, systemd):
#
#   local    builds the server in release mode and runs it as the systemd user
#            service `schwitzer-server-local` on 127.0.0.1:7444, for use on
#            this machine; restarts it only when binary or unit changed. The
#            binary lives in ~/.local/state/schwitzer-server-local/bin, the
#            data (identity, rooms) in ~/.local/share/schwitzer-server.
#
# It builds from an export of HEAD, so uncommitted changes in the checkout
# are ignored: what runs is what is committed.
#
# Usage: deploy.sh local
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cache=${XDG_CACHE_HOME:-$HOME/.cache}/schwitzer-server-local
state=${XDG_STATE_HOME:-$HOME/.local/state}/schwitzer-server-local
bin=$state/bin/schwitzer-server
data=${XDG_DATA_HOME:-$HOME/.local/share}/schwitzer-server
unit_file=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/schwitzer-server-local.service
service=schwitzer-server-local.service

say() { printf '[deploy] %s\n' "$*"; }

journal() {
    journalctl -q --user -u schwitzer-server-local -o cat --no-pager "$@"
}

deploy_local() {
    rev=$(git -C "$root" rev-parse --short HEAD)
    rm -rf "$cache/src"
    mkdir -p "$cache/src"
    git -C "$root" archive HEAD | tar -x -C "$cache/src"
    say "local: building $rev"
    cargo build --release --locked -p schwitzer-server \
        --manifest-path "$cache/src/Cargo.toml" --target-dir "$cache/target"

    changed=
    built=$cache/target/release/schwitzer-server
    if ! cmp -s "$built" "$bin"; then
        install -D -m 755 "$built" "$bin"
        changed=binary
    fi
    unit=$(sed -e "s|@BIN@|$bin|" -e "s|@DATA@|$data|" "$cache/src/deploy/schwitzer-server-local.service")
    if [ "$unit" != "$(cat "$unit_file" 2>/dev/null || true)" ]; then
        mkdir -p "$(dirname "$unit_file")"
        printf '%s\n' "$unit" >"$unit_file"
        systemctl --user daemon-reload
        changed="${changed:+$changed, }unit"
    fi
    mkdir -p "$data"
    systemctl --user enable --quiet "$service"

    if [ -z "$changed" ] && systemctl --user is-active --quiet "$service"; then
        say "local: unchanged, still running"
    else
        if ! systemctl --user is-active --quiet "$service"; then
            taken=$(ss -Hulpn 'sport = :7444')
            if [ -n "$taken" ]; then
                say "local: 127.0.0.1:7444 is taken by another process; stop it first:"
                printf '%s\n' "$taken"
                exit 1
            fi
        fi
        say "local: restarting (${changed:-was not running})"
        since=$(date +%s)
        systemctl --user restart "$service"
        tries=0
        until [ -n "$(journal --since "@$since" --grep '^server url:')" ]; do
            tries=$((tries + 1))
            if [ "$tries" -ge 50 ] || ! systemctl --user is-active --quiet "$service"; then
                say "local: did not come up"
                journal --since "@$since" | tail -n 20
                exit 1
            fi
            sleep 0.2
        done
    fi
    say "local: $(journal --grep '^server url:' -n 1)"
}

case "${1:-}" in
local) deploy_local ;;
*)
    say "usage: $0 local"
    exit 2
    ;;
esac
say "done"

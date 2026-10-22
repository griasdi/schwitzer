#!/bin/sh
# Deploys the committed HEAD of this checkout as a systemd user service:
#
#   stkn     on stkn.org, over ssh with your ssh user: `schwitzer-server` on UDP 7444
#            (IPv4 and IPv6), public host stkn.org. Enables lingering there,
#            so the service runs without a login and starts at boot.
#
# It builds the server in release mode on this machine, from an export of
# HEAD: uncommitted changes in the checkout are ignored, what runs is what is
# committed. The service restarts only when binary or unit changed. On the
# target the binary lives in ~/.local/state/<service>/bin, the data (identity,
# rooms) in ~/.local/share/schwitzer-server.
#
# Usage: deploy.sh stkn
#
# `deploy.sh activate …` is the half that runs on the target; `stkn` calls
# it.
#
# Run by the user only, to install the service or change its unit; agents never
# run it. The service then updates itself (--auto-update).
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cache=${XDG_CACHE_HOME:-$HOME/.cache}/schwitzer-deploy
stkn_ssh=stkn.org
# Where `stkn` uploads binary and unit template, relative to the remote home.
stkn_stage=.cache/schwitzer-server-deploy

say() { printf '[deploy] %s\n' "$*"; }

# Exports HEAD to $cache/src and builds it there; sets $src and $built.
build() {
    rev=$(git -C "$root" rev-parse --short HEAD)
    src=$cache/src
    rm -rf "$src"
    mkdir -p "$src"
    git -C "$root" archive HEAD | tar -x -C "$src"
    say "building $rev"
    cargo build --release --locked -p schwitzer-server \
        --manifest-path "$src/Cargo.toml" --target-dir "$cache/target"
    built=$cache/target/release/schwitzer-server
}

# activate <label> <service> <listen> <public host> <binary> <unit template>
#
# Installs binary and unit as the systemd user service <service> on this
# machine and (re)starts it when either changed or it is not running.
activate() {
    label=$1 service=$2 listen=$3 public_host=$4 new_bin=$5 template=$6
    bin=${XDG_STATE_HOME:-$HOME/.local/state}/$service/bin/schwitzer-server
    data=${XDG_DATA_HOME:-$HOME/.local/share}/schwitzer-server
    unit_file=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$service.service
    port=${listen##*:}
    journal() { journalctl -q --user -u "$service" -o cat --no-pager "$@"; }

    changed=
    if ! cmp -s "$new_bin" "$bin"; then
        install -D -m 755 "$new_bin" "$bin"
        changed=binary
    fi
    unit=$(sed -e "s|@BIN@|$bin|" -e "s|@DATA@|$data|" -e "s|@LISTEN@|$listen|" \
        -e "s|@PUBLIC_HOST@|$public_host|" "$template")
    if [ "$unit" != "$(cat "$unit_file" 2>/dev/null || true)" ]; then
        mkdir -p "$(dirname "$unit_file")"
        printf '%s\n' "$unit" >"$unit_file"
        systemctl --user daemon-reload
        changed="${changed:+$changed, }unit"
    fi
    mkdir -p "$data"
    systemctl --user enable --quiet "$service"

    if [ -z "$changed" ] && systemctl --user is-active --quiet "$service"; then
        say "$label: unchanged, still running"
    else
        if ! systemctl --user is-active --quiet "$service"; then
            taken=$(ss -Hulpn "sport = :$port")
            if [ -n "$taken" ]; then
                say "$label: UDP port $port is taken by another process; stop it first:"
                printf '%s\n' "$taken"
                exit 1
            fi
        fi
        say "$label: restarting (${changed:-was not running})"
        since=$(date +%s)
        systemctl --user restart "$service"
        tries=0
        until [ -n "$(journal --since "@$since" --grep '^server url:')" ]; do
            tries=$((tries + 1))
            if [ "$tries" -ge 50 ] || ! systemctl --user is-active --quiet "$service"; then
                say "$label: did not come up"
                journal --since "@$since" | tail -n 20
                exit 1
            fi
            sleep 0.2
        done
    fi
    say "$label: $(journal --grep '^server url:' -n 1)"
}

deploy_stkn() {
    build
    say "stkn: uploading to $stkn_ssh"
    ssh "$stkn_ssh" mkdir -p "$stkn_stage"
    rsync -cz "$built" "$src/deploy/schwitzer-server.service" "$stkn_ssh:$stkn_stage/"
    if [ "$(ssh "$stkn_ssh" 'loginctl show-user "$USER" -p Linger --value')" != yes ]; then
        say "stkn: enabling lingering"
        ssh "$stkn_ssh" loginctl enable-linger
    fi
    # The activating half comes from the HEAD export as well.
    ssh "$stkn_ssh" sh -s activate stkn schwitzer-server "'[::]:7444'" stkn.org \
        "$stkn_stage/schwitzer-server" "$stkn_stage/schwitzer-server.service" \
        <"$src/deploy/deploy.sh"
}

case "${1:-}" in
stkn)
    deploy_stkn
    say "done"
    ;;
activate)
    shift
    activate "$@"
    ;;
*)
    say "usage: $0 stkn"
    exit 2
    ;;
esac

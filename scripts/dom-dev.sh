#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOCKET="${1:-/tmp/zzdom.sock}"
DOM_PORT="${2:-8092}"
ALA_PORT="${3:-8093}"
ALA_ASSETS="${ALA_ASSETS:-$ROOT/clients/web/dist-dev}"
DOM_HOME="${DOM_HOME:-${TMPDIR:-/tmp}/zz-dom-home}"
PIDS=()

cleanup() {
    for pid in ${PIDS[@]+"${PIDS[@]}"}; do
        kill "$pid" 2>/dev/null || true
    done
}
trap cleanup EXIT INT TERM

gateway_url() {
    for _ in $(seq 100); do
        if grep -q 'browser client: ' "$1" 2>/dev/null; then
            sed -n 's/.*browser client: //p' "$1"
            return
        fi
        sleep 0.1
    done
    echo "the gateway writing $1 never started" >&2
    exit 1
}

umask 077
cd "$ROOT"
unset ZZ_SOCKET ZZ_PANE ZZ_SESSION TMUX TMUX_PANE ZZ_DEV_BUILD
cargo build -q -p zz-cli -p zz-web
scripts/build-dom.sh
if [[ ! -f "$ALA_ASSETS/index.html" ]]; then
    WEB_SYNTAX_HIGHLIGHTING=0 /opt/homebrew/bin/bash scripts/build-web-wasm.sh
fi
mkdir -p "$DOM_HOME"
if ! HOME="$DOM_HOME" target/debug/zz_cli --socket "$SOCKET" has-session -t demo 2>/dev/null; then
    HOME="$DOM_HOME" target/debug/zz_cli --socket "$SOCKET" new-session -d -s demo
fi
rm -f "$DOM_HOME/dom.url" "$DOM_HOME/ala.url"
target/debug/zz-web --assets clients/dom/dist --bind "127.0.0.1:$DOM_PORT" --socket "$SOCKET" >"$DOM_HOME/dom.url" &
PIDS+=($!)
target/debug/zz-web --assets "$ALA_ASSETS" --bind "127.0.0.1:$ALA_PORT" --socket "$SOCKET" >"$DOM_HOME/ala.url" &
PIDS+=($!)
echo "HTML client  $(gateway_url "$DOM_HOME/dom.url")"
echo "ala client   $(gateway_url "$DOM_HOME/ala.url")"
echo "daemon       $SOCKET (HOME=$DOM_HOME)"
wait

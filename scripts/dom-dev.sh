#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOCKET="${1:-/tmp/zzdom.sock}"
DOM_PORT="${2:-8092}"
GPUI_PORT="${3:-8093}"
GPUI_ASSETS="${GPUI_ASSETS:-$ROOT/clients/web/dist-dev}"
DOM_HOME="${DOM_HOME:-${TMPDIR:-/tmp}/zz-dom-home}"
PIDS=()

cleanup() {
    for pid in ${PIDS[@]+"${PIDS[@]}"}; do
        kill "$pid" 2>/dev/null || true
    done
}
trap cleanup EXIT INT TERM

cd "$ROOT"
unset ZZ_SOCKET ZZ_PANE ZZ_SESSION TMUX TMUX_PANE ZZ_DEV_BUILD
cargo build -q -p zz-cli -p zz-web
scripts/build-dom.sh
if [[ ! -f "$GPUI_ASSETS/index.html" ]]; then
    WEB_SYNTAX_HIGHLIGHTING=0 /opt/homebrew/bin/bash scripts/build-web-wasm.sh
fi
mkdir -p "$DOM_HOME"
if ! HOME="$DOM_HOME" target/debug/zz_cli --socket "$SOCKET" has-session -t demo 2>/dev/null; then
    HOME="$DOM_HOME" target/debug/zz_cli --socket "$SOCKET" new-session -d -s demo
fi
target/debug/zz-web --assets clients/dom/dist --bind "127.0.0.1:$DOM_PORT" --socket "$SOCKET" &
PIDS+=($!)
target/debug/zz-web --assets "$GPUI_ASSETS" --bind "127.0.0.1:$GPUI_PORT" --socket "$SOCKET" &
PIDS+=($!)
echo "HTML client  http://127.0.0.1:$DOM_PORT/"
echo "gpui client  http://127.0.0.1:$GPUI_PORT/"
echo "daemon       $SOCKET (HOME=$DOM_HOME)"
wait

#!/usr/bin/env bash
set -euo pipefail

bundle="$(realpath "${CARGO_TARGET_DIR:-$PWD/target}/ladybird-app")"
state="$bundle/state"
unset ZZ_PANE ZZ_SESSION TMUX TMUX_PANE ZZ_TMUX_EXECUTABLE ZZ_APP_STARTUP_DIRECTORY ZZ_STARTUP_REENTRY
export XDG_CONFIG_HOME="$state/config" XDG_DATA_HOME="$state/data" XDG_CACHE_HOME="$state/cache"
export ZZ_LADYBIRD_HUB="$bundle/runtime/libexec/Hub"
mkdir -p "$XDG_CONFIG_HOME/zz-dev" "$XDG_DATA_HOME/zz-dev" "$XDG_CACHE_HOME"
touch "$XDG_CONFIG_HOME/zz-dev/config" "$XDG_DATA_HOME/zz-dev/import-prompted-v2"
socket="${XDG_RUNTIME_DIR:?}/zz-ladybird-$(stat -c %i "$bundle").sock"
export ZZ_SOCKET="$socket"
url="${1:-https://ladybird.org}"
"$bundle/zz" --socket "$socket" app &
app=$!
for ((retry=0; retry<150; ++retry)); do
    kill -0 "$app" 2>/dev/null || { wait "$app"; exit 1; }
    if [[ -S "$socket" ]]; then
        session="$("$bundle/cli" --socket "$socket" list-panes -F '#{session_id}' 2>/dev/null | head -1 || true)"
        if [[ -n "$session" ]]; then
            "$bundle/cli" --socket "$socket" new-window -t "$session" --kind browser "$url"
            wait "$app"
            exit
        fi
    fi
    sleep 0.2
done
echo "The spike desktop did not attach within 30 seconds." >&2
exit 1

#!/usr/bin/env bash
set -uo pipefail
ZB=${ZZ_BINARY:-/opt/homebrew/bin/zz}
mkdir -p /tmp/mn-runs
RUN=$(cd "$(mktemp -d /tmp/mn-runs/XXXXXX)" && pwd -P)
ID=${RUN##*/}
H=$RUN/h SOCK=/tmp/mns-$ID.sock
mkdir -p "$H/Library" "$H/.claude" "$H/.codex" "$RUN/ws" "$RUN/tmp"
ln -s "$HOME/Library/Keychains" "$H/Library/Keychains"
ln -s "$HOME/.codex/auth.json" "$H/.codex/auth.json"
Z() { env -i HOME="$H" USER="$USER" PATH="$PATH" SHELL=/bin/zsh TERM=xterm-256color LANG=en_US.UTF-8 TMPDIR="$RUN/tmp" ZZ_SOCKET="$SOCK" ZZ_TRAY=0 ZZ_UPDATE_CHECK=0 ${CALLER:+ZZ_PANE=$CALLER} "$ZB" "$@"; }
PX=$(Z new-session -d -P -F '#{pane_id}' -s work -n main -x 200 -y 50 -c "$RUN/ws")
HOSTP=$(Z split-window -d -P -F '#{pane_id}' -t "$PX" -c "$RUN/ws" --kind agent claude)
Z kill-pane -t "$PX"
CHILD=$(CALLER=$HOSTP Z new-pane -d -P -F '#{pane_id}' --kind agent codex 2>/dev/null)
TERM_PANE=$(CALLER=$CHILD Z split-window -d -P -F '#{pane_id}' -t "$CHILD")
echo "run $ID host=$HOSTP child=$CHILD terminal=$TERM_PANE"
echo "before:"; Z list-panes -a -F '  #{pane_id} #{pane_kind} parent=#{agent_parent}'
Z kill-pane -t "$HOSTP"
sleep 3
echo "after closing $HOSTP:"; Z list-panes -a -F '  #{pane_id} #{pane_kind} parent=#{agent_parent}' 2>&1 | sed 's/^error/  error/'
Z kill-server >/dev/null 2>&1
rm -f "$SOCK"
pkill -f "$RUN/" 2>/dev/null
for _ in 1 2 3 4 5; do pgrep -f "$RUN/" >/dev/null || break; sleep 1; done
pkill -9 -f "$RUN/" 2>/dev/null
rm -rf "$RUN"

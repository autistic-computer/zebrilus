#!/usr/bin/env bash
# Prototype for knowledge/designs/durable-driver-chat.md: keep a Driver Agent's checkpoint on every machine
# and restart the driver from it anywhere. Uses only zz verbs over zz's own ssh link.
#
#   driver-sync.sh start HOST          start a Claude driver on HOST with ~/.zz-driver/skill.md, owner epoch 1
#   driver-sync.sh mirror              on the owner: ship the checkpoint to every peer, every 2 s
#   driver-sync.sh push PEER           one mirror pass to PEER
#   driver-sync.sh handoff TO          make TO the owner and start a fresh driver there from the checkpoint
set -euo pipefail
ME=${DRIVER_ME:-$(hostname -s)}
L=.zz-driver
PEERS=${DRIVER_PEERS:-$(cat "$HOME/$L/peers" 2>/dev/null || zz list-hosts | cut -d: -f1)}
HERE=$(cd "$(dirname "$0")" && pwd)

Z() { local h=$1; shift; if [ "$h" = "$ME" ]; then zz "$@"; else zz --host "$h" "$@"; fi; }
on() { if [ "$1" = "$ME" ]; then (cd && bash -c "$2"); else zz --host "$1" run-shell "cd && $2"; fi; }
owner() { on "$1" "cat $L/owner 2>/dev/null || echo 0 - - -"; }
epoch() { owner "$1" | cut -d' ' -f1; }

send() {
  local peer=$1 src=$2 dest=$3
  on "$peer" "mkdir -p '$(dirname "$dest")' && : > '$dest'"
  split -b 900k --filter "zz --host $peer load-buffer -b driver-sync - && zz --host $peer run-shell \"zz save-buffer -a -b driver-sync '$dest' && zz delete-buffer -b driver-sync\"" "$src"
}

transcript() { ls "$HOME"/.claude/projects/*/"$1".jsonl 2>/dev/null | head -1; }

push() {
  local peer=$1 e host agent id phome tx
  read -r e host agent id < "$HOME/$L/owner"
  [ "$(epoch "$peer")" -le "$e" ] || { echo "fenced: $peer is ahead of epoch $e" >&2; zz kill-session -t driver 2>/dev/null; exit 3; }
  tx=$(transcript "$id")
  [ -n "$tx" ] && python3 "$HERE/driver-checkpoint.py" chat "$tx" > "$HOME/$L/chat.jsonl.new" && mv "$HOME/$L/chat.jsonl.new" "$HOME/$L/chat.jsonl"
  phome=$(on "$peer" 'echo $HOME')
  for f in $(cd "$HOME" && find "$L/work" "$L/chat.jsonl" "$L/skill.md" "$L/driver-sync.sh" "$L/driver-checkpoint.py" -type f 2>/dev/null); do
    [ "$(on "$peer" "cksum < '$f' 2>/dev/null || true")" = "$(cksum < "$HOME/$f")" ] || send "$peer" "$HOME/$f" "$phome/$f"
  done
  on "$peer" "mkdir -p $L && echo '$e $host $agent $id' > $L/owner"
}

launch() {
  local host=$1 id=$2 first=$3 dir
  dir=$(on "$host" 'echo $HOME')/$L/work
  Z "$host" new-session -d -s driver -c "$dir" "claude --session-id $id --append-system-prompt \"\$(cat ../skill.md)\" \"\$(cat $first)\""
  Z "$host" new-window -d -t driver -n sync "$dir/../driver-sync.sh mirror"
}

case ${1:-} in
  start)
    host=$2 id=$(uuidgen | tr 'A-Z' 'a-z')
    on "$host" "mkdir -p $L/work && echo '1 $host claude $id' > $L/owner && printf '%s\n' 'You are fabrico'\''s driver. Wait for his first request.' > $L/work/.hello"
    launch "$host" "$id" .hello
    echo "driver $id on $host" ;;
  push) push "$2" ;;
  mirror)
    while sleep 2; do
      read -r _ host _ _ < "$HOME/$L/owner"; [ "$host" = "$ME" ] || exit 0
      for p in $PEERS; do [ "$p" = "$ME" ] || push "$p" || true; done
    done ;;
  handoff)
    to=$2 best=0 src=""
    for h in $PEERS $ME; do e=$(epoch "$h" 2>/dev/null || echo 0); [ "$e" -le "$best" ] || { best=$e; src=$h; }; done
    read -r e old agent _ <<< "$(owner "$src")"
    if Z "$old" kill-session -t driver 2>/dev/null; then on "$old" "$L/driver-sync.sh push $to"; else echo "$old unreachable or stopped: starting from $to's copy"; fi
    id=$(uuidgen | tr 'A-Z' 'a-z')
    for h in $to $PEERS; do on "$h" "echo '$((e + 1)) $to $agent $id' > $L/owner" 2>/dev/null || true; done
    on "$to" "cd $L && python3 driver-checkpoint.py packet chat.jsonl work/ledger.md > work/.packet"
    launch "$to" "$id" .packet
    echo "driver $id now on $to at epoch $((e + 1)), started from the checkpoint" ;;
  *) sed -n '2,9p' "$0"; exit 2 ;;
esac

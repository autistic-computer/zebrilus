#!/usr/bin/env bash
# Prototype for knowledge/designs/durable-lead-chat.md: keep a lead agent's chat on every machine
# and move the lead between them. Uses only zz verbs over zz's own ssh link.
#
#   lead-sync.sh start HOST AGENT        start a lead (claude|codex) on HOST, owner epoch 1
#   lead-sync.sh mirror                  on the owner: ship new bytes to every peer, every 2 s
#   lead-sync.sh push PEER               one mirror pass to PEER
#   lead-sync.sh handoff TO              make TO the owner and resume the same chat there
set -euo pipefail
ME=${LEAD_ME:-$(hostname -s)}
L=.zz-lead
PEERS=${LEAD_PEERS:-$(cat "$HOME/.zz-lead/peers" 2>/dev/null || zz list-hosts | cut -d: -f1)}

Z() { local h=$1; shift; if [ "$h" = "$ME" ]; then zz "$@"; else zz --host "$h" "$@"; fi; }
on() { if [ "$1" = "$ME" ] || [ "$1" = local ]; then (cd && bash -c "$2"); else zz --host "$1" run-shell "cd && $2"; fi; }
owner() { on "$1" "cat $L/owner 2>/dev/null || echo 0 - - -"; }
epoch() { owner "$1" | cut -d' ' -f1; }

chat_rel() {
  local agent=$1 id=$2 home=$3
  if [ "$agent" = claude ]; then
    printf '.claude/projects/%s/%s' "$(printf '%s' "$home/$L/work" | tr '/.' '--')" "$id"
  else
    (cd && ls .codex/sessions/*/*/*/rollout-*-"$id".jsonl 2>/dev/null | head -1 | sed 's/\.jsonl$//')
  fi
}

send() {
  local peer=$1 src=$2 dest=$3 from=$4
  [ "$from" -gt 0 ] || on "$peer" "mkdir -p '$(dirname "$dest")' && : > '$dest'"
  tail -c +$((from + 1)) "$src" | split -b 900k --filter "zz --host $peer load-buffer -b lead-sync - && zz --host $peer run-shell \"zz save-buffer -a -b lead-sync '$dest' && zz delete-buffer -b lead-sync\"" -
}

push() {
  local peer=$1 e host agent id mine theirs rel prel size have
  read -r e host agent id < "$HOME/$L/owner"
  theirs=$(epoch "$peer")
  [ "$theirs" -le "$e" ] || { echo "fenced: $peer is at epoch $theirs, this machine at $e" >&2; zz kill-session -t lead 2>/dev/null; exit 3; }
  phome=$(on "$peer" 'echo $HOME')
  rel=$(chat_rel "$agent" "$id" "$HOME")
  prel=$rel
  [ "$agent" = claude ] && prel=$(chat_rel "$agent" "$id" "$phome")
  [ -f "$HOME/$rel.jsonl" ] || return 0
  size=$(wc -c < "$HOME/$rel.jsonl")
  have=$(on "$peer" "wc -c < '$prel.jsonl' 2>/dev/null || echo 0")
  if [ "$have" -gt 0 ] && [ "$(on "$peer" "tail -c 4096 '$prel.jsonl' | head -c $((have < 4096 ? have : 4096)) | cksum")" != \
       "$(head -c "$have" "$HOME/$rel.jsonl" | tail -c 4096 | cksum)" ]; then have=0; fi
  [ "$have" -ge "$size" ] || send "$peer" "$HOME/$rel.jsonl" "$phome/$prel.jsonl" "$have"
  for f in $(cd "$HOME" && find "$L/work" "$rel" -type f 2>/dev/null); do
    dest=$f; [[ $f == "$rel"/* ]] && dest=$prel/${f#"$rel"/}
    [ "$(on "$peer" "cksum < '$dest' 2>/dev/null || true")" = "$(cksum < "$HOME/$f")" ] || send "$peer" "$HOME/$f" "$phome/$dest" 0
  done
  on "$peer" "mkdir -p $L && echo '$e $host $agent $id' > $L/owner"
}

case ${1:-} in
  start)
    host=$2 agent=$3 id=$(uuidgen | tr 'A-Z' 'a-z')
    on "$host" "mkdir -p $L/work && echo '1 $host $agent $id' > $L/owner"
    cmd="claude --session-id $id"; [ "$agent" = codex ] && cmd=codex
    Z "$host" new-session -d -s lead -c "$(on "$host" 'echo $HOME')/$L/work" "$cmd"
    Z "$host" new-window -d -t lead -n sync "~/$L/lead-sync.sh mirror"
    echo "lead $agent $id on $host" ;;
  push) push "$2" ;;
  mirror)
    while sleep 2; do
      read -r _ host _ _ < "$HOME/$L/owner"; [ "$host" = "$ME" ] || exit 0
      for p in $PEERS; do [ "$p" = "$ME" ] || push "$p" || true; done
    done ;;
  handoff)
    to=$2 best=0 src=""
    for h in $PEERS $ME; do e=$(epoch "$h" 2>/dev/null || echo 0); [ "$e" -le "$best" ] || { best=$e; src=$h; }; done
    read -r e old agent id <<< "$(owner "$src")"
    if Z "$old" kill-session -t lead 2>/dev/null; then Z "$old" run-shell "~/$L/lead-sync.sh push $to"; else echo "$old unreachable or stopped: resuming from $to's copy"; fi
    on "$to" "echo '$((e + 1)) $to $agent $id' > $L/owner"
    for h in $PEERS; do [ "$h" = "$to" ] || on "$h" "echo '$((e + 1)) $to $agent $id' > $L/owner" 2>/dev/null || true; done
    cmd="claude --resume $id"; [ "$agent" = codex ] && cmd="codex resume $id"
    Z "$to" new-session -d -s lead -c "$(on "$to" 'echo $HOME')/$L/work" "$cmd"
    Z "$to" new-window -d -t lead -n sync "~/$L/lead-sync.sh mirror"
    echo "lead $agent $id now on $to at epoch $((e + 1))" ;;
  *) sed -n '2,12p' "$0"; exit 2 ;;
esac

#!/usr/bin/env bash
# trust.sh HOST DIR: accept Claude Code's and Codex's folder-trust dialogs for DIR, as the user would
h=$1 dir=$2 s=trust-$RANDOM
Z() { ssh -o BatchMode=yes "$h" "bash -lc '$*'" </dev/null; }
Z "zz new-session -d -s $s -c $dir claude" >/dev/null
sleep 7
if Z "zz capture-pane -p -t $s" | grep -q "trust this folder"; then Z "zz send-keys -t $s Down"; sleep 0.5; Z "zz send-keys -t $s Enter"; sleep 4; fi
Z "zz capture-pane -p -t $s" | grep -q "Chrome extension detected" && { Z "zz send-keys -t $s Escape"; sleep 1; }
Z "zz kill-session -t $s"
codex="codex"; [ "$h" = alienware ] && codex="codex -m gpt-6-luna"
Z "zz new-session -d -s $s -c $dir $codex" >/dev/null
sleep 7
Z "zz capture-pane -p -t $s" | grep -q "Trust this folder" && { Z "zz send-keys -t $s Enter"; sleep 3; }
Z "zz capture-pane -p -t $s" | grep -q "Update available" && { Z "zz send-keys -t $s 3 Enter"; sleep 2; }
Z "zz kill-session -t $s"

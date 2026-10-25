#!/usr/bin/env bash
# cleanup.sh SNAPSHOT [--dry-run]: close what a batch left behind on each host. A pane counts when its
# id is newer than the snapshot and it runs claude or codex or sits under ~/orch-lab; a session
# counts when its id is newer and every pane in it counts.
DRY=${2:-}
while read -r h pane session; do
  ssh -n "$h" "bash -lc 'zz list-panes -a -F \"#{pane_id} #{session_id} #{pane_current_path} #{pane_start_command} #{pane_current_command}\"'" |
  while read -r id sid path rest; do
    n=${id#%}; s=${sid#\$}
    [ "$n" -gt "$pane" ] || continue
    if [[ $path == */orch-lab/* || $rest == *claude* || $rest == *codex* || $rest == *node* || $s -gt $session ]]; then
      echo "$h kill $id $path $rest"
      [ "$DRY" = --dry-run ] || ssh "$h" "bash -lc 'zz kill-pane -t $id'" </dev/null
    fi
  done
done < "$1"

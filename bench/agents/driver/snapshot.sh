#!/usr/bin/env bash
# snapshot.sh FILE: record the highest pane and session id on each host before a batch
for h in xps alienware ubuntu; do
  printf '%s %s %s\n' "$h" \
    "$(ssh -n "$h" "bash -lc 'zz list-panes -a -F \"#{pane_id}\"'" | tr -d % | sort -n | tail -1)" \
    "$(ssh -n "$h" "bash -lc 'zz list-sessions -F \"#{session_id}\"'" | tr -d '$' | sort -n | tail -1)"
done > "$1"

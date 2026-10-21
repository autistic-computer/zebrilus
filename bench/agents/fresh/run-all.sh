#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
TASKS=${TASKS:-"n0 n1 n2 n3 n4 n5 n6 n7${REMOTE_HOST:+ n8}"}
REPS=${REPS:-"1 2"}
CLAUDE_ARMS=${CLAUDE_ARMS:-"bare hooks pane"}
CODEX_ARMS=${CODEX_ARMS:-"bare hooks trusted pane"}
PAR=${PAR:-5}
for r in $REPS; do for t in $TASKS; do
  for a in $CLAUDE_ARMS; do echo "claude-haiku-5-5 $a $t $r"; done
  for a in $CODEX_ARMS; do echo "gpt-6-luna $a $t $r"; done
done; done | xargs -P "$PAR" -L1 sh -c '"'"$S"'/run-one.sh" "$0" "$1" "$2" "$3"'
python3 "$S/summary.py" "$S/out/results.tsv"

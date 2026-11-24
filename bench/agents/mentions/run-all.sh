#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
HOSTS=${HOSTS:-"claude codex"}
SCENARIOS=${SCENARIOS:-"m1 m3 m4 m4a m4b m4c m5 m6 m7 m8 m8b"}
REPS=${REPS:-"1 2 3"}
PAR=${PAR:-2}
[ "$PAR" -gt 2 ] && PAR=2
for r in $REPS; do for s in $SCENARIOS; do for h in $HOSTS; do echo "$h $s $r"; done; done; done |
  xargs -P "$PAR" -L1 sh -c '"'"$S"'/run-one.sh" "$0" "$1" "$2"'
python3 "$S/summary.py" "${RESULTS:-$S/out/results.tsv}"

#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
MODELS=${MODELS:-"gpt-6-luna claude-haiku-5-5"}
TASKS=${TASKS:-"t1 t2 t3 t4 t5 t6 t7 t9 t10 t11 t12 t8"}
PAR=${PAR:-3}
ARMS=${ARMS:-"none skill inline hook stub stubhook"}
for m in $MODELS; do for a in $ARMS; do for t in $TASKS; do
  echo "zz $m $a $t"
done; done; done | xargs -P "$PAR" -L1 sh -c 'BACKEND=$0 "'"$S"'/run-one.sh" "$1" "$2" "$3"'
"$S/summary.sh"

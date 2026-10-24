#!/usr/bin/env bash
# run-all.sh: ARMS="none v2" TASKS="g f r a" REPS=1 PAR=8, or JOBS="arm task rep" lines; plus run-one.sh's variables.
# Runs from a frozen copy of this directory, so editing the harness mid-batch is safe.
S=$(cd "$(dirname "$0")" && pwd)
OUT=${OUT:-$S/out}; mkdir -p "$OUT"; export OUT
STAMP=$(date +%m%d%H%M%S)
H=$OUT/harness-$STAMP
mkdir -p "$H" && cp -R "$S"/*.sh "$S"/*.py "$S/skills" "$H/"
"$H/snapshot.sh" "$OUT/snapshot-$STAMP"
{ if [ -n "${JOBS:-}" ]; then printf '%s\n' "$JOBS"; else
  for arm in ${ARMS:-none v2}; do for task in ${TASKS:-g f r a}; do for rep in $(seq 1 "${REPS:-1}"); do
    echo "$arm $task $rep"
  done; done; done; fi; } | xargs -P "${PAR:-8}" -L 1 bash -c '"$0"/run-one.sh "$1" "$2" "$3"' "$H"
"$H/cleanup.sh" "$OUT/snapshot-$STAMP"

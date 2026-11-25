#!/usr/bin/env bash
S=$(cd "$(dirname "$0")" && pwd)
RUN=$(cd "${1:?run folder}" && pwd) || exit 2
REPO=$(cd "$S/../../.." && pwd)
BRIEF="$(cat "$S/review.md")

Run folder: $RUN
Repo: $REPO"
if [ -n "${ZZ_PANE-}" ] && [ "${2-}" != - ]; then
  zz new-pane -d -P -F '#{pane_id}' -c "$REPO" --kind agent "${2:-claude}" "$BRIEF"
else
  printf '%s\n' "$BRIEF"
fi

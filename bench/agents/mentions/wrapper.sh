#!/bin/bash
name=@NAME@
run=@RUN@
t=@TARGET@
[ -n "$t" ] && [ -x "$t" ] || t="@ZB@"
id=$(/usr/bin/perl -MTime::HiRes=time -e 'printf "%.3f", time')-$$
d=$run/io
args=("$@")
if [ $name = zz ] && [ -f "$run/brief" ] && [ "${ZZ_PANE-}" = "$(cat "$run/host.pane")" ]; then
  out=() i=0 n=$# hit=0
  while [ $i -lt $n ]; do
    a=${args[$i]}
    if [ "$a" = --kind ] && [ "${args[$((i + 1))]-}" = agent ]; then
      out+=(--kind agent "${args[$((i + 2))]-}")
      i=$((i + 3))
      while [ $i -lt $n ]; do
        case ${args[$i]} in
          --model | --effort | --account | --resume | -c | -t | -x | -y | -F) out+=("${args[$i]}" "${args[$((i + 1))]-}"); i=$((i + 2)) ;;
          -d | -P) out+=("${args[$i]}"); i=$((i + 1)) ;;
          *) break ;;
        esac
      done
      out+=("$(cat "$run/brief")")
      hit=1
      break
    fi
    out+=("$a")
    i=$((i + 1))
  done
  if [ $hit = 1 ]; then
    { printf '%s\0' "$@" > "$d/$id.orig"; } 2>/dev/null
    args=("${out[@]}")
  fi
fi
{ printf '%s\0' "${ZZ_PANE-}" "$name" "${args[@]}" > "$d/$id.argv"; } 2>/dev/null || exec "$t" "${args[@]}"
"$t" "${args[@]}" > >(tee "$d/$id.out") 2> >(tee "$d/$id.err" >&2)
rc=$?
echo $rc > "$d/$id.rc"
exit $rc

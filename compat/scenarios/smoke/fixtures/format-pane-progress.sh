#!/bin/sh
set -eu

export LC_ALL=C

if [ -n "${ZZ_SMOKE_ZZ_BIN:-}" ]; then
    side=zz
    binary="$ZZ_SMOKE_ZZ_BIN"
    set -- --socket "$ZZ_SMOKE_ZZ_SOCKET"
else
    side=tmux
    binary="$ZZ_SMOKE_TMUX_BIN"
    set -- -L "$ZZ_SMOKE_TMUX_LABEL"
fi
prefix_args="$*"
main_client() {
    # shellcheck disable=SC2086
    "$binary" $prefix_args "$@"
}

session=paneprog
work="$HOME/format-pane-progress-work-$side"
rm -rf "$work"
mkdir -p "$work"
: >"$work/failures"
failed=0
check_count=0

record_failure() {
    failed=1
    echo "$1" >>"$work/failures"
}

check_equal() {
    check_count=$((check_count + 1))
    if [ "$2" != "$3" ]; then
        record_failure "$1 want=[$2] got=[$3]"
    fi
}

cleanup() {
    cleanup_status=$?
    trap - EXIT
    set +e
    main_client kill-session -t "=$session" >/dev/null 2>&1
    exit "$cleanup_status"
}
trap cleanup EXIT

bar() {
    main_client display-message -p -t "$pane" '#{pane_pb_state}/#{pane_pb_progress}'
}

marker=0
emit() {
    marker=$((marker + 1))
    printf '\033]%s\007\r\nMARK%02d\r\n' "$1" "$marker" >"$pane_tty"
    attempt=0
    while [ "$attempt" -lt 400 ]; do
        if main_client capture-pane -p -S - -t "$pane" | grep -q "^MARK$(printf '%02d' "$marker")\$"; then
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 0.05
    done
    record_failure "emit-$1"
    return 1
}

emit_raw() {
    marker=$((marker + 1))
    format="$1"
    shift
    # shellcheck disable=SC2059
    printf "\033]$format\r\nMARK%02d\r\n" "$@" "$marker" >"$pane_tty"
    attempt=0
    while [ "$attempt" -lt 400 ]; do
        if main_client capture-pane -p -S - -t "$pane" | grep -q "^MARK$(printf '%02d' "$marker")\$"; then
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 0.05
    done
    record_failure "emit-raw-$format"
    return 1
}

main_client kill-session -t "=$session" >/dev/null 2>&1 || true
main_client new-session -d -s "$session" -x 80 -y 24
pane="$(main_client list-panes -t "=$session" -F '#{pane_id}' | sed -n '1p')"
pane_tty="$(main_client display-message -p -t "$pane" '#{pane_tty}')"

check_equal fresh hidden/0 "$(bar)"

emit '9;4;0' || { echo "format-pane-progress-$side: hidden"; exit 0; }
check_equal state-hidden hidden/0 "$(bar)"
emit '9;4;1;50' || { echo "format-pane-progress-$side: normal"; exit 0; }
check_equal state-normal normal/50 "$(bar)"
emit '9;4;2;30' || { echo "format-pane-progress-$side: error"; exit 0; }
check_equal state-error error/30 "$(bar)"

emit '9;4;3' || { echo "format-pane-progress-$side: indeterminate"; exit 0; }
check_equal state-indeterminate indeterminate/30 "$(bar)"
emit '9;4;4;10' || { echo "format-pane-progress-$side: paused"; exit 0; }
check_equal state-paused paused/10 "$(bar)"

emit '9;4;3;70' || { echo "format-pane-progress-$side: indeterminate-progress"; exit 0; }
check_equal indeterminate-drops-its-progress indeterminate/10 "$(bar)"

for ignored in '9;4;5' '9;4;1;101' '9;4' '9;4;' '9;4;1;5x' '9;5;1'; do
    emit "$ignored" || { echo "format-pane-progress-$side: ignored"; exit 0; }
    check_equal "ignores-$ignored" indeterminate/10 "$(bar)"
done

emit '9;4;2;' || { echo "format-pane-progress-$side: bare-state"; exit 0; }
check_equal trailing-semicolon-keeps-progress error/10 "$(bar)"

emit '9;4;1;100' || { echo "format-pane-progress-$side: full"; exit 0; }
check_equal full-progress normal/100 "$(bar)"
emit '9;4;0;0' || { echo "format-pane-progress-$side: reset"; exit 0; }
check_equal back-to-hidden hidden/0 "$(bar)"

emit_raw '9;4;1;50\030' || { echo "format-pane-progress-$side: can"; exit 0; }
check_equal can-commits normal/50 "$(bar)"
emit_raw '9;4;2;20\032' || { echo "format-pane-progress-$side: sub"; exit 0; }
check_equal sub-commits error/20 "$(bar)"
emit_raw '9;4;1;%b77\007' '\0' || { echo "format-pane-progress-$side: nul"; exit 0; }
check_equal nul-inside-is-dropped normal/77 "$(bar)"

if [ "$check_count" -ne 19 ]; then
    record_failure "total-checks $check_count"
fi
if [ "$failed" -eq 0 ]; then
    main_client set-environment -g FORMAT_PANE_PROGRESS "clean:$check_count"
else
    sed "s/^/format-pane-progress-$side: /" "$work/failures"
fi

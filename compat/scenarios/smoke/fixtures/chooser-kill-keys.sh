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

session=ckill
work="$HOME/chooser-kill-keys-work-$side"
steps="$work/steps"
snaps="$work/snaps"
rm -rf "$work"
mkdir -p "$steps" "$snaps"
: >"$work/failures"
failed=0
check_count=0
step=0
attach_pid=""
client=""

record_failure() {
    failed=1
    echo "$1 want=[$2] got=[$3]" >>"$work/failures"
}

check_equal() {
    check_count=$((check_count + 1))
    if [ "$2" != "$3" ]; then
        record_failure "$1" "$2" "$3"
    fi
}

cleanup() {
    cleanup_status=$?
    trap - EXIT
    set +e
    step=$((step + 1))
    echo quit >"$steps/step-$step" 2>/dev/null
    main_client kill-session -t "=$session" >/dev/null 2>&1
    if [ -n "$attach_pid" ]; then
        kill "$attach_pid" >/dev/null 2>&1
        wait "$attach_pid" >/dev/null 2>&1
    fi
    main_client set-environment -gu CHOOSER_ROW >/dev/null 2>&1
    main_client unbind-key -n C-o >/dev/null 2>&1
    exit "$cleanup_status"
}
trap cleanup EXIT

drive() {
    step=$((step + 1))
    printf '%s\n' "$1" >"$steps/step-$step"
    attempt=0
    while [ "$attempt" -lt 400 ]; do
        if [ -f "$steps/ack-$step" ]; then
            sleep 0.25
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 0.05
    done
    record_failure "drive-$step" ack ""
    return 0
}

chosen_row() {
    row="$(main_client show-environment -g CHOOSER_ROW 2>/dev/null || true)"
    printf '%s' "${row#CHOOSER_ROW=}"
}

windows() {
    main_client list-windows -t "=$session" -F '#{window_name}' 2>/dev/null | tr '\n' ' '
}

open_chooser() {
    main_client set-environment -g CHOOSER_ROW pending
    drive "keys 0f"
    sleep 0.5
}

press() {
    drive "keys $1"
}

settle() {
    attempt=0
    while [ "$attempt" -lt 60 ]; do
        if [ "$(chosen_row)" != pending ]; then
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 0.05
    done
    return 0
}

main_client kill-session -t "=$session" >/dev/null 2>&1 || true
main_client new-session -d -s "$session" -n w0
for name in w1 w2 w3 w4; do
    main_client new-window -t "=$session" -n "$name" -d
done
main_client select-window -t "=$session:0"
mine="#{==:#{session_name},$session}"

env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE \
    TERM=xterm-256color \
    python3 "$HOME/chooser-drive.py" "$steps" "$snaps" 80 24 \
    "$binary" $prefix_args attach-session -t "=$session" \
    >"$work/attach.out" 2>&1 &
attach_pid=$!

attempt=0
while [ "$attempt" -lt 400 ]; do
    client="$(main_client list-clients -t "=$session" -F '#{client_name}' | sed -n '1p')"
    if [ -n "$client" ]; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.05
done
if [ -z "$client" ]; then
    echo "chooser-kill-keys-$side: attach-client"
    exit 0
fi
sleep 0.5

check_equal setup-windows 'w0 w1 w2 w3 w4 ' "$(windows)"

main_client bind-key -n C-o choose-tree -w -f "$mine" \
    'set-environment -g CHOOSER_ROW %%'

open_chooser
press 67
press 6a
press 6a
press 78
press 0d
settle
check_equal x-prompt-eats-the-next-key pending "$(chosen_row)"
check_equal x-prompt-declined-kills-nothing 'w0 w1 w2 w3 w4 ' "$(windows)"
press 71
sleep 0.3

open_chooser
press 67
press 6a
press 6a
press 78
press 6e
press 0d
settle
check_equal x-declined-leaves-the-chooser-open "=$session:1." "$(chosen_row)"
check_equal x-declined-still-kills-nothing 'w0 w1 w2 w3 w4 ' "$(windows)"

open_chooser
press 67
press 6a
press 6a
press 78
press 79
sleep 0.6
check_equal x-confirmed-kills-the-row 'w0 w2 w3 w4 ' "$(windows)"
press 0d
settle
check_equal x-confirmed-leaves-the-chooser-open "=$session:2." "$(chosen_row)"

open_chooser
press 67
press 6a
press 6a
press 74
press 58
press 0d
settle
check_equal tagged-X-prompts pending "$(chosen_row)"
check_equal tagged-X-declined-kills-nothing 'w0 w2 w3 w4 ' "$(windows)"
press 71
sleep 0.3

open_chooser
press 67
press 6a
press 6a
press 58
press 0d
settle
check_equal untagged-X-is-inert "=$session:2." "$(chosen_row)"

open_chooser
press 67
press 6a
press 6a
press 74
press 54
press 58
press 0d
settle
check_equal T-untags-every-row "=$session:3." "$(chosen_row)"

open_chooser
press 67
press 14
press 58
press 0d
settle
check_equal ctrl-t-tags-the-top-level pending "$(chosen_row)"
check_equal ctrl-t-kills-nothing-on-a-declined-prompt 'w0 w2 w3 w4 ' "$(windows)"
press 71
sleep 0.3

main_client bind-key -n C-o choose-tree -y -w -f "$mine" \
    'set-environment -g CHOOSER_ROW %%'
open_chooser
press 67
press 6a
press 6a
press 78
sleep 0.6
check_equal y-flag-answers-the-current-kill 'w0 w3 w4 ' "$(windows)"
press 71
sleep 0.3

open_chooser
press 67
press 6a
press 6a
press 74
press 58
sleep 0.6
check_equal y-flag-answers-the-tagged-kill 'w0 w4 ' "$(windows)"
press 71
sleep 0.3

if [ "$check_count" -ne 15 ]; then
    record_failure "total-checks" 15 "$check_count"
fi
if [ "$failed" -eq 0 ]; then
    main_client set-environment -g CHOOSER_KILL_KEYS "clean:$check_count"
else
    sed "s/^/chooser-kill-keys-$side: /" "$work/failures"
fi

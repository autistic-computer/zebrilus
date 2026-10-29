#!/bin/sh
set -eu

export LC_ALL=C

if [ -n "${ZZ_SMOKE_ZZ_BIN:-}" ]; then
    side=zz
    main_client() {
        "$ZZ_SMOKE_ZZ_BIN" --socket "$ZZ_SMOKE_ZZ_SOCKET" "$@"
    }
else
    side=tmux
    main_client() {
        "$ZZ_SMOKE_TMUX_BIN" -L "$ZZ_SMOKE_TMUX_LABEL" "$@"
    }
fi

work="$HOME/if-shell-background-order-work-$side"
transcript="$HOME/if-shell-background-order-$side.txt"
branches='1 2 3 4 5 6 7 8 9 a b c'
ordered=123456789abc
attempts=8
rm -rf "$work"
mkdir -p "$work"
: >"$transcript"

wait_for_order() {
    attempt=0
    while [ "$attempt" -lt 300 ]; do
        value="$(main_client display-message -p '#{@order}')"
        if [ "${#value}" -ge 12 ]; then
            printf '%s' "$value"
            return
        fi
        attempt=$((attempt + 1))
        sleep 0.05
    done
    printf 'timeout:%s' "$value"
}

write_config() {
    config=$1
    first_condition=$2
    {
        echo "set -g @order ''"
        echo "if -b $first_condition 'set -ag @order 1'"
        for branch in $branches; do
            [ "$branch" = 1 ] && continue
            echo "if -b true 'set -ag @order $branch'"
        done
    } >"$config"
}

apply_once() {
    config=$1
    main_client set -g @order '' >/dev/null
    main_client source-file "$config"
    wait_for_order
}

ordered_round() {
    label=$1
    config="$work/$label.conf"
    write_config "$config" true
    attempt=0
    while [ "$attempt" -lt "$attempts" ]; do
        if [ "$(apply_once "$config")" = "$ordered" ]; then
            printf '%s=file-order\n' "$label" >>"$transcript"
            return
        fi
        attempt=$((attempt + 1))
    done
    printf '%s=never-file-order-in-%s\n' "$label" "$attempts" >>"$transcript"
}

slow_first_round() {
    config="$work/slow-first.conf"
    write_config "$config" "'sleep 0.3'"
    value="$(apply_once "$config")"
    case "$value" in
    ???????????1) printf 'slow-first=slow-branch-last\n' >>"$transcript" ;;
    *) printf 'slow-first=slow-branch-not-last:%s\n' "$value" >>"$transcript" ;;
    esac
}

ordered_round equal-cost-a
ordered_round equal-cost-b
ordered_round equal-cost-c
slow_first_round

main_client set -gu @order >/dev/null 2>&1 || true
rm -rf "$work"

main_client load-buffer -b order-transcript "$transcript"

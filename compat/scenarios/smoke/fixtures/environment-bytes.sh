#!/bin/sh
set -eu

export LC_ALL=C.UTF-8

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

root="$HOME/environment-bytes-$side"
rm -rf "$root"
mkdir -p "$root"

value=$(printf 'a\377b')
name=$(printf 'N\377')
session_value=$(printf 'x\377y')

hex() {
    od -An -tx1 -v "$1" | tr -d ' \n'
}

main_client set-environment -g ZZBYTES "$value"
main_client show-environment -g ZZBYTES >"$root/global" 2>&1 || :
main_client show-environment -gs ZZBYTES >"$root/global-s" 2>&1 || :
main_client set-environment -g "$name" plain
main_client show-environment -g "$name" >"$root/name" 2>&1 || :
main_client set-environment ZZSESS "$session_value"
main_client show-environment ZZSESS >"$root/session" 2>&1 || :

fmt() {
    main_client display-message -p "$1" >"$root/fmt-$2" 2>&1 || :
}
fmt '#{ZZBYTES}' plain
fmt '#{b:#{ZZBYTES}}' basename
fmt '#{=1:#{ZZBYTES}}' trim-left
fmt '#{=2:#{ZZBYTES}}' trim-left-wide
fmt '#{=-2:#{ZZBYTES}}' trim-right
fmt '#{?ZZBYTES,yes,no}' conditional
fmt '#{n:ZZBYTES}|#{w:#{ZZBYTES}}' measures
fmt '#{p5:#{ZZBYTES}}' padded
fmt '#{q:#{ZZBYTES}}' quoted
main_client list-panes -a -F '#{ZZBYTES}' >"$root/fmt-list" 2>&1 || :

cat >"$root/inherit.sh" <<INHERIT
#!/bin/sh
LC_ALL=C.UTF-8 "$binary" $prefix_args set-environment -g ZZINHERIT "\${ZZBYTES-unset}"
sleep 30
INHERIT
main_client new-window -d -n inherit "sh $root/inherit.sh"
attempt=0
while [ "$attempt" -lt 80 ]; do
    main_client show-environment -g ZZINHERIT >"$root/inherit" 2>/dev/null || :
    if [ -s "$root/inherit" ]; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 0.25
done

# shellcheck disable=SC2086
"$binary" $prefix_args -C show-environment -g ZZBYTES </dev/null 2>/dev/null \
    | LC_ALL=C sed -n '/^ZZBYTES=/p' >"$root/control" || :

main_client set-option -g update-environment "ZZBYTES ZZPLAIN"
ZZBYTES="$value" ZZPLAIN=neighbour \
    ZZ_ENVBYTES_BIN="$binary" ZZ_ENVBYTES_ARGS="$prefix_args" ZZ_ENVBYTES_ROOT="$root" \
    python3 "$HOME/environment-bytes-attach.py" || :

result=broken
if [ "$(hex "$root/global")" = 5a5a42595445533d61ff620a ] &&
    [ "$(hex "$root/global-s")" = 5a5a42595445533d2261ff62223b206578706f7274205a5a42595445533b0a ] &&
    [ "$(hex "$root/name")" = 4eff3d706c61696e0a ] &&
    [ "$(hex "$root/session")" = 5a5a534553533d78ff790a ] &&
    [ "$(hex "$root/inherit")" = 5a5a494e48455249543d61ff620a ] &&
    [ "$(hex "$root/attach-bytes")" = 5a5a42595445533d61ff620a ] &&
    [ "$(hex "$root/attach-plain")" = 5a5a504c41494e3d6e65696768626f75720a ] &&
    [ "$(hex "$root/control")" = 5a5a42595445533d61ff620a ] &&
    [ "$(hex "$root/fmt-plain")" = 61ff620a ] &&
    [ "$(hex "$root/fmt-basename")" = 61ff620a ] &&
    [ "$(hex "$root/fmt-trim-left")" = 610a ] &&
    [ "$(hex "$root/fmt-trim-left-wide")" = 61620a ] &&
    [ "$(hex "$root/fmt-trim-right")" = 61ff620a ] &&
    [ "$(hex "$root/fmt-conditional")" = 7965730a ] &&
    [ "$(hex "$root/fmt-measures")" = 337c320a ] &&
    [ "$(hex "$root/fmt-padded")" = 61ff622020200a ] &&
    [ "$(hex "$root/fmt-quoted")" = 61ff620a ] &&
    [ "$(hex "$root/fmt-list")" = 61ff620a ]; then
    result=clean:18
fi

main_client set-environment -g ZZ_ENVBYTES_RESULT "$result"

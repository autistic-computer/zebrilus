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

utf8_client() {
    # shellcheck disable=SC2086
    LC_ALL=C.UTF-8 "$binary" $prefix_args "$@"
}
ascii_client() {
    # shellcheck disable=SC2086
    env -u TMUX LC_ALL=C LANG=C LC_CTYPE=C "$binary" $prefix_args "$@"
}
tmux_client() {
    # shellcheck disable=SC2086
    env TMUX=/tmp/zz-sanitizer-fake,1,0 LC_ALL=C LANG=C LC_CTYPE=C \
        "$binary" $prefix_args "$@"
}
lang_client() {
    # shellcheck disable=SC2086
    env -u TMUX -u LC_ALL -u LC_CTYPE LANG=en_US.UTF-8 "$binary" $prefix_args "$@"
}
lc_ctype_client() {
    # shellcheck disable=SC2086
    env -u TMUX -u LC_ALL LC_CTYPE=C LANG=en_US.UTF-8 "$binary" $prefix_args "$@"
}

root="$HOME/client-utf8-sanitizer-$side"
rm -rf "$root"
mkdir -p "$root"

hex() {
    od -An -tx1 -v "$1" | tr -d ' \n'
}

bytes=$(printf 'a\377b')
wide=$(printf 'x\346\274\242y')
combining=$(printf 'e\314\201z')
tabbed=$(printf 'a\tb')

utf8_client set-environment -g ZZSAN_BYTES "$bytes"
utf8_client set-environment -g ZZSAN_WIDE "$wide"
utf8_client set-environment -g ZZSAN_COMB "$combining"
utf8_client set-environment -g ZZSAN_TAB "$tabbed"
utf8_client new-window -d -n sanitize 'sleep 30'

ascii_client show-environment -g ZZSAN_BYTES >"$root/ascii-bytes" 2>&1 || :
ascii_client show-environment -g ZZSAN_WIDE >"$root/ascii-wide" 2>&1 || :
ascii_client show-environment -g ZZSAN_COMB >"$root/ascii-comb" 2>&1 || :
ascii_client show-environment -g ZZSAN_TAB >"$root/ascii-tab" 2>&1 || :
ascii_client display-message -p 'x#{ZZSAN_WIDE}y' >"$root/ascii-format" 2>&1 || :

ascii_client list-panes -a -F '#{ZZSAN_BYTES}' >"$root/ascii-list" 2>&1 || :

utf8_client new-window -d -n captured 'printf "x\346\274\242y\n"; sleep 30'
i=0
while [ "$i" -lt 100 ]; do
    utf8_client capture-pane -p -t captured 2>/dev/null | head -1 >"$root/settle"
    case "$(hex "$root/settle")" in
    78e6bca2790a) break ;;
    esac
    i=$((i + 1))
    sleep 0.1
done
ascii_client capture-pane -p -t captured 2>/dev/null | head -1 >"$root/ascii-capture"

utf8_client show-environment -g ZZSAN_BYTES >"$root/utf8-bytes" 2>&1 || :
tmux_client show-environment -g ZZSAN_BYTES >"$root/tmux-bytes" 2>&1 || :
lang_client show-environment -g ZZSAN_BYTES >"$root/lang-bytes" 2>&1 || :
lc_ctype_client show-environment -g ZZSAN_BYTES >"$root/lcctype-bytes" 2>&1 || :

result=broken
if [ "$(hex "$root/ascii-bytes")" = 5a5a53414e5f42595445533d615f620a ] &&
    [ "$(hex "$root/ascii-wide")" = 5a5a53414e5f574944453d785f5f790a ] &&
    [ "$(hex "$root/ascii-comb")" = 5a5a53414e5f434f4d423d657a0a ] &&
    [ "$(hex "$root/ascii-tab")" = 5a5a53414e5f5441423d615f620a ] &&
    [ "$(hex "$root/ascii-format")" = 78785f5f79790a ] &&
    [ "$(hex "$root/ascii-list")" = 615f620a615f620a ] &&
    [ "$(hex "$root/utf8-bytes")" = 5a5a53414e5f42595445533d61ff620a ] &&
    [ "$(hex "$root/tmux-bytes")" = 5a5a53414e5f42595445533d61ff620a ] &&
    [ "$(hex "$root/lang-bytes")" = 5a5a53414e5f42595445533d61ff620a ] &&
    [ "$(hex "$root/lcctype-bytes")" = 5a5a53414e5f42595445533d615f620a ] &&
    [ "$(hex "$root/ascii-capture")" = 78e6bca2790a ]; then
    result=clean:11
fi

utf8_client set-environment -g ZZ_SANITIZE_RESULT "$result"

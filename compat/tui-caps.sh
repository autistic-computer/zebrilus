#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-caps.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-caps.sh\n' >&2
}

COMPAT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd -- "$COMPAT_DIR/.." && pwd)"
SELF_CHECK=0
POSITIONAL=()
for argument in "$@"; do
  case "$argument" in
  --self-check) SELF_CHECK=1 ;;
  -*)
    usage
    exit 2
    ;;
  *) POSITIONAL+=("$argument") ;;
  esac
done
[ "${#POSITIONAL[@]}" -le 2 ] || {
  usage
  exit 2
}
ZZ_INPUT="${POSITIONAL[0]:-${ZZ_BIN:-$REPO_DIR/target/debug/zz_cli}}"
TMUX_INPUT="${POSITIONAL[1]:-${TMUX_BIN:-${ZZ_COMPAT_TMUX:-$COMPAT_DIR/.cache/tmux-src/tmux}}}"

resolve_binary() {
  local input="$1"
  if [ -x "$input" ]; then
    printf '%s\n' "$(cd -- "$(dirname -- "$input")" && pwd)/$(basename -- "$input")"
    return 0
  fi
  command -v -- "$input"
}
ZZ_BIN="$(resolve_binary "$ZZ_INPUT")" || {
  printf 'error: zz binary not found: %s\n' "$ZZ_INPUT" >&2
  exit 2
}
TMUX_BIN="$(resolve_binary "$TMUX_INPUT")" || {
  printf 'error: tmux binary not found: %s\n' "$TMUX_INPUT" >&2
  exit 2
}

SCRATCH_DIR="$(mktemp -d /tmp/zzcaps.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzcapso-$TOKEN"
INNER_SOCKET_NAME="zzcapsi-$TOKEN"
ZZ_SOCKET="/tmp/zzcaps-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="caps"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
RUNTIME_DIR="$SCRATCH_DIR/run"
DIAGNOSTICS_DIR=""
CASE_UNDER_TEST=""
ZZ_PID=""
CHECKS=0
FAILURES=0
RECORDED=0
CASE_DIFFS=0
ASSERT_MODE=count
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
CASE_LOCALE=C
ZZ_PANE=""
TMUX_PANE_ID=""
SILENT_TERMINAL=0
SILENT_SIDE=""
WIDTH_RUN=0
PYTHON="$(command -v python3)"
mkdir -p "$ZZ_HOME/config" "$TMUX_HOME/config" "$OUTER_HOME/config" "$ZZ_LOG_DIR" "$RUNTIME_DIR"
chmod 700 "$RUNTIME_DIR"

MODE_NAMES=(
  alternate_on bracket_paste_flag cursor_flag cursor_shape wrap_flag
  mouse_all_flag mouse_any_flag mouse_button_flag mouse_sgr_flag
  mouse_standard_flag mouse_utf8_flag synchronized_output_flag origin_flag
  keypad_flag keypad_cursor_flag pane_key_mode
)
MODE_FORMAT='#{alternate_on}|#{bracket_paste_flag}|#{cursor_flag}|#{cursor_shape}|#{wrap_flag}|#{mouse_all_flag}|#{mouse_any_flag}|#{mouse_button_flag}|#{mouse_sgr_flag}|#{mouse_standard_flag}|#{mouse_utf8_flag}|#{synchronized_output_flag}|#{origin_flag}|#{keypad_flag}|#{keypad_cursor_flag}|#{pane_key_mode}'
MODE_RECORDED=""
modes_except() {
  local keep=" $* " name out=""
  for name in "${MODE_NAMES[@]}"; do
    [[ "$keep" == *" $name "* ]] && continue
    out="$out $name"
  done
  printf '%s\n' "${out# }"
}

FACT_NAMES=(client_termname client_utf8 client_flags client_colours client_termfeatures client_theme)
FACT_FORMAT='#{client_termname}|#{client_utf8}|#{client_flags}|#{client_colours}|#{client_termfeatures}|#{client_theme}'
BASELINE_ZZ=""
BASELINE_PIN=""

scrubbed() {
  env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL \
    -u XDG_STATE_HOME -u ZZ_LOG_DIR -u COLORTERM -u TERM_PROGRAM \
    LANG=C LC_ALL=C TMUX_TMPDIR=/tmp XDG_RUNTIME_DIR="$RUNTIME_DIR" "$@"
}
tmux_outer_command() {
  scrubbed HOME="$OUTER_HOME" XDG_CONFIG_HOME="$OUTER_HOME/config" TERM=xterm-256color \
    "$TMUX_BIN" -L "$OUTER_SOCKET_NAME" "$@"
}
zz_command() {
  scrubbed HOME="$ZZ_HOME" XDG_CONFIG_HOME="$ZZ_HOME/config" ZZ_LOG_DIR="$ZZ_LOG_DIR" \
    TERM=xterm-256color "$ZZ_BIN" --socket "$ZZ_SOCKET" "$@"
}
tmux_inner_command() {
  scrubbed HOME="$TMUX_HOME" XDG_CONFIG_HOME="$TMUX_HOME/config" TERM=xterm-256color \
    "$TMUX_BIN" -L "$INNER_SOCKET_NAME" "$@"
}
side_command() {
  local side="$1"
  shift
  case "$side" in
  zz) zz_command "$@" ;;
  tmux) tmux_inner_command "$@" ;;
  esac
}
outer_window() {
  printf '%s\n' "=$OUTER_SESSION:w-$1"
}
side_pane() {
  case "$1" in
  zz) printf '%s\n' "$ZZ_PANE" ;;
  tmux) printf '%s\n' "$TMUX_PANE_ID" ;;
  esac
}

scratch_pids() {
  local entry
  for entry in /proc/[0-9]*; do
    if { tr '\0' '\n' <"$entry/environ"; } 2>/dev/null | grep -qF -- "$SCRATCH_DIR"; then
      printf '%s\n' "${entry#/proc/}"
    fi
  done
}
reap_scratch() {
  local pids attempt
  pids="$(scratch_pids)"
  [ -n "$pids" ] || return 0
  printf 'cleanup: reaping %s leftover processes of this run\n' "$(printf '%s\n' "$pids" | wc -l)" >&2
  kill $pids >/dev/null 2>&1
  for ((attempt = 0; attempt < 60; attempt++)); do
    [ -n "$(scratch_pids)" ] || return 0
    sleep 0.05
  done
  pids="$(scratch_pids)"
  [ -z "$pids" ] || kill -KILL $pids >/dev/null 2>&1
}

cleanup() {
  local status=$?
  trap - EXIT ERR INT TERM
  set +e
  tmux_outer_command kill-server >/dev/null 2>&1
  zz_command kill-server >/dev/null 2>&1
  tmux_inner_command kill-server >/dev/null 2>&1
  if [ -S "$RUNTIME_DIR/zz/default.sock" ]; then
    scrubbed HOME="$ZZ_HOME" "$ZZ_BIN" --socket "$RUNTIME_DIR/zz/default.sock" kill-server >/dev/null 2>&1
  fi
  if [ -n "$ZZ_PID" ]; then
    kill "$ZZ_PID" >/dev/null 2>&1
    wait "$ZZ_PID" >/dev/null 2>&1
  fi
  reap_scratch
  rm -f -- "$ZZ_SOCKET" "/tmp/tmux-$(id -u)/$OUTER_SOCKET_NAME" "/tmp/tmux-$(id -u)/$INNER_SOCKET_NAME"
  rm -rf -- "$SCRATCH_DIR"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

die() {
  printf 'error: %s\n' "$*" >&2
  exit 2
}

diagnostics_dir() {
  if [ -z "$DIAGNOSTICS_DIR" ]; then
    DIAGNOSTICS_DIR="${ZZ_CAPS_DIAGNOSTICS_DIR:-$(mktemp -d /tmp/zzcaps-diag.XXXXXX)}"
    mkdir -p "$DIAGNOSTICS_DIR"
  fi
  printf '%s\n' "$DIAGNOSTICS_DIR"
}

dump_diagnostics() {
  local label="$1"
  local dir side log
  dir="$(diagnostics_dir)"
  {
    printf 'wait that ran out: %s\n' "$label"
    printf 'at: %s\n' "$(date -Is 2>/dev/null || date)"
    printf 'case under test: %s\n' "${CASE_UNDER_TEST:-none yet}"
    printf 'zz: %s\n' "$ZZ_BIN"
    printf 'tmux: %s\n' "$TMUX_BIN"
    printf 'zz socket: %s\n' "$ZZ_SOCKET"
    printf 'outer socket: %s\n' "$OUTER_SOCKET_NAME"
    printf 'inner tmux socket: %s\n' "$INNER_SOCKET_NAME"
    printf 'ZZ_LOG_DIR (%s) holds:\n' "$ZZ_LOG_DIR"
    ls -1 -- "$ZZ_LOG_DIR" 2>&1 || true
  } >"$dir/what-fired.txt" 2>&1 || true
  for side in zz tmux; do
    tmux_outer_command capture-pane -p -e -S - -t "$(outer_window "$side")" \
      >"$dir/outer-$side.screen.txt" 2>&1 || true
    side_command "$side" list-clients \
      -F '#{client_name} session=#{client_session} #{client_width}x#{client_height} flags=#{client_flags}' \
      >"$dir/$side.list-clients.txt" 2>&1 || true
    side_command "$side" list-panes -a \
      -F '#{session_name}:#{window_index}.#{pane_index} #{pane_width}x#{pane_height} dead=#{pane_dead}' \
      >"$dir/$side.list-panes.txt" 2>&1 || true
  done
  tmux_outer_command list-panes -a \
    -F '#{window_name} #{pane_width}x#{pane_height} dead=#{pane_dead}' \
    >"$dir/outer.list-panes.txt" 2>&1 || true
  cp -f -- "$SCRATCH_DIR/zz-daemon.out" "$dir/zz-daemon.stdout.txt" 2>/dev/null || true
  cp -f -- "$SCRATCH_DIR/zz-daemon.err" "$dir/zz-daemon.stderr.txt" 2>/dev/null || true
  for log in "$ZZ_LOG_DIR"/*; do
    [ -f "$log" ] || continue
    cp -f -- "$log" "$dir/ring-$(basename -- "$log").txt" 2>/dev/null || true
  done
  printf 'diagnostics retained in %s:\n' "$dir" >&2
  ls -1 -- "$dir" >&2 || true
}

wait_for() {
  local label="$1"
  local attempt
  shift
  for ((attempt = 0; attempt < 200; attempt++)); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  dump_diagnostics "$label"
  die "$label did not happen within 10 seconds"
}

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null | head -n 1)" = "$INNER_SESSION" ]
}
client_gone() {
  [ -z "$(side_command "$1" list-clients -F x 2>/dev/null)" ]
}
outer_screen() {
  tmux_outer_command capture-pane -p -e -t "$(outer_window "$1")" 2>/dev/null
}

wait_settled() {
  local side="$1" marker="$2"
  local attempt current previous=""
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(outer_screen "$side")"
    if [ -n "$current" ] && [ "$current" = "$previous" ] && [[ "$current" == *"$marker"* ]]; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  dump_diagnostics "$side settled on $marker"
  die "$side did not settle on $marker within 10 seconds"
}

send_both() {
  local command="$1" side
  for side in zz tmux; do
    side_command "$side" send-keys -t "$(side_pane "$side")" "$command" Enter
  done
}
checkpoint() {
  local name="$1" side
  send_both "printf 'MARK-%s\\n' $name"
  for side in zz tmux; do
    wait_settled "$side" "MARK-$name"
  done
}

declare -A RECORD_OWNERS=([unattributed]=0)

known_drift() {
  case "$1" in
  esac
}

known_drift_reason() {
  case "$1" in
  esac
}

owner_tally() {
  local key entries=()
  for key in $(printf '%s\n' "${!RECORD_OWNERS[@]}" | LC_ALL=C sort); do
    entries+=("$key=${RECORD_OWNERS[$key]}")
  done
  printf 'owners %s' "${entries[*]}"
}

assert_row() {
  local name="$1" zz_value="$2" pin_value="$3" drift
  drift="$(known_drift "$name")"
  if [ -n "$drift" ]; then
    record_row "$name" "$zz_value" "$pin_value" "$(known_drift_reason "$drift")" "$drift"
    return 0
  fi
  CHECKS=$((CHECKS + 1))
  if [ "$zz_value" = "$pin_value" ]; then
    [ "$ASSERT_MODE" = count ] && printf 'ok    %s: both %s\n' "$name" "$zz_value"
    return 0
  fi
  CASE_DIFFS=$((CASE_DIFFS + 1))
  if [ "$ASSERT_MODE" = count ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s: tmux %s, zz %s\n' "$name" "$pin_value" "$zz_value"
  fi
}
record_row() {
  local name="$1" zz_value="$2" pin_value="$3" reason="${4:-}" owner="${5:-unattributed}"
  RECORDED=$((RECORDED + 1))
  RECORD_OWNERS[$owner]=$((${RECORD_OWNERS[$owner]:-0} + 1))
  [ "$ASSERT_MODE" = count ] || return 0
  if [ "$zz_value" = "$pin_value" ]; then
    printf 'note  %s: both %s\n' "$name" "$zz_value"
  else
    printf 'note  %s: tmux %s, zz %s\n' "$name" "$pin_value" "$zz_value"
  fi
  [ -z "$reason" ] || printf '      recorded, not asserted: %s\n' "$reason"
}
compare_row() {
  local recorded="$1" row="$2" label="$3" zz_value="$4" pin_value="$5"
  if [[ " $recorded " == *" $row "* ]]; then
    record_row "$label" "$zz_value" "$pin_value"
  else
    assert_row "$label" "$zz_value" "$pin_value"
  fi
}

read_modes() {
  tmux_outer_command display-message -p -t "$(outer_window "$1")" "$MODE_FORMAT"
}
read_facts() {
  side_command "$1" list-clients -F "$FACT_FORMAT" 2>/dev/null | head -n 1
}

compare_tuple() {
  local label="$1" recorded="$2" zz_tuple="$3" pin_tuple="$4"
  shift 4
  local names=("$@")
  local -a zz_fields pin_fields
  IFS='|' read -r -a zz_fields <<<"$zz_tuple"
  IFS='|' read -r -a pin_fields <<<"$pin_tuple"
  local index
  for index in "${!names[@]}"; do
    compare_row "$recorded" "${names[$index]}" "$label ${names[$index]}" \
      "${zz_fields[$index]-}" "${pin_fields[$index]-}"
  done
}

ZZ_ATTACH_PREFIX=""
write_attach() {
  local side="$1" destination="$2" term="$3"
  shift 3
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ] && [ -n "$ZZ_ATTACH_PREFIX" ]; then
    printf 'printf %q\n' "$ZZ_ATTACH_PREFIX" >>"$destination"
  fi
  if [ "$side" = zz ]; then
    printf 'exec env -i HOME=%q XDG_CONFIG_HOME=%q XDG_RUNTIME_DIR=%q ZZ_LOG_DIR=%q LANG=%q LC_ALL=%q TERM=%q PATH=%q TMUX_TMPDIR=/tmp %q --socket %q %s attach-session -t %q\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$RUNTIME_DIR" "$ZZ_LOG_DIR" "$CASE_LOCALE" "$CASE_LOCALE" \
      "$term" "$PATH" "$ZZ_BIN" "$ZZ_SOCKET" "$*" "=$INNER_SESSION" >>"$destination"
  else
    printf 'exec env -i HOME=%q XDG_CONFIG_HOME=%q XDG_RUNTIME_DIR=%q LANG=%q LC_ALL=%q TERM=%q PATH=%q TMUX_TMPDIR=/tmp %q -L %q %s attach-session -t %q\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$RUNTIME_DIR" "$CASE_LOCALE" "$CASE_LOCALE" "$term" \
      "$PATH" "$TMUX_BIN" "$INNER_SOCKET_NAME" "$*" "=$INNER_SESSION" >>"$destination"
  fi
  chmod +x "$destination"
}

write_silent_terminal() {
  cat >"$SCRATCH_DIR/silent-terminal.py" <<'PY'
import fcntl
import os
import pty
import sys
import termios
import tty

size = fcntl.ioctl(1, termios.TIOCGWINSZ, b"\0" * 8)
tty.setraw(0)
pid, master = pty.fork()
if pid == 0:
    fcntl.ioctl(0, termios.TIOCSWINSZ, size)
    os.execv(sys.argv[1], sys.argv[1:])
while True:
    try:
        data = os.read(master, 65536)
    except OSError:
        break
    if not data:
        break
    while data:
        data = data[os.write(1, data):]
os.waitpid(pid, 0)
PY
}

open_case() {
  local name="$1" term="$2" zz_flags="$3" tmux_flags="$4"
  local side
  CASE_UNDER_TEST="$name"
  for side in zz tmux; do
    tmux_outer_command kill-window -t "$(outer_window "$side")" >/dev/null 2>&1 || true
  done
  for side in zz tmux; do
    wait_for "$side client gone before $name" client_gone "$side"
  done
  write_attach zz "$SCRATCH_DIR/attach-zz.sh" "$term" $zz_flags
  write_attach tmux "$SCRATCH_DIR/attach-tmux.sh" "$term" $tmux_flags
  local silent
  for side in zz tmux; do
    silent="$SILENT_TERMINAL"
    if [ -n "$SILENT_SIDE" ]; then
      if [ "$SILENT_SIDE" = "$side" ]; then silent=1; else silent=0; fi
    fi
    if [ "$silent" = 1 ]; then
      tmux_outer_command new-window -d -n "w-$side" \
        "$PYTHON $SCRATCH_DIR/silent-terminal.py $SCRATCH_DIR/attach-$side.sh"
    else
      tmux_outer_command new-window -d -n "w-$side" "$SCRATCH_DIR/attach-$side.sh"
    fi
  done
  for side in zz tmux; do
    wait_for "outer $side pane for $name" outer_pane_is "$(outer_window "$side")" 80x24
    wait_for "$side client attached for $name" client_attached "$side"
  done
}

detach_case() {
  local side
  for side in zz tmux; do
    side_command "$side" detach-client -s "=$INNER_SESSION" >/dev/null 2>&1 || true
  done
  for side in zz tmux; do
    wait_for "$side client detached" client_gone "$side"
  done
}

case_modes() {
  local name="$1" term="$2" zz_flags="$3" tmux_flags="$4" recorded="${5:-$MODE_RECORDED}"
  open_case "$name" "$term" "$zz_flags" "$tmux_flags"
  checkpoint "${name//[^a-zA-Z0-9]/}"
  compare_tuple "$name" "$recorded" "$(read_modes zz)" "$(read_modes tmux)" "${MODE_NAMES[@]}"
}

case_extended_key() {
  local name="$1" side
  for side in zz tmux; do
    side_command "$side" set-option -gu @extkey >/dev/null 2>&1 || true
    side_command "$side" set-option -gu @extdone >/dev/null 2>&1 || true
    side_command "$side" bind-key -n C-Enter set-option -g @extkey C-Enter >/dev/null
    side_command "$side" bind-key -n F9 set-option -g @extdone 1 >/dev/null
  done
  open_case "$name" xterm-256color '' ''
  checkpoint "${name//[^a-zA-Z0-9]/}"
  for side in zz tmux; do
    tmux_outer_command send-keys -t "$(outer_window "$side")" C-Enter F9
  done
  for side in zz tmux; do
    wait_for "$side decoded F9 for $name" extended_done "$side"
  done
  assert_row "$name @extkey" \
    "$(side_command zz show-options -gqv @extkey 2>/dev/null)" \
    "$(side_command tmux show-options -gqv @extkey 2>/dev/null)"
  for side in zz tmux; do
    side_command "$side" unbind-key -n C-Enter >/dev/null 2>&1 || true
    side_command "$side" unbind-key -n F9 >/dev/null 2>&1 || true
  done
}
extended_done() {
  [ "$(side_command "$1" show-options -gqv @extdone 2>/dev/null)" = 1 ]
}

list_delta() {
  local base="$1" now="$2" item out=""
  local IFS=,
  for item in $now; do
    case ",$base," in
    *",$item,"*) ;;
    *) out="${out:+$out,}$item" ;;
    esac
  done
  printf '%s\n' "${out:-none}"
}
list_without() {
  local list="$1" exclude="$2" item out=""
  [ "$list" = none ] && list=""
  local IFS=,
  for item in $list; do
    case ",$exclude," in
    *",$item,"*) ;;
    *) out="${out:+$out,}$item" ;;
    esac
  done
  printf '%s\n' "${out:-none}"
}
features_present() {
  local roster="$1" names="$2" name out=""
  local IFS=,
  for name in $names; do
    case ",$roster," in
    *",$name,"*) out="${out:+$out,}$name:yes" ;;
    *) out="${out:+$out,}$name:no" ;;
    esac
  done
  printf '%s\n' "$out"
}
tuple_field() {
  local tuple="$1" index="$2"
  local -a fields
  IFS='|' read -r -a fields <<<"$tuple"
  printf '%s\n' "${fields[$index]-}"
}

case_facts() {
  local name="$1" term="$2" zz_flags="$3" tmux_flags="$4" recorded="${5:-}"
  local baseline="${6:-}" named="${7:-}"
  open_case "$name" "$term" "$zz_flags" "$tmux_flags"
  checkpoint "${name//[^a-zA-Z0-9]/}"
  local zz_tuple pin_tuple
  zz_tuple="$(read_facts zz)"
  pin_tuple="$(read_facts tmux)"
  compare_tuple "$name" "$recorded" "$zz_tuple" "$pin_tuple" "${FACT_NAMES[@]}"
  if [ "$baseline" = baseline ]; then
    BASELINE_ZZ="$zz_tuple"
    BASELINE_PIN="$pin_tuple"
    return 0
  fi
  [ -n "$BASELINE_ZZ" ] || return 0
  local index zz_base pin_base disagreed
  for index in 2 4; do
    zz_base="$(tuple_field "$BASELINE_ZZ" "$index")"
    pin_base="$(tuple_field "$BASELINE_PIN" "$index")"
    disagreed="$(list_delta "$zz_base" "$pin_base"),$(list_delta "$pin_base" "$zz_base")"
    compare_row "$recorded" "delta-${FACT_NAMES[$index]}" "$name delta-${FACT_NAMES[$index]}" \
      "$(list_without "$(list_delta "$zz_base" "$(tuple_field "$zz_tuple" "$index")")" "$disagreed")" \
      "$(list_without "$(list_delta "$pin_base" "$(tuple_field "$pin_tuple" "$index")")" "$disagreed")"
  done
  [ -n "$named" ] || return 0
  assert_row "$name flag-features" \
    "$(features_present "$(tuple_field "$zz_tuple" 4)" "$named")" \
    "$(features_present "$(tuple_field "$pin_tuple" 4)" "$named")"
}

case_restore() {
  open_case restore xterm '' ''
  checkpoint restore
  detach_case
  local zz_tuple pin_tuple
  zz_tuple="$(read_modes zz)"
  pin_tuple="$(read_modes tmux)"
  compare_tuple "restore/detached" "" "$zz_tuple" "$pin_tuple" "${MODE_NAMES[@]}"
}

case_clear_on_attach_off() {
  local side
  for side in zz tmux; do
    side_command "$side" set-option -g clear-on-attach off >/dev/null ||
      die "$side refused clear-on-attach off"
  done
  case_modes 'clear-on-attach/off' xterm '' ''
  assert_row 'clear-on-attach/off history_size' \
    "$(tmux_outer_command display-message -p -t "$(outer_window zz)" '#{history_size}')" \
    "$(tmux_outer_command display-message -p -t "$(outer_window tmux)" '#{history_size}')"
  detach_case
  compare_tuple "clear-on-attach/off/detached" "" "$(read_modes zz)" "$(read_modes tmux)" \
    "${MODE_NAMES[@]}"
  for side in zz tmux; do
    side_command "$side" set-option -gu clear-on-attach >/dev/null
  done
}

COLOUR_SAMPLE="printf 'CLR \\033[31mR\\033[0m \\033[38;5;42mI\\033[0m \\033[38;2;10;20;30mX\\033[0m \\033[41mB\\033[0m \\033[38;5;12mA\\033[0m \\033[38;5;9mQ\\033[0m END\\n'"
COLOUR_SAMPLE_RGB="printf 'CLR \\033[38;2;205;0;0mR\\033[0m \\033[38;5;42mI\\033[0m \\033[38;2;10;20;30mX\\033[0m \\033[41mB\\033[0m \\033[38;5;12mA\\033[0m \\033[38;5;9mQ\\033[0m END\\n'"
PALETTE_NAMED='\033]4;1;rgb:00/ff/00\033\\'
PALETTE_INDEXED='\033]4;42;rgb:ff/00/ff\033\\'
PALETTE_RESET='\033]104\033\\'
OSC_FOREGROUND='\033]10;rgb:ff/00/00\033\\'
OSC_FOREGROUND_RESET='\033]110\033\\'
OSC_BACKGROUND='\033]11;rgb:00/00/80\033\\'
OSC_BACKGROUND_RESET='\033]111\033\\'
colour_line() {
  outer_screen "$1" | grep -a 'CLR ' | grep -a 'END' | tail -n 1
}
strip_escapes() {
  printf '%s' "$1" | sed -e $'s/\033\\[[0-9;]*m//g'
}
cell_class() {
  local line="$1" glyph="$2" found
  found="$(printf '%s' "$line" | grep -ao $'\033\\[[0-9;]*m'"$glyph" | tail -n 1 | cat -v)"
  printf '%s\n' "${found:-none}"
}
colour_stage() {
  local stage="$1" zz_escape="$2" pin_escape="$3" zz_sample="$4" pin_sample="$5"
  local row="${6:-assert_row}"
  local zz_line pin_line glyph
  side_command zz send-keys -t "$(side_pane zz)" "printf '$zz_escape'; clear; $zz_sample" Enter
  side_command tmux send-keys -t "$(side_pane tmux)" "printf '$pin_escape'; clear; $pin_sample" Enter
  checkpoint "colour$stage"
  zz_line="$(colour_line zz)"
  pin_line="$(colour_line tmux)"
  assert_row "colours/$stage glyphs" "$(strip_escapes "$zz_line")" "$(strip_escapes "$pin_line")"
  for glyph in R I X B A Q; do
    "$row" "colours/$stage cell $glyph" \
      "$(cell_class "$zz_line" "$glyph")" "$(cell_class "$pin_line" "$glyph")"
  done
  "$row" "colours/$stage line" "$(printf '%s' "$zz_line" | cat -v)" \
    "$(printf '%s' "$pin_line" | cat -v)"
}

case_one_sided_autowrap() {
  ZZ_ATTACH_PREFIX='\033[?7l'
  case_modes 'sc/one-sided-autowrap' xterm '' '' "$(modes_except wrap_flag)"
  ZZ_ATTACH_PREFIX=""
}

pin_theme_known() {
  [ -n "$(tuple_field "$(read_facts tmux)" 5)" ]
}

case_menu_arming() {
  local name="$1" zz_flag="$2" pin_flag="$3" recorded="${4:-$MODE_RECORDED}" side
  side_command zz bind-key -T prefix E display-menu -x 4 -y 8 -T CAPSMENU $zz_flag \
    'Alpha item' a 'set-option -g @capsmenu alpha' >/dev/null
  side_command tmux bind-key -T prefix E display-menu -x 4 -y 8 -T CAPSMENU $pin_flag \
    'Alpha item' a 'set-option -g @capsmenu alpha' >/dev/null
  open_case "$name" xterm-256color '' ''
  checkpoint "${name//[^a-zA-Z0-9]/}"
  for side in zz tmux; do
    tmux_outer_command send-keys -t "$(outer_window "$side")" C-b E
  done
  for side in zz tmux; do
    wait_for "$side raised the menu for $name" menu_on_screen "$side"
    wait_settled "$side" CAPSMENU
  done
  compare_tuple "$name" "$recorded" "$(read_modes zz)" "$(read_modes tmux)" \
    "${MODE_NAMES[@]}"
  for side in zz tmux; do
    tmux_outer_command send-keys -t "$(outer_window "$side")" Escape
  done
  for side in zz tmux; do
    wait_for "$side dropped the menu for $name" menu_gone "$side"
    side_command "$side" unbind-key -T prefix E >/dev/null 2>&1 || true
  done
}
menu_on_screen() {
  [[ "$(outer_screen "$1")" == *CAPSMENU* ]]
}
menu_gone() {
  [[ "$(outer_screen "$1")" != *CAPSMENU* ]]
}
case_menu_arming_mouse_off() {
  local name="$1" zz_flag="$2" pin_flag="$3" recorded="${4:-$MODE_RECORDED}" side
  for side in zz tmux; do
    side_command "$side" set-option -g mouse off >/dev/null
  done
  case_menu_arming "$name" "$zz_flag" "$pin_flag" "$recorded"
  for side in zz tmux; do
    side_command "$side" set-option -gu mouse >/dev/null 2>&1 || true
  done
}

case_one_sided_focus_follows_mouse() {
  local who="$1" recorded="$2" side
  for side in zz tmux; do
    [ "$who" = both ] || [ "$who" = "$side" ] || continue
    side_command "$side" set-option -g focus-follows-mouse on >/dev/null
  done
  case_modes "sc/focus-follows-mouse-$who" xterm-256color '' '' "$recorded"
  for side in zz tmux; do
    side_command "$side" set-option -gu focus-follows-mouse >/dev/null 2>&1 || true
  done
}

case_one_sided_theme_reply() {
  open_case 'sc/one-sided-theme' xterm '' ''
  tmux_outer_command set-option -w -t "$(outer_window tmux)" window-style bg=colour4 \
    >/dev/null
  wait_for 'the pin learned a theme from its outer window' pin_theme_known
  compare_tuple 'sc/one-sided-theme' '' \
    "$(read_facts zz)" "$(read_facts tmux)" "${FACT_NAMES[@]}"
  tmux_outer_command set-option -w -t "$(outer_window tmux)" -u window-style \
    >/dev/null 2>&1 || true
}

case_one_sided_detach() {
  open_case 'sc/one-sided-detach' xterm '' ''
  checkpoint sconesideddetach
  side_command zz detach-client -s "=$INNER_SESSION" >/dev/null 2>&1 || true
  wait_for 'zz client detached for sc/one-sided-detach' client_gone zz
  compare_tuple 'sc/one-sided-detach' \
    "$(modes_except keypad_flag keypad_cursor_flag)" \
    "$(read_modes zz)" "$(read_modes tmux)" "${MODE_NAMES[@]}"
}

case_silent_colours() {
  local name="$1" stage="$2" zz_flags="$3" pin_flags="$4"
  local previous="$SILENT_TERMINAL"
  SILENT_TERMINAL=1
  open_case "$name" xterm "$zz_flags" "$pin_flags"
  colour_stage "$stage" '' '' "$COLOUR_SAMPLE" "$COLOUR_SAMPLE"
  SILENT_TERMINAL="$previous"
}

case_silent_tc() {
  local name="$1" zz_override="$2" pin_override="$3"
  local previous="$SILENT_TERMINAL"
  [ -z "$zz_override" ] || side_command zz set-option -sa terminal-overrides "$zz_override" >/dev/null
  [ -z "$pin_override" ] || side_command tmux set-option -sa terminal-overrides "$pin_override" >/dev/null
  SILENT_TERMINAL=1
  open_case "$name" xterm-256color '' ''
  colour_stage "${name//[^a-zA-Z0-9]/}" '' '' "$COLOUR_SAMPLE" "$COLOUR_SAMPLE"
  SILENT_TERMINAL="$previous"
  side_command zz set-option -su terminal-overrides >/dev/null
  side_command tmux set-option -su terminal-overrides >/dev/null
}

USER_KEY_SEQUENCE=$'\033[99~'
case_user_key() {
  local name="$1" zz_key="$2" pin_key="$3" side
  for side in zz tmux; do
    side_command "$side" set-option -gu @userkey >/dev/null 2>&1 || true
    side_command "$side" set-option -gu @extdone >/dev/null 2>&1 || true
    side_command "$side" bind-key -n User3 set-option -g @userkey User3 >/dev/null
    side_command "$side" bind-key -n F9 set-option -g @extdone 1 >/dev/null
  done
  [ -z "$zz_key" ] || side_command zz set-option -s 'user-keys[3]' "$zz_key" >/dev/null
  [ -z "$pin_key" ] || side_command tmux set-option -s 'user-keys[3]' "$pin_key" >/dev/null
  open_case "$name" xterm-256color '' ''
  checkpoint "${name//[^a-zA-Z0-9]/}"
  for side in zz tmux; do
    tmux_outer_command send-keys -t "$(outer_window "$side")" -H 1b 5b 39 39 7e
    tmux_outer_command send-keys -t "$(outer_window "$side")" F9
  done
  for side in zz tmux; do
    wait_for "$side decoded F9 for $name" extended_done "$side"
  done
  assert_row "$name @userkey" \
    "$(side_command zz show-options -gqv @userkey 2>/dev/null)" \
    "$(side_command tmux show-options -gqv @userkey 2>/dev/null)"
  for side in zz tmux; do
    side_command "$side" unbind-key -n User3 >/dev/null 2>&1 || true
    side_command "$side" unbind-key -n F9 >/dev/null 2>&1 || true
    side_command "$side" set-option -su user-keys >/dev/null 2>&1 || true
  done
}

case_silent() {
  SILENT_TERMINAL=1
  case_facts 'silent/bare' xterm '' '' '' baseline
  case_facts 'silent/-T' xterm '-T RGB' '-T RGB' '' '' RGB
  case_facts 'silent/-2' xterm -2 -2 '' '' 256
  side_command zz set-option -s extended-keys on >/dev/null
  side_command tmux set-option -s extended-keys on >/dev/null
  case_modes 'silent/extended' xterm-256color '' ''
  side_command zz set-option -s extended-keys off >/dev/null
  side_command tmux set-option -s extended-keys off >/dev/null
  case_silent_colours 'silent/colours' silent '' ''
  case_silent_colours 'silent/colours-2' silent-2 -2 -2
  SILENT_TERMINAL=0
  BASELINE_ZZ=""
  BASELINE_PIN=""
}

case_colours() {
  local sabotage="${1:-none}"
  local pin_sample="$COLOUR_SAMPLE" pin_named="$PALETTE_NAMED"
  local pin_indexed="$PALETTE_INDEXED" pin_reset="$PALETTE_RESET"
  case "$sabotage" in
  named-as-rgb) pin_sample="$COLOUR_SAMPLE_RGB" ;;
  named-entry) pin_named='\033]4;1;rgb:00/00/ff\033\\' ;;
  indexed-entry) pin_indexed='' ;;
  reset) pin_reset='' ;;
  esac
  local pin_pane='1=#123456 42=colour200' pin_other='3=#654321'
  local pin_osc_foreground="$OSC_FOREGROUND" pin_window='fg=colour2,bg=colour4'
  case "$sabotage" in
  pane-entry) pin_pane='' ;;
  pane-untouched) pin_other='1=#123456 3=#654321' ;;
  osc-foreground) pin_osc_foreground='' ;;
  window-style) pin_window='' ;;
  esac
  open_case colours xterm-256color '' ''
  colour_stage stock '' '' "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage palette "$PALETTE_NAMED" "$pin_named" "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage indexed "$PALETTE_INDEXED" "$pin_indexed" "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage reset "$PALETTE_RESET" "$pin_reset" "$COLOUR_SAMPLE" "$pin_sample"
  pane_colours_set zz 1=#123456 42=colour200
  pane_colours_set tmux $pin_pane
  colour_stage pane '' '' "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage pane-osc "$PALETTE_NAMED" "$PALETTE_NAMED" "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage pane-osc-reset "$PALETTE_RESET" "$PALETTE_RESET" "$COLOUR_SAMPLE" "$pin_sample"
  pane_colours_set zz 3=#654321
  pane_colours_set tmux $pin_other
  colour_stage pane-untouched '' '' "$COLOUR_SAMPLE" "$pin_sample"
  pane_colours_set zz
  pane_colours_set tmux
  colour_stage pane-unset '' '' "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage osc-10 "$OSC_FOREGROUND" "$pin_osc_foreground" "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage osc-110 "$OSC_FOREGROUND_RESET" "$OSC_FOREGROUND_RESET" \
    "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage osc-11 "$OSC_BACKGROUND" "$OSC_BACKGROUND" "$COLOUR_SAMPLE" "$pin_sample"
  colour_stage osc-111 "$OSC_BACKGROUND_RESET" "$OSC_BACKGROUND_RESET" \
    "$COLOUR_SAMPLE" "$pin_sample"
  window_style_set zz 'fg=colour2,bg=colour4'
  window_style_set tmux "$pin_window"
  colour_stage window-style '' '' "$COLOUR_SAMPLE" "$pin_sample"
  window_style_set zz 'fg=#102030,bg=colour200'
  window_style_set tmux 'fg=#102030,bg=colour200'
  colour_stage window-style-rgb '' '' "$COLOUR_SAMPLE" "$pin_sample"
  window_style_set zz
  window_style_set tmux
  colour_stage window-style-unset '' '' "$COLOUR_SAMPLE" "$pin_sample"
}

window_style_set() {
  local side="$1" style="${2:-}"
  if [ -n "$style" ]; then
    side_command "$side" set-option -w -t "$(side_pane "$side")" window-style "$style" >/dev/null
  else
    side_command "$side" set-option -wu -t "$(side_pane "$side")" window-style >/dev/null
  fi
}

pane_colours_set() {
  local side="$1" entry
  shift
  side_command "$side" set-option -wu -t "$(side_pane "$side")" pane-colours >/dev/null
  for entry in "$@"; do
    side_command "$side" set-option -w -t "$(side_pane "$side")" \
      "pane-colours[${entry%%=*}]" "${entry#*=}" >/dev/null
  done
}

WIDTH_SUFFIX=" \344\270\226\347\225\214 | e\314\201 | \360\237\230\200 |"
case_widths() {
  local zz_suffix="${1:-$WIDTH_SUFFIX}" pin_suffix="${2:-$WIDTH_SUFFIX}"
  local label="${3:-widths}" zz_flags="${4:-}" pin_flags="${5:-}"
  open_case "$label" xterm-256color "$zz_flags" "$pin_flags"
  WIDTH_RUN=$((WIDTH_RUN + 1))
  local marker="W$WIDTH_RUN"
  side_command zz send-keys -t "$(side_pane zz)" \
    "clear; printf 'W%s$zz_suffix' $WIDTH_RUN" Enter
  side_command tmux send-keys -t "$(side_pane tmux)" \
    "clear; printf 'W%s$pin_suffix' $WIDTH_RUN" Enter
  local side
  for side in zz tmux; do
    wait_settled "$side" "$marker"
  done
  assert_row "$label/cursor" \
    "$(tmux_outer_command display-message -p -t "$(outer_window zz)" '#{cursor_x},#{cursor_y}')" \
    "$(tmux_outer_command display-message -p -t "$(outer_window tmux)" '#{cursor_x},#{cursor_y}')"
  compare_row "${6:-}" line "$label/line" \
    "$(outer_screen zz | grep -a "$marker" | head -n 1 | sed "s/$marker/W/" | cat -v)" \
    "$(outer_screen tmux | grep -a "$marker" | head -n 1 | sed "s/$marker/W/" | cat -v)"
}

normalise_diagnostic() {
  sed -e 's/^tmux: /PROG: /' -e 's/^zz: /PROG: /' \
    -e 's/^usage: tmux /usage: PROG /' -e 's/^usage: zz /usage: PROG /' \
    -e 's/^tmux [A-Za-z0-9.-]*$/tmux VERSION/'
}
run_cli() {
  local side="$1"
  shift
  local binary=("$ZZ_BIN")
  [ "$side" = zz ] || binary=("$TMUX_BIN")
  local output status
  output="$(scrubbed HOME="$SCRATCH_DIR/cli-home" XDG_CONFIG_HOME="$SCRATCH_DIR/cli-home/config" \
    TMUX_TMPDIR="$RUNTIME_DIR" TERM=xterm-256color "${binary[@]}" "$@" 2>&1)" && status=0 || status=$?
  printf 'exit=%s\n%s\n' "$status" "$(printf '%s' "$output" | normalise_diagnostic)"
}
case_cli() {
  local zz_extra="$1" tmux_extra="$2"
  mkdir -p "$SCRATCH_DIR/cli-home/config"
  local label
  for label in '-2 -V' '-u -V' '-T sixel -V' '-T' '-W -V'; do
    local -a arguments
    read -r -a arguments <<<"$label"
    assert_row "cli/$label" \
      "$(run_cli zz $zz_extra "${arguments[@]}")" \
      "$(run_cli tmux $tmux_extra "${arguments[@]}")"
  done
}

write_silent_terminal
zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

zz_command new-session -d -s "$INNER_SESSION" -x 80 -y 24 "$INNER_SHELL" ||
  die "could not create the zz session"
tmux_inner_command -f /dev/null new-session -d -s "$INNER_SESSION" -x 80 -y 24 "$INNER_SHELL" ||
  die "could not create the tmux session"
ZZ_PANE="$(zz_command list-panes -t "=$INNER_SESSION" -F '#{pane_id}' | head -n 1)"
TMUX_PANE_ID="$(tmux_inner_command list-panes -t "=$INNER_SESSION" -F '#{pane_id}' | head -n 1)"
[ -n "$ZZ_PANE" ] && [ -n "$TMUX_PANE_ID" ] || die "an inner session has no pane"

tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n hold -x 80 -y 24 \
  'sleep 86400' || die "could not create the outer session"
tmux_outer_command set-option -g status off
tmux_outer_command set-option -s extended-keys on

printf 'outer-terminal capability differential (pin %s)\n' "$(basename -- "$TMUX_BIN")"

if [ "$SELF_CHECK" -eq 0 ]; then
  case_modes 'legacy' xterm '' ''
  case_restore
  case_clear_on_attach_off
  case_extended_key 'keys/off'
  side_command zz set-option -s extended-keys on >/dev/null
  side_command tmux set-option -s extended-keys on >/dev/null
  case_modes 'extended' xterm-256color '' ''
  case_extended_key 'keys/on'
  side_command zz set-option -s extended-keys always >/dev/null
  side_command tmux set-option -s extended-keys always >/dev/null
  case_modes 'extended-always' xterm-256color '' ''
  case_extended_key 'keys/always'
  side_command zz set-option -s extended-keys off >/dev/null
  side_command tmux set-option -s extended-keys off >/dev/null
  case_menu_arming 'menu/nomouse' '' ''
  case_menu_arming 'menu/mouse-keys' -M -M
  case_menu_arming_mouse_off 'menu/nomouse-mouse-off' '' ''
  case_menu_arming_mouse_off 'menu/mouse-keys-mouse-off' -M -M
  case_facts 'facts/bare' xterm '' '' '' baseline
  case_facts 'facts/-2' xterm -2 -2 '' '' 256
  case_facts 'facts/-u' xterm -u -u
  case_facts 'facts/-T' xterm '-T sixel' '-T sixel' '' '' sixel
  CASE_LOCALE=C.UTF-8
  case_facts 'facts/utf8-locale' xterm '' ''
  case_widths
  CASE_LOCALE=C
  case_widths "$WIDTH_SUFFIX" "$WIDTH_SUFFIX" widths/non-utf8
  case_widths "$WIDTH_SUFFIX" "$WIDTH_SUFFIX" widths/-u -u -u
  case_widths "$WIDTH_SUFFIX" "$WIDTH_SUFFIX" widths/-T-utf8 '-T utf8' '-T utf8'
  case_colours
  case_silent
  case_silent_tc 'silent/tc' ',*:Tc' ',*:Tc'
  case_user_key 'keys/user' "$USER_KEY_SEQUENCE" "$USER_KEY_SEQUENCE"
  case_cli '' ''

  printf '%s asserted rows, %s recorded rows\n' "$CHECKS" "$RECORDED"
  printf '%s recorded not asserted (%s)\n' "$RECORDED" "$(owner_tally)"
  if [ "$FAILURES" -ne 0 ]; then
    printf '%s of %s asserted rows differ\n' "$FAILURES" "$CHECKS"
    exit 1
  fi
  printf 'all %s asserted rows identical\n' "$CHECKS"
  exit 0
fi

SELF_CHECK_FAILURES=0
ASSERT_MODE=probe

self_check_case() {
  local name="$1" expectation="$2"
  shift 2
  CASE_DIFFS=0
  "$@"
  if [ "$expectation" = catches ] && [ "$CASE_DIFFS" -gt 0 ]; then
    printf 'ok    self-check %s: caught\n' "$name"
    return 0
  fi
  if [ "$expectation" = quiet ] && [ "$CASE_DIFFS" -eq 0 ]; then
    printf 'ok    self-check %s: no difference reported\n' "$name"
    return 0
  fi
  SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
  printf 'FAIL  self-check %s: %s asserted rows differed, expected %s\n' \
    "$name" "$CASE_DIFFS" "$expectation"
}

printf 'self-check: one deliberate one-sided difference per channel\n'

self_check_case 'control, facts with no sabotage' quiet \
  case_facts 'sc/control' xterm '' '' '' baseline

self_check_case 'a one-sided flag, -u on the pin only' catches \
  case_facts 'sc/one-sided-u' xterm '' -u

self_check_case 'a one-sided -T sixel on the pin only' catches \
  case_facts 'sc/one-sided-T' xterm '' '-T sixel' '' '' sixel

self_check_case 'an OSC 10 sent to zz only' catches case_colours osc-foreground
self_check_case 'window-style set on zz only' catches case_colours window-style

SILENT_TERMINAL=1
self_check_case 'control, a silent terminal with no sabotage' quiet \
  case_facts 'sc/silent-control' xterm '' '' '' baseline
self_check_case 'a one-sided 256 beside -T RGB on the pin, silent terminal' catches \
  case_facts 'sc/silent-T' xterm '-T RGB' '-T 256,RGB' '' '' RGB

self_check_case 'a one-sided -2 on the pin only, silent terminal' catches \
  case_facts 'sc/silent-2' xterm '' -2 '' '' 256

SILENT_SIDE=zz
self_check_case 'a terminal that answers for the pin alone' catches \
  case_facts 'sc/one-sided-replies' xterm '' ''
SILENT_SIDE=""
SILENT_TERMINAL=0

SILENT_TERMINAL=1
side_command zz set-option -s extended-keys on >/dev/null
side_command tmux set-option -s extended-keys on >/dev/null
self_check_case 'control, extended keys on both over a terminal naming none' quiet \
  case_modes 'sc/silent-extended-control' xterm-256color '' ''
self_check_case '-T extkeys named for zz alone on a silent terminal' catches \
  case_modes 'sc/silent-extkeys' xterm-256color '-T extkeys' '' \
  "$(modes_except pane_key_mode)"
side_command zz set-option -s extended-keys off >/dev/null
side_command tmux set-option -s extended-keys off >/dev/null
SILENT_TERMINAL=0

self_check_case 'control, the silent colour sample with no sabotage' quiet \
  case_silent_colours 'sc/silent-colours' sc-silent '' ''
self_check_case 'a one-sided -2 on the pin only, silent terminal' catches \
  case_silent_colours 'sc/silent-colours-pin-2' sc-silent-pin-2 '' -2
self_check_case 'a one-sided -2 on zz only, silent terminal' catches \
  case_silent_colours 'sc/silent-colours-zz-2' sc-silent-zz-2 -2 ''
self_check_case 'control, a Tc override on both sides of a silent terminal' quiet \
  case_silent_tc 'sc/silent-tc' ',*:Tc' ',*:Tc'
self_check_case 'a Tc override on the pin only, silent terminal' catches \
  case_silent_tc 'sc/silent-tc-pin' '' ',*:Tc'
self_check_case 'control, a user key on both sides' quiet \
  case_user_key 'sc/user-key' "$USER_KEY_SEQUENCE" "$USER_KEY_SEQUENCE"
self_check_case 'user-keys set on the pin only' catches \
  case_user_key 'sc/user-key-pin' '' "$USER_KEY_SEQUENCE"

self_check_case 'autowrap turned off on zz side before it attaches' catches \
  case_one_sided_autowrap

self_check_case 'control, focus-follows-mouse on both sides' quiet \
  case_one_sided_focus_follows_mouse both ''
self_check_case 'focus-follows-mouse on the pin only' catches \
  case_one_sided_focus_follows_mouse tmux "$(modes_except mouse_all_flag mouse_button_flag)"

self_check_case 'control, the plain menu on both sides' quiet \
  case_menu_arming 'sc/menu-nomouse-both' '' ''
self_check_case 'the -M menu on zz only' catches \
  case_menu_arming 'sc/menu-one-sided-M' -M '' \
  "$(modes_except mouse_all_flag mouse_button_flag)"
self_check_case 'the -M menu on zz only, mouse off' catches \
  case_menu_arming_mouse_off 'sc/menu-one-sided-M-mouse-off' -M '' \
  "$(modes_except mouse_all_flag mouse_any_flag mouse_button_flag mouse_sgr_flag)"

self_check_case 'zz detached while the pin stays attached' catches \
  case_one_sided_detach

self_check_case 'a theme reply the pin alone can get' catches \
  case_one_sided_theme_reply

self_check_case 'a named cell spelled as its RGB colour on the pin only' catches \
  case_colours named-as-rgb

self_check_case 'a one-sided palette entry' catches case_colours named-entry

self_check_case 'an OSC 4 on an indexed entry sent to zz only' catches \
  case_colours indexed-entry

self_check_case 'an OSC 104 sent to zz only' catches case_colours reset

self_check_case 'pane-colours set on zz only' catches case_colours pane-entry

self_check_case 'pane-colours naming the untouched entry on the pin only' catches \
  case_colours pane-untouched

side_command tmux set-option -s extended-keys on >/dev/null
self_check_case 'the legacy case driven with extended keys on the pin' catches \
  case_modes 'sc/legacy-extended' xterm '' ''
self_check_case 'an extended key with extended keys on the pin only' catches \
  case_extended_key 'sc/extended-key'
side_command tmux set-option -s extended-keys off >/dev/null

self_check_case 'a one-sided unknown CLI option' catches case_cli '' -W

CASE_LOCALE=C.UTF-8
self_check_case 'a one-sided wide codepoint' catches \
  case_widths "$WIDTH_SUFFIX" "$WIDTH_SUFFIX\344\270\226"
CASE_LOCALE=C

self_check_case 'a one-sided -u under a C locale' catches \
  case_widths "$WIDTH_SUFFIX" "$WIDTH_SUFFIX" sc/one-sided-codeset -u ''

ASSERT_MODE=count
if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
  printf '%s self-check cases did not behave as required\n' "$SELF_CHECK_FAILURES"
  exit 1
fi
printf 'self-check: every sabotage caught in its own channel\n'
exit 0

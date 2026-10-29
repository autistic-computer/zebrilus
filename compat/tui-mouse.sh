#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-mouse.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-mouse.sh\n' >&2
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
[ "${#POSITIONAL[@]}" -le 2 ] || { usage; exit 2; }
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
ZZ_BIN="$(resolve_binary "$ZZ_INPUT")" || { printf 'error: zz binary not found: %s\n' "$ZZ_INPUT" >&2; exit 2; }
TMUX_BIN="$(resolve_binary "$TMUX_INPUT")" || { printf 'error: tmux binary not found: %s\n' "$TMUX_INPUT" >&2; exit 2; }

COLUMNS_UNDER_TEST=80
ROWS_UNDER_TEST=24
SCRATCH_DIR="$(mktemp -d /tmp/zzms.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzmso-$TOKEN"
INNER_SOCKET_NAME="zzmsi-$TOKEN"
ZZ_SOCKET="/tmp/zzms-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="mus"
WINDOW_NAME="win"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
CASE_LABEL=""
FAILURES=0
CHECKS=0
RECORDS=0
declare -A RECORD_OWNERS=([unattributed]=0)
LAST_DIFFERED=0
MESSAGE_HOLD_MS=20000
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
SABOTAGE=""
mkdir -p "$ZZ_HOME" "$TMUX_HOME" "$OUTER_HOME" "$ZZ_LOG_DIR"

scrubbed() {
  env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL \
    -u XDG_STATE_HOME -u ZZ_LOG_DIR -u XDG_RUNTIME_DIR \
    TMUX_TMPDIR=/tmp "$@"
}
tmux_outer_command() {
  scrubbed HOME="$OUTER_HOME" XDG_CONFIG_HOME="$OUTER_HOME/config" \
    "$TMUX_BIN" -L "$OUTER_SOCKET_NAME" "$@"
}
zz_command() {
  scrubbed HOME="$ZZ_HOME" XDG_CONFIG_HOME="$ZZ_HOME/config" ZZ_LOG_DIR="$ZZ_LOG_DIR" \
    "$ZZ_BIN" --socket "$ZZ_SOCKET" "$@"
}
tmux_inner_command() {
  scrubbed HOME="$TMUX_HOME" XDG_CONFIG_HOME="$TMUX_HOME/config" \
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

cleanup() {
  local status=$?
  local pid
  trap - EXIT ERR INT TERM
  set +e
  tmux_outer_command kill-server >/dev/null 2>&1
  zz_command kill-server >/dev/null 2>&1
  tmux_inner_command kill-server >/dev/null 2>&1
  for pid in $(pgrep -f -- "$SCRATCH_DIR" 2>/dev/null); do
    kill "$pid" >/dev/null 2>&1
  done
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

dump_state() {
  local label="$1" side
  printf 'wait that ran out: %s (case %s)\n' "$label" "${CASE_LABEL:-none yet}" >&2
  for side in zz tmux; do
    printf -- '--- %s screen ---\n' "$side" >&2
    tmux_outer_command capture-pane -p -t "=$OUTER_SESSION:$side" 2>&1 | cat -v >&2 || true
    printf -- '--- %s panes ---\n' "$side" >&2
    side_command "$side" list-panes -t "=$INNER_SESSION" \
      -F '#{pane_id} active=#{pane_active} #{pane_left},#{pane_top} #{pane_width}x#{pane_height} mode=#{pane_in_mode}' >&2 2>&1 || true
  done
}

wait_for() {
  local label="$1" attempt
  shift
  for ((attempt = 0; attempt < 200; attempt++)); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  dump_state "$label"
  die "$label did not happen within 10 seconds"
}

wait_at_most() {
  local attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

capture_screen() {
  tmux_outer_command capture-pane -p -e -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
capture_plain() {
  tmux_outer_command capture-pane -p -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" = "$INNER_SESSION" ]
}
screen_has() {
  capture_plain "$1" | grep -Fq -- "$2"
}
screen_lacks() {
  screen_has "$1" "$2" && return 1
  return 0
}
both_screen_has() {
  wait_for "$2 on the zz screen" screen_has zz "$1"
  wait_for "$2 on the tmux screen" screen_has tmux "$1"
}
both_screen_lacks() {
  wait_for "$2 gone from the zz screen" screen_lacks zz "$1"
  wait_for "$2 gone from the tmux screen" screen_lacks tmux "$1"
}
wait_settled() {
  local side="$1" marker="$2" label="$3" previous="" current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_screen "$side" 2>/dev/null || true)"
    if [ -n "$previous" ] && [ "$current" = "$previous" ] &&
      printf '%s' "$current" | grep -Fq -- "$marker"; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  dump_state "$label"
  die "$label did not settle within 10 seconds"
}
settle_both() {
  wait_settled zz "$1" "$2 settled on the zz screen"
  wait_settled tmux "$1" "$2 settled on the tmux screen"
}

send_bytes() {
  local side="$1" text="$2" hex
  hex="$(printf '%s' "$text" | od -An -tx1 | tr -s ' \n' '  ')"
  # shellcheck disable=SC2086
  tmux_outer_command send-keys -t "=$OUTER_SESSION:$side" -H $hex ||
    die "the outer tmux refused send-keys -H for $side"
}
send_mouse() {
  local side="$1" button="$2" column="$3" row="$4" kind="$5"
  send_bytes "$side" "$(printf '\033[<%s;%s;%s%s' "$button" "$column" "$row" "$kind")"
}
send_mouse_both() {
  send_mouse zz "$@"
  send_mouse tmux "$@"
}
CLICK_SABOTAGE_SIDE=""
CLICK_SABOTAGE_COLUMN=""
CLICK_SABOTAGE_ROW=""
BORDER_SABOTAGE_COLUMN=""
COPY_PASTE_SABOTAGE_SIDE=""
click_both() {
  local button="$1" column="$2" row="$3" side aimed aimed_row
  for side in zz tmux; do
    aimed="$column"
    aimed_row="$row"
    if [ "$side" = "$CLICK_SABOTAGE_SIDE" ]; then
      [ -n "$CLICK_SABOTAGE_COLUMN" ] && aimed="$CLICK_SABOTAGE_COLUMN"
      [ -n "$CLICK_SABOTAGE_ROW" ] && aimed_row="$CLICK_SABOTAGE_ROW"
    fi
    send_mouse "$side" "$button" "$aimed" "$aimed_row" M
    send_mouse "$side" "$button" "$aimed" "$aimed_row" m
  done
}
WHEEL_SABOTAGE_SIDE=""
send_wheel_both() {
  local button="$1" column="$2" row="$3" side
  for side in zz tmux; do
    [ "$side" = "$WHEEL_SABOTAGE_SIDE" ] && continue
    send_mouse "$side" "$button" "$column" "$row" M
  done
}

reset_click_sequence_both() {
  local column="$1" row="$2"
  send_mouse_both 0 "$column" "$row" M
  send_mouse_both 0 "$column" "$row" m
}

pane_geometry() {
  side_command "$1" display-message -p -t "$2" \
    '#{pane_left} #{pane_top} #{pane_right} #{pane_bottom} #{pane_width} #{pane_height}'
}
pane_field() {
  pane_geometry "$1" "$2" | awk -v n="$3" '{ print $n }'
}
status_row() {
  printf '%s\n' "$ROWS_UNDER_TEST"
}
active_pane_index() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_index}' 2>/dev/null |
    awk '$1 == 1 { print $2; exit }'
}
active_pane_index_is() {
  [ "$(active_pane_index "$1")" = "$2" ]
}
pane_in_mode() {
  side_command "$1" display-message -p -t "$2" '#{pane_in_mode}' 2>/dev/null
}
pane_in_mode_is() {
  [ "$(pane_in_mode "$1" "$2")" = "$3" ]
}

assert_value() {
  compare_value same "$1" "$2" "$3"
}
check_value() {
  local prefix="$1" mode_name="${1}_MODE" reason_name="${1}_REASON"
  shift
  RECORD_REASON="${!reason_name}"
  compare_value "${!mode_name}" "$@"
  RECORD_REASON=""
}
case_owner() {
  case "$1" in
  status-clicks/*)
    printf 'gap:menus.client-owned'
    ;;
  esac
}
note_record() {
  local owner
  owner="$(case_owner "$1")"
  [ -n "$owner" ] || owner=unattributed
  RECORD_OWNERS[$owner]=$((${RECORD_OWNERS[$owner]:-0} + 1))
  RECORDS=$((RECORDS + 1))
}
owner_tally() {
  local key entries=()
  for key in $(printf '%s\n' "${!RECORD_OWNERS[@]}" | LC_ALL=C sort); do
    entries+=("$key=${RECORD_OWNERS[$key]}")
  done
  printf 'owners %s' "${entries[*]}"
}
compare_value() {
  local mode="$1" name="$2" zz_value="$3" pin_value="$4"
  if [ "$mode" = same ]; then
    CHECKS=$((CHECKS + 1))
  else
    note_record "$name"
  fi
  if [ "$zz_value" = "$pin_value" ]; then
    LAST_DIFFERED=0
    printf 'ok    %s: both %s\n' "$name" "$(printf '%s' "$zz_value" | cat -v)"
    return 0
  fi
  LAST_DIFFERED=1
  if [ "$mode" = same ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s\n' "$name"
  else
    printf 'note  %s recorded, not asserted: %s\n' "$name" "$RECORD_REASON"
  fi
  printf '        tmux: %s\n' "$(printf '%s' "$pin_value" | cat -v)"
  printf '        zz:   %s\n' "$(printf '%s' "$zz_value" | cat -v)"
  return 0
}
RECORD_REASON=""
record_value() {
  RECORD_REASON="$2"
  compare_value record "$1" "$3" "$4"
  RECORD_REASON=""
}
assert_screen() {
  local name="$1" zz_rows tmux_rows index count differing
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  CHECKS=$((CHECKS + 1))
  count=0
  differing=-1
  for ((index = 0; index < ROWS_UNDER_TEST; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      [ "$differing" -ge 0 ] || differing="$index"
      count=$((count + 1))
    fi
  done
  if [ "$count" -eq 0 ]; then
    LAST_DIFFERED=0
    printf 'ok    %s: all %s rows identical\n' "$name" "$ROWS_UNDER_TEST"
    return 0
  fi
  LAST_DIFFERED=1
  FAILURES=$((FAILURES + 1))
  printf 'DIFF  %s: %s of %s rows differ, first %s\n' \
    "$name" "$count" "$ROWS_UNDER_TEST" "$differing"
  for ((index = 0; index < ROWS_UNDER_TEST; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      printf '        row %s tmux: %s\n' "$index" "$(printf '%s' "${tmux_rows[index]-}" | cat -v)"
      printf '        row %s zz:   %s\n' "$index" "$(printf '%s' "${zz_rows[index]-}" | cat -v)"
    fi
  done
  return 0
}
check_screen() {
  local prefix="$1" mode_name="${1}_MODE" reason_name="${1}_REASON"
  if [ "${!mode_name}" = same ]; then
    assert_screen "$2"
  else
    record_screen "$2" "${!reason_name}"
  fi
}
record_screen() {
  local name="$1" reason="$2" zz_rows tmux_rows index count
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  note_record "$name"
  count=0
  for ((index = 0; index < ROWS_UNDER_TEST; index++)); do
    [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ] && count=$((count + 1))
  done
  if [ "$count" -eq 0 ]; then
    LAST_DIFFERED=0
    printf 'ok    %s: all %s rows identical\n' "$name" "$ROWS_UNDER_TEST"
    return 0
  fi
  LAST_DIFFERED=1
  printf 'note  %s recorded, not asserted: %s\n' "$name" "$reason"
  for ((index = 0; index < ROWS_UNDER_TEST; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      printf '        row %s tmux: %s\n' "$index" "$(printf '%s' "${tmux_rows[index]-}" | cat -v)"
      printf '        row %s zz:   %s\n' "$index" "$(printf '%s' "${zz_rows[index]-}" | cat -v)"
    fi
  done
  return 0
}

write_attach() {
  local side="$1" destination="$2"
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME -u XDG_RUNTIME_DIR HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp %q --socket %q attach-session -t %q\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" "=$INNER_SESSION" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME -u XDG_RUNTIME_DIR HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp %q -L %q attach-session -t %q\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" "=$INNER_SESSION" >>"$destination"
  fi
  chmod +x "$destination"
}
set_on_both() {
  side_command zz set-option -g "$1" "$2" >/dev/null || die "zz refused set-option -g $1"
  side_command tmux set-option -g "$1" "$2" >/dev/null || die "tmux refused set-option -g $1"
}
run_on_both() {
  side_command zz "$@" >/dev/null || die "zz refused $1"
  side_command tmux "$@" >/dev/null || die "tmux refused $1"
}
send_both() {
  local side pane
  for side in zz tmux; do
    pane="$(side_command "$side" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_id}' |
      awk '$1 == 1 { print $2; exit }')"
    [ -n "$pane" ] || die "$side has no active pane"
    side_command "$side" send-keys -t "$pane" "$1" Enter >/dev/null || die "$side refused send-keys"
  done
}
send_to_pane_both() {
  local target="$1"
  shift
  run_on_both send-keys -t "$target" "$@"
}
mark_both() {
  local name="$1"
  send_both "printf '\\033[H\\033[2JMARK-%s\\n' $name"
  settle_both "MARK-$name" "$name"
}
option_on_both() {
  wait_for "$3 on zz" option_is zz "$1" "$2"
  wait_for "$3 on tmux" option_is tmux "$1" "$2"
}
option_is() {
  [ "$(side_command "$1" show-options -gqv "$2" 2>/dev/null)" = "$3" ]
}
option_value() {
  side_command "$1" show-options -gqv "$2" 2>/dev/null
}

start_both() {
  write_attach zz "$SCRATCH_DIR/attach-zz.sh"
  write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"
  zz_command new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" \
    -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" "$INNER_SHELL" ||
    die "could not create the zz session"
  tmux_inner_command -f /dev/null new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" \
    -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" "$INNER_SHELL" ||
    die "could not create the tmux session"
  set_on_both status-right ''
  set_on_both status-left L
  set_on_both automatic-rename off
  set_on_both display-time "$MESSAGE_HOLD_MS"
  set_on_both default-shell /bin/sh
  set_on_both mouse on
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" "$SCRATCH_DIR/attach-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off >/dev/null
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/attach-tmux.sh" >/dev/null
  wait_for 'outer zz pane' outer_pane_is "=$OUTER_SESSION:zz" "${COLUMNS_UNDER_TEST}x${ROWS_UNDER_TEST}"
  wait_for 'outer tmux pane' outer_pane_is "=$OUTER_SESSION:tmux" "${COLUMNS_UNDER_TEST}x${ROWS_UNDER_TEST}"
  wait_for 'zz client attached' client_attached zz
  wait_for 'tmux client attached' client_attached tmux
  run_on_both rename-window -t "=$INNER_SESSION:0" "$WINDOW_NAME"
}

split_both() {
  run_on_both split-window -h -t "=$INNER_SESSION:0.0" "$INNER_SHELL"
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  wait_for 'zz pane 0 active' active_pane_index_is zz 0
  wait_for 'tmux pane 0 active' active_pane_index_is tmux 0
}
unsplit_both() {
  run_on_both kill-pane -t "=$INNER_SESSION:0.1"
  wait_for 'zz back to one pane' pane_count_is zz 1
  wait_for 'tmux back to one pane' pane_count_is tmux 1
}
pane_count_is() {
  [ "$(side_command "$1" list-panes -t "=$INNER_SESSION" -F x 2>/dev/null | wc -l)" = "$2" ]
}
leave_copy_mode_both() {
  local side
  for side in zz tmux; do
    side_command "$side" send-keys -X -t "=$INNER_SESSION:0.0" cancel >/dev/null 2>&1 || true
  done
  wait_for 'the pin out of copy mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 0
  wait_for 'zz out of copy mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 0
}

respawn_program_both() {
  run_on_both respawn-pane -k -t "=$INNER_SESSION:0.0" "$@"
}
respawn_shell_both() {
  respawn_program_both sh -c "$INNER_SHELL"
  settle_both '$' 'the shell back in pane 0'
}
program_output() {
  local side="$1" marker="$2"
  capture_plain "$side" | sed -n "/$marker/,\$p" | sed "1d" | sed '/^[[:space:]]*$/d' |
    head -n 4 | cat -v | tr '\n' '|'
}

case_click_selects_pane() {
  CASE_LABEL=click-selects-pane
  split_both
  mark_both click
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.1" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.1" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 3))" "$((top + 3))"
  wait_for 'the pin selected pane 1' active_pane_index_is tmux 1
  settle_both MARK-click 'the click on pane 1'
  assert_value click-selects-pane/active-pane \
    "$(active_pane_index zz)" "$(active_pane_index tmux)"
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  unsplit_both
}

case_click_user_binding() {
  CASE_LABEL=click-user-binding
  run_on_both set-option -gu @mousekey
  local side
  for side in zz tmux; do
    side_command "$side" bind-key -n MouseDown1Pane set-option -g @mousekey \
      "$(binding_value_for "$side")" >/dev/null || die "$side refused bind-key"
  done
  mark_both userbind
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 4))" "$((top + 2))"
  wait_for 'the pin ran its own mouse binding' option_is tmux @mousekey \
    "$(binding_value_for tmux)"
  settle_both MARK-userbind 'the user binding click'
  check_value USER_BINDING click-user-binding/option \
    "$(option_value zz @mousekey)" "$(option_value tmux @mousekey)"
  run_on_both unbind-key -n MouseDown1Pane
  run_on_both set-option -gu @mousekey
}

outlast_the_pin_click_timeout() {
  sleep 0.35
}
case_click_user_binding_target() {
  CASE_LABEL=click-user-binding-target
  run_on_both set-option -gu @mousectx
  run_on_both bind-key -n MouseDown1Pane set-option -gF @mousectx \
    '#{mouse_x},#{mouse_y},#{mouse_pane},#{mouse_status_line}'
  mark_both userctx
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 7))" "$((top + 5))"
  wait_for 'the pin published the mouse context' pin_option_set @mousectx
  settle_both MARK-userctx 'the mouse context click'
  check_value MOUSE_CONTEXT click-user-binding-target/context \
    "$(option_value zz @mousectx)" "$(option_value tmux @mousectx)"
  run_on_both unbind-key -n MouseDown1Pane
  run_on_both set-option -gu @mousectx
}
case_click_mouse_target() {
  CASE_LABEL=click-mouse-target
  split_both
  local spelling left top
  for spelling in '=' '{mouse}'; do
    run_on_both set-option -pu -t "=$INNER_SESSION:0.0" @mousetgt
    run_on_both set-option -pu -t "=$INNER_SESSION:0.1" @mousetgt
    run_on_both bind-key -n MouseDown1Pane set-option -p -t "$spelling" @mousetgt clicked
    mark_both mousetgt
    left="$(pane_field tmux "=$INNER_SESSION:0.1" 1)"
    top="$(pane_field tmux "=$INNER_SESSION:0.1" 2)"
    outlast_the_pin_click_timeout
    click_both 0 "$((left + 3))" "$((top + 3))"
    wait_for "the pin resolved $spelling to the clicked pane" \
      pin_pane_option_set "=$INNER_SESSION:0.1" @mousetgt
    settle_both MARK-mousetgt "the $spelling target click"
    assert_value "click-mouse-target/$spelling-clicked-pane" \
      "$(pane_option_value zz "=$INNER_SESSION:0.1" @mousetgt)" \
      "$(pane_option_value tmux "=$INNER_SESSION:0.1" @mousetgt)"
    assert_value "click-mouse-target/$spelling-other-pane" \
      "$(pane_option_value zz "=$INNER_SESSION:0.0" @mousetgt)" \
      "$(pane_option_value tmux "=$INNER_SESSION:0.0" @mousetgt)"
    run_on_both unbind-key -n MouseDown1Pane
    run_on_both select-pane -t "=$INNER_SESSION:0.0"
    wait_for 'the pin back on pane 0' active_pane_index_is tmux 0
  done
  run_on_both set-option -pu -t "=$INNER_SESSION:0.0" @mousetgt
  run_on_both set-option -pu -t "=$INNER_SESSION:0.1" @mousetgt
  unsplit_both
}
pane_option_value() {
  side_command "$1" show-options -pqv -t "$2" "$3" 2>/dev/null
}
pin_pane_option_set() {
  [ -n "$(pane_option_value tmux "$1" "$2")" ]
}

BINDING_VALUE_ZZ=""
BINDING_VALUE_TMUX=""
binding_value_for() {
  local override="BINDING_VALUE_${1^^}"
  printf '%s' "${!override:-fired}"
}
pin_option_set() {
  [ -n "$(option_value tmux "$1")" ]
}

case_border_user_binding() {
  CASE_LABEL=border-user-binding
  local right top
  split_both
  run_on_both set-option -gu @borderkey
  local side
  for side in zz tmux; do
    side_command "$side" bind-key -n MouseDown1Border set-option -g @borderkey \
      "$(binding_value_for "$side")" >/dev/null || die "$side refused bind-key"
  done
  mark_both borderbind
  right="$(pane_field tmux "=$INNER_SESSION:0.0" 3)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((right + 2))" "$((top + 4))"
  wait_for 'the pin ran its own border binding' option_is tmux @borderkey \
    "$(binding_value_for tmux)"
  settle_both MARK-borderbind 'the border click'
  assert_value border-user-binding/option \
    "$(option_value zz @borderkey)" "$(option_value tmux @borderkey)"
  run_on_both unbind-key -n MouseDown1Border
  run_on_both set-option -gu @borderkey
  unsplit_both
}

case_status_user_binding() {
  CASE_LABEL=status-user-binding
  run_on_both new-window -d -t "=$INNER_SESSION:" -n second "$INNER_SHELL"
  run_on_both select-window -t "=$INNER_SESSION:0"
  run_on_both set-option -gu @statuskey
  run_on_both bind-key -n WheelDownStatus set-option -g @statuskey fired
  mark_both statusbind
  local column row
  row="$(status_row)"
  column="$(status_column_of second)"
  [ -n "$column" ] || die 'the second window is not on the status row'
  send_mouse_both 65 "$column" "$row" M
  wait_for 'the pin ran its own status binding' option_is tmux @statuskey fired
  settle_both 'second' 'the status wheel binding'
  assert_value status-user-binding/option \
    "$(option_value zz @statuskey)" "$(option_value tmux @statuskey)"
  run_on_both unbind-key -n WheelDownStatus
  run_on_both set-option -gu @statuskey
  run_on_both kill-window -t "=$INNER_SESSION:1"
  wait_for 'the pin back to one window' pin_window_count_is 1
}

case_wheel_up_pane() {
  CASE_LABEL=wheel-up-pane
  mark_both wheel
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_mouse_both 64 "$((left + 4))" "$((top + 4))" M
  wait_for 'the pin entered copy mode on a wheel' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 1
  settle_both MARK-wheel 'the wheel over the pane'
  check_value WHEEL wheel-up-pane/pane-in-mode \
    "$(pane_in_mode zz "=$INNER_SESSION:0.0")" "$(pane_in_mode tmux "=$INNER_SESSION:0.0")"
  check_value WHEEL wheel-up-pane/pane-mode \
    "$(side_command zz display-message -p -t "=$INNER_SESSION:0.0" '#{pane_mode}')" \
    "$(side_command tmux display-message -p -t "=$INNER_SESSION:0.0" '#{pane_mode}')"
  leave_copy_mode_both
}

case_drag_selects() {
  CASE_LABEL=drag-selects
  respawn_program_both sh -c "printf 'DRAGLINE alpha beta gamma\n'; exec sleep 600"
  settle_both 'gamma' 'the drag sample'
  delete_buffer_both
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_mouse_both 0 "$((left + 1))" "$((top + 1))" M
  send_mouse_both 32 "$((left + 8))" "$((top + 1))" M
  wait_for 'the pin entered copy mode on a drag' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 1
  send_mouse_both 0 "$((left + 8))" "$((top + 1))" m
  wait_for 'the pin ended its drag' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 0
  settle_both 'gamma' 'the drag released'
  check_value DRAG drag-selects/buffer "$(buffer_sample zz)" "$(buffer_sample tmux)"
  check_value DRAG drag-selects/pane-in-mode \
    "$(pane_in_mode zz "=$INNER_SESSION:0.0")" "$(pane_in_mode tmux "=$INNER_SESSION:0.0")"
  check_value DRAG drag-selects/selection \
    "$(side_command zz display-message -p -t "=$INNER_SESSION:0.0" '#{selection_present}')" \
    "$(side_command tmux display-message -p -t "=$INNER_SESSION:0.0" '#{selection_present}')"
  delete_buffer_both
  leave_copy_mode_both
  respawn_shell_both
}

case_multi_click() {
  CASE_LABEL=multi-click
  respawn_program_both sh -c "printf 'MULTI alpha beta gamma\n'; exec sleep 600"
  settle_both 'gamma' 'the multi-click sample'
  delete_buffer_both
  local left top column
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  column="$((left + 8))"
  outlast_the_pin_click_timeout
  click_both 0 "$column" "$((top + 1))"
  click_both 0 "$column" "$((top + 1))"
  wait_for 'the pin copied a word' pin_buffer_has alpha
  settle_both 'gamma' 'the double click'
  check_value MULTI_CLICK multi-click/double-buffer \
    "$(buffer_sample zz)" "$(buffer_sample tmux)"
  delete_buffer_both
  outlast_the_pin_click_timeout
  click_both 0 "$column" "$((top + 1))"
  click_both 0 "$column" "$((top + 1))"
  click_both 0 "$column" "$((top + 1))"
  wait_for 'the pin copied a line' pin_buffer_has gamma
  settle_both 'gamma' 'the triple click'
  check_value MULTI_CLICK multi-click/triple-buffer \
    "$(buffer_sample zz)" "$(buffer_sample tmux)"
  delete_buffer_both
  respawn_shell_both
}
delete_buffer_both() {
  local side
  for side in zz tmux; do
    side_command "$side" delete-buffer >/dev/null 2>&1 || true
  done
}
buffer_sample() {
  side_command "$1" show-buffer 2>/dev/null | head -n 1 | cat -v
}
pin_buffer_has() {
  case "$(buffer_sample tmux)" in
  *"$1"*) return 0 ;;
  *) return 1 ;;
  esac
}

case_right_click_pane() {
  CASE_LABEL=right-click-pane
  mark_both rightclick
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_mouse_both 2 "$((left + 6))" "$((top + 4))" M
  both_screen_has 'Kill' 'the pane menu'
  settle_both Kill 'the right click'
  check_screen RIGHT_CLICK right-click-pane/screen
  send_bytes zz $'\033'
  send_bytes tmux $'\033'
  both_screen_lacks 'Kill' 'the pane menu closed'
  send_mouse_both 2 "$((left + 6))" "$((top + 4))" m
  respawn_shell_both
}

CONTEXT_LONG_WORD="WRAPPEDHEAD$(printf 'x%.0s' $(seq 1 120))LONGTAIL"
context_sample_both() {
  local link=""
  [ "${1-}" = with-link ] &&
    link="printf '\033]8;;https://example.com/page\033\\\\LINKTEXT\033]8;;\033\\\\\n'; "
  respawn_program_both sh -c "printf 'MOUSECTX alpha beta gamma\n'; printf '%s\n' '$CONTEXT_LONG_WORD'; ${link}printf 'MOUSECTXEND\n'; exec sleep 600"
  settle_both MOUSECTXEND 'the mouse context sample'
}

case_right_click_pane_over_a_word() {
  CASE_LABEL=right-click-pane-over-a-word
  context_sample_both
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_mouse_both 2 "$((left + 16))" "$((top + 1))" M
  both_screen_has 'Copy Line' 'the pane menu over a word'
  settle_both 'Copy Line' 'the right click over a word'
  check_screen CONTEXT_MENU right-click-pane/over-a-word
  send_bytes zz $'\033'
  send_bytes tmux $'\033'
  both_screen_lacks 'Copy Line' 'the pane menu over a word closed'
  send_mouse_both 2 "$((left + 16))" "$((top + 1))" m
  respawn_shell_both
}

CONTEXT_SABOTAGE_CHANNEL=""
CONTEXT_SABOTAGE_COLUMN=""
CONTEXT_SABOTAGE_ROW=""
context_probe() {
  local name="$1" column="$2" row="$3" side aimed aimed_row
  reset_click_sequence_both "$column" "$row"
  run_on_both set-option -gu @mousectx
  for side in zz tmux; do
    aimed="$column"
    aimed_row="$row"
    if [ "$side" = zz ] && [ "$name" = "$CONTEXT_SABOTAGE_CHANNEL" ]; then
      aimed="$CONTEXT_SABOTAGE_COLUMN"
      aimed_row="$CONTEXT_SABOTAGE_ROW"
    fi
    send_mouse "$side" 18 "$aimed" "$aimed_row" M
    send_mouse "$side" 18 "$aimed" "$aimed_row" m
  done
  wait_for "the pin published $name" pin_option_set @mousectx
  if [ "$name" != blank-cell ]; then
    wait_for "the pin read the $name cell" pin_option_answered @mousectx
  fi
  wait_at_most option_nonempty zz @mousectx || true
  settle_both MOUSECTXEND "the $name probe"
  check_value CONTEXT_FORMAT "mouse-context/$name" \
    "$(option_value zz @mousectx)" "$(option_value tmux @mousectx)"
}
option_nonempty() {
  [ -n "$(option_value "$1" "$2")" ]
}
pin_option_answered() {
  [ "$(option_value tmux "$1")" != '[][][]' ]
}
case_mouse_context_formats() {
  CASE_LABEL=mouse-context
  context_sample_both with-link
  run_on_both set-option -gu @mousectx
  run_on_both bind-key -n C-MouseDown3Pane set-option -gF @mousectx \
    '[#{mouse_word}][#{mouse_line}][#{mouse_hyperlink}]'
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  context_probe over-a-word "$((left + 16))" "$((top + 1))"
  context_probe wrapped-head "$((left + 3))" "$((top + 2))"
  context_probe wrapped-tail "$((left + 5))" "$((top + 3))"
  context_probe hyperlink "$((left + 4))" "$((top + 4))"
  context_probe blank-cell "$((left + 11))" "$((top + 7))"
  run_on_both unbind-key -n C-MouseDown3Pane
  run_on_both set-option -gu @mousectx
  respawn_shell_both
}

case_border_drag() {
  CASE_LABEL=border-drag
  split_both
  mark_both border
  local right top border_column before_width
  before_width="$(pane_field tmux "=$INNER_SESSION:0.0" 5)"
  right="$(pane_field tmux "=$INNER_SESSION:0.0" 3)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  border_column="$((right + 2))"
  local drag_column=$((border_column - 8)) zz_drag_column
  zz_drag_column="${BORDER_SABOTAGE_COLUMN:-$drag_column}"
  send_mouse_both 0 "$border_column" "$((top + 4))" M
  send_mouse zz 32 "$zz_drag_column" "$((top + 4))" M
  send_mouse tmux 32 "$drag_column" "$((top + 4))" M
  send_mouse zz 0 "$zz_drag_column" "$((top + 4))" m
  send_mouse tmux 0 "$drag_column" "$((top + 4))" m
  wait_for 'the pin resized on a border drag' pin_pane_width_changed "$before_width"
  settle_both MARK-border 'the border drag'
  check_value BORDER border-drag/pane-width \
    "$(pane_field zz "=$INNER_SESSION:0.0" 5)" "$(pane_field tmux "=$INNER_SESSION:0.0" 5)"
  unsplit_both
}
pin_pane_width_changed() {
  [ "$(pane_field tmux "=$INNER_SESSION:0.0" 5)" != "$1" ]
}

case_border_click() {
  CASE_LABEL=border-click
  split_both
  run_on_both select-pane -m -t "=$INNER_SESSION:0.1"
  wait_for 'the pin marked pane 1' pin_marked_set_is 1
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  wait_for 'the pin back on pane 0' active_pane_index_is tmux 0
  mark_both borderclick
  local right top
  right="$(pane_field tmux "=$INNER_SESSION:0.0" 3)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((right + 2))" "$((top + 4))"
  if pin_border_click_clears_mark; then
    wait_for 'the pin cleared its mark on a border click' pin_marked_set_is 0
    settle_both MARK-borderclick 'the border click'
    assert_value border-click/marked-set "$(marked_set zz)" "$(marked_set tmux)"
  else
    settle_both MARK-borderclick 'the border click'
    assert_value border-click/marked-set "$(marked_set zz)" "$(marked_set tmux)"
  fi
  assert_value border-click/active-pane \
    "$(active_pane_index zz)" "$(active_pane_index tmux)"
  unsplit_both
}
BORDER_CLICK_DRIFT='3.8 binds MouseDown1Border to select-pane -t= (key-bindings.c), so unbinding it on zz leaves the same marked and active panes as the pin'
pin_border_click_clears_mark() {
  side_command tmux list-keys -T root MouseDown1Border 2>/dev/null | grep -q 'select-pane -M'
}
marked_set() {
  side_command "$1" display-message -p -t "=$INNER_SESSION:0.0" \
    '#{pane_marked_set}' 2>/dev/null
}
pin_marked_set_is() {
  [ "$(marked_set tmux)" = "$1" ]
}

case_status_clicks() {
  CASE_LABEL=status-clicks
  run_on_both new-window -d -t "=$INNER_SESSION:" -n second "$INNER_SHELL"
  run_on_both select-window -t "=$INNER_SESSION:0"
  mark_both status
  assert_value status-clicks/row \
    "$(capture_plain zz | tail -n 1)" "$(capture_plain tmux | tail -n 1)"
  local column row
  row="$(status_row)"
  column="$(status_column_of second)"
  [ -n "$column" ] || die 'the second window is not on the status row'
  outlast_the_pin_click_timeout
  click_both 0 "$column" "$row"
  wait_for 'the pin switched window on a status click' pin_window_is 1
  settle_both 'second' 'the status click'
  check_value STATUS status-clicks/window-after-name-click \
    "$(current_window zz)" "$(current_window tmux)"
  run_on_both select-window -t "=$INNER_SESSION:0"
  wait_for 'the pin back on window 0' pin_window_is 0
  settle_both 'second' 'back on window 0'

  send_mouse_both 65 "$column" "$row" M
  wait_for 'the pin took the status wheel' pin_window_is 1
  settle_both 'second' 'the status wheel down'
  check_value STATUS status-clicks/window-after-wheel-down \
    "$(current_window zz)" "$(current_window tmux)"
  send_mouse_both 64 "$column" "$row" M
  wait_for 'the pin took the status wheel back' pin_window_is 0
  settle_both 'second' 'the status wheel up'
  check_value STATUS status-clicks/window-after-wheel-up \
    "$(current_window zz)" "$(current_window tmux)"

  status_window_menu 2 status-clicks/right-click-screen 'the window menu'
  status_window_menu 10 status-clicks/alt-right-click-screen 'the alt window menu'
  respawn_shell_both
  run_on_both kill-window -t "=$INNER_SESSION:1"
  wait_for 'the pin back to one window' pin_window_count_is 1
}
STATUS_MENU_DRIFT='NATIVE, gap:menus.client-owned: 3.8 menus belong to the window (ad6832e6), so the window menu a status click opens for a window the client is not showing is drawn on that window and the pin client shows nothing, while zz menus are client overlays and zz draws it over the current window'
pin_menus_follow_the_window() {
  ! side_command tmux list-commands display-panes 2>/dev/null | grep -q -- '-b'
}
status_window_menu() {
  local button="$1" name="$2" label="$3"
  send_mouse_both "$button" "$column" "$row" M
  wait_for "$label on the zz screen" screen_has zz 'Rename'
  if ! pin_menus_follow_the_window; then
    wait_for "$label on the tmux screen" screen_has tmux 'Rename'
    settle_both Rename "$label"
    check_screen STATUS_MENU "$name"
    send_bytes zz $'\033'
    send_bytes tmux $'\033'
    both_screen_lacks 'Rename' "$label closed"
  else
    wait_settled zz Rename "$label"
    record_screen "$name" "$STATUS_MENU_DRIFT"
    send_bytes zz $'\033'
    wait_for "$label gone from the zz screen" screen_lacks zz 'Rename'
  fi
  send_mouse_both "$button" "$column" "$row" m
}
status_column_of() {
  local row index
  row="$(capture_plain tmux | tail -n 1)"
  index="$(awk -v haystack="$row" -v needle="$1" 'BEGIN { print index(haystack, needle) }')"
  [ "$index" -gt 0 ] || return 1
  printf '%s\n' "$index"
}
current_window() {
  side_command "$1" display-message -p -t "=$INNER_SESSION:" '#{window_index}' 2>/dev/null
}
pin_window_is() {
  [ "$(current_window tmux)" = "$1" ]
}
pin_window_count_is() {
  [ "$(side_command tmux list-windows -t "=$INNER_SESSION" -F x | wc -l)" = "$1" ]
}

case_app_mouse() {
  local label="$1" mouse="$2"
  CASE_LABEL="app-mouse-$label"
  set_on_both mouse "$mouse"
  local side
  for side in zz tmux; do
    side_command "$side" respawn-pane -k -t "=$INNER_SESSION:0.0" sh -c \
      "stty -echo -icanon min 1 time 0; printf 'APPMOUSE\n\033[?1000h$(app_mouse_extra "$side")'; exec cat -v" >/dev/null ||
      die "$side refused respawn-pane"
  done
  settle_both APPMOUSE "the mouse program for $label"
  local left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 5))" "$((top + 3))"
  wait_for "the pin's program saw the click for $label" program_saw tmux APPMOUSE 'M'
  settle_both APPMOUSE "the click for $label"
  assert_value "app-mouse-$label/report" \
    "$(program_output zz APPMOUSE)" "$(program_output tmux APPMOUSE)"
  respawn_shell_both
  set_on_both mouse on
}
SABOTAGE_ENCODING=""
app_mouse_extra() {
  [ "$SABOTAGE_ENCODING" = "$1" ] && return 0
  printf '%s' '\033[?1006h'
}
program_saw() {
  case "$(program_output "$1" "$2")" in
  *"$3"*) return 0 ;;
  *) return 1 ;;
  esac
}

arm_button_event_both() {
  local marker="$1" side
  for side in zz tmux; do
    side_command "$side" respawn-pane -k -t "=$INNER_SESSION:0.0" sh -c \
      "stty -echo -icanon min 1 time 0; printf '$marker\n\033[?1002h\033[?1006h'; exec cat -v" >/dev/null ||
      die "$side refused respawn-pane"
  done
  settle_both "$marker" "the button-event program behind $marker"
}
app_reports() {
  capture_plain "$1" | sed -n "/$2/,\$p" | sed "1d" | sed '/^[[:space:]]*$/d' |
    head -n 1 | cat -v
}
report_count() {
  app_reports "$1" "$2" | grep -o -- '\[<' | wc -l | tr -d ' '
}
program_saw_reports() {
  [ "$(report_count "$1" "$2")" -ge "$3" ]
}
reports_held() {
  local side="$1" marker="$2" want="$3" attempt held=0
  for ((attempt = 0; attempt < 200; attempt++)); do
    if [ "$(report_count "$side" "$marker")" = "$want" ]; then
      held=$((held + 1))
      [ "$held" -ge 12 ] && return 0
    else
      held=0
    fi
    sleep 0.05
  done
  dump_state "the $side report count holding at $want"
  die "the $side report count never held at $want"
}

case_app_mouse_drag() {
  CASE_LABEL=app-mouse-drag
  arm_button_event_both DRAGREPORT
  local left top row
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  row="$((top + 3))"
  send_mouse_both 0 "$((left + 2))" "$row" M
  send_mouse_both 32 "$((left + 8))" "$row" M
  send_mouse_both 0 "$((left + 8))" "$row" m
  wait_for "the pin's program saw the press, the motion and the release" \
    program_saw_reports tmux DRAGREPORT 3
  settle_both DRAGREPORT 'the drag under button-event tracking'
  assert_value app-mouse-drag/reports \
    "$(app_reports zz DRAGREPORT)" "$(app_reports tmux DRAGREPORT)"
  respawn_shell_both
}

case_app_mouse_double_click() {
  CASE_LABEL=app-mouse-double-click
  arm_button_event_both CLICKREPORT
  local left top row column
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  row="$((top + 3))"
  column="$((left + 5))"
  outlast_the_pin_click_timeout
  click_both 0 "$column" "$row"
  click_both 0 "$column" "$row"
  wait_for "the pin's program saw both clicks" program_saw_reports tmux CLICKREPORT 4
  reports_held tmux CLICKREPORT 4
  settle_both CLICKREPORT 'the double click under button-event tracking'
  assert_value app-mouse-double-click/reports \
    "$(app_reports zz CLICKREPORT)" "$(app_reports tmux CLICKREPORT)"
  respawn_shell_both
}

PASTE_BYTES=$'\033[200~pasted-text\033[201~'
PASTE_BYTES_ZZ=""
PASTE_BYTES_TMUX=""
paste_bytes_for() {
  local override="PASTE_BYTES_${1^^}"
  printf '%s' "${!override:-$PASTE_BYTES}"
}
case_paste_into_pane() {
  CASE_LABEL=paste-into-pane
  respawn_program_both sh -c "stty -echo -icanon min 1 time 0; printf 'PASTEPANE\n\033[?2004h'; exec cat -v"
  settle_both PASTEPANE 'the paste program'
  send_bytes zz "$(paste_bytes_for zz)"
  send_bytes tmux "$(paste_bytes_for tmux)"
  wait_for "the pin's program saw the paste" program_saw tmux PASTEPANE 'pasted-text'
  settle_both PASTEPANE 'the paste into the pane'
  assert_value paste-into-pane/received \
    "$(program_output zz PASTEPANE)" "$(program_output tmux PASTEPANE)"
  respawn_shell_both
}
case_paste_into_copy_mode() {
  CASE_LABEL=paste-into-copy-mode
  respawn_shell_both
  run_on_both clear-history -t "=$INNER_SESSION:0.0"
  mark_both pastecopy
  run_on_both copy-mode -t "=$INNER_SESSION:0.0"
  wait_for 'the pin in copy mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 1
  wait_for 'zz in copy mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 1
  settle_both MARK-pastecopy 'copy mode before the paste'
  if [ -n "$COPY_PASTE_SABOTAGE_SIDE" ]; then
    side_command "$COPY_PASTE_SABOTAGE_SIDE" send-keys -X -t "=$INNER_SESSION:0.0" cancel \
      >/dev/null 2>&1 || true
    wait_for 'the sabotaged side out of copy mode' \
      pane_in_mode_is "$COPY_PASTE_SABOTAGE_SIDE" "=$INNER_SESSION:0.0" 0
    settle_both MARK-pastecopy 'copy mode left on one side before the paste'
  fi
  send_bytes zz "$(paste_bytes_for zz)"
  send_bytes tmux "$(paste_bytes_for tmux)"
  settle_both MARK-pastecopy 'the paste under copy mode'
  assert_value paste-into-copy-mode/pane-in-mode \
    "$(pane_in_mode zz "=$INNER_SESSION:0.0")" "$(pane_in_mode tmux "=$INNER_SESSION:0.0")"
  assert_screen paste-into-copy-mode/screen
  leave_copy_mode_both
  settle_both MARK-pastecopy 'copy mode left after the paste'
  check_screen PASTE_COPY paste-into-copy-mode/after-cancel-screen
  respawn_shell_both
}
case_paste_into_prompt() {
  CASE_LABEL=paste-into-prompt
  mark_both pasteprompt
  run_on_both bind-key -T prefix Q command-prompt -p 'PASTEPROMPT ' 'set-option -g @pasted "%%"'
  send_bytes zz $'\002'
  send_bytes tmux $'\002'
  send_bytes zz 'Q'
  send_bytes tmux 'Q'
  both_screen_has PASTEPROMPT 'the paste prompt'
  settle_both MARK-pasteprompt 'the paste prompt'
  send_bytes zz "$(paste_bytes_for zz)"
  send_bytes tmux "$(paste_bytes_for tmux)"
  settle_both MARK-pasteprompt 'the paste at the prompt'
  assert_value paste-into-prompt/row \
    "$(capture_plain zz | tail -n 1)" "$(capture_plain tmux | tail -n 1)"
  send_bytes zz $'\033'
  send_bytes tmux $'\033'
  both_screen_lacks PASTEPROMPT 'the paste prompt cancelled'
  run_on_both unbind-key -T prefix Q
}
case_paste_under_menu() {
  CASE_LABEL=paste-under-menu
  respawn_program_both sh -c "printf 'MENUPROG\n'; stty -echo -icanon min 1 time 0; exec cat"
  both_screen_has MENUPROG 'the paste program'
  run_on_both set-option -gu @menupick
  run_on_both bind-key -T prefix E display-menu -x 4 -y 8 -T PASTEMENU \
    'Alpha item' a 'set-option -g @menupick alpha' \
    'Beta item' b 'set-option -g @menupick beta'
  send_bytes zz $'\002'
  send_bytes tmux $'\002'
  send_bytes zz 'E'
  send_bytes tmux 'E'
  both_screen_has PASTEMENU 'the paste menu'
  settle_pointer
  send_bytes zz "$(paste_bytes_for zz)"
  send_bytes tmux "$(paste_bytes_for tmux)"
  settle_pointer
  check_screen PASTE_MENU paste-under-menu/screen
  assert_value paste-under-menu/option \
    "$(option_value zz @menupick)" "$(option_value tmux @menupick)"
  send_bytes zz $'\033'
  send_bytes tmux $'\033'
  both_screen_lacks PASTEMENU 'the paste menu closed'
  run_on_both unbind-key -T prefix E
  run_on_both set-option -gu @menupick
  respawn_shell_both
  settle_both '$' 'the shell after the paste menu'
}

FOCUS_BYTES=$'\033[O\033[I'
FOCUS_BYTES_ZZ=""
FOCUS_BYTES_TMUX=""
focus_bytes_for() {
  local override="FOCUS_BYTES_${1^^}"
  printf '%s' "${!override:-$FOCUS_BYTES}"
}
case_focus() {
  local label="$1" events="$2"
  CASE_LABEL="focus-$label"
  set_on_both focus-events "$events"
  respawn_program_both sh -c "stty -echo -icanon min 1 time 0; printf 'FOCUSPROG\n\033[?1004h'; exec cat -v"
  settle_both FOCUSPROG "the focus program with focus-events $events"
  send_bytes zz "$(focus_bytes_for zz)"
  send_bytes tmux "$(focus_bytes_for tmux)"
  if [ "$events" = on ]; then
    wait_for "the pin's program saw a focus report" program_saw tmux FOCUSPROG '['
  fi
  settle_both FOCUSPROG "the focus reports with focus-events $events"
  if [ "$events" = on ]; then
    assert_value "focus-$label/received" \
      "$(program_output zz FOCUSPROG)" "$(program_output tmux FOCUSPROG)"
  else
    check_value FOCUS_OFF "focus-$label/received" \
      "$(program_output zz FOCUSPROG)" "$(program_output tmux FOCUSPROG)"
  fi
  respawn_shell_both
  set_on_both focus-events off
}

MODE_POINTER_SETTLE_POLLS=8
mode_screens_agree() {
  [ "$(capture_screen zz)" = "$(capture_screen tmux)" ]
}
settle_still() {
  local side="$1" previous="" current attempt stable=0
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_screen "$side" 2>/dev/null || true)"
    if [ "$current" = "$previous" ]; then
      stable=$((stable + 1))
      [ "$stable" -ge 4 ] && return 0
    else
      stable=0
    fi
    previous="$current"
    sleep 0.05
  done
  return 0
}
settle_pointer() {
  local attempt
  for ((attempt = 0; attempt < 40; attempt++)); do
    mode_screens_agree && break
    sleep 0.05
  done
  settle_still zz
  settle_still tmux
}
mode_is_open_both() {
  wait_for 'the pin in a pane mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 1
  wait_for 'zz in a pane mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 1
}
mode_leave_both() {
  local side
  for side in zz tmux; do
    side_command "$side" send-keys -t "=$INNER_SESSION:0.0" Escape >/dev/null 2>&1 || true
  done
  wait_for 'the pin left its pane mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 0
  wait_for 'zz left its pane mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 0
}
mode_pointer_scene() {
  run_on_both set-option -g history-limit 2000
  run_on_both set-option -t "$INNER_SESSION" default-size \
    "${COLUMNS_UNDER_TEST}x${ROWS_UNDER_TEST}"
}
customize_open_both() {
  run_on_both customize-mode -t "=$INNER_SESSION:0.0"
  mode_is_open_both
  settle_pointer
}

case_customize_mouse_click() {
  CASE_LABEL=customize-mouse-click
  local left top
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 3))" "$((top + 6))"
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-click/screen
  mode_leave_both
}

case_customize_mouse_double_click() {
  CASE_LABEL=customize-mouse-double-click
  local left top
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_to_pane_both "=$INNER_SESSION:0.0" Right
  settle_pointer
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" M
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" m
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" M
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" m
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-double-click/screen
  send_to_pane_both "=$INNER_SESSION:0.0" Escape
  settle_pointer
  mode_leave_both
}

case_customize_mouse_quiet() {
  CASE_LABEL=customize-mouse-quiet
  local left top
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_wheel_both 64 "$((left + 3))" "$((top + 4))" M
  send_wheel_both 65 "$((left + 3))" "$((top + 4))" M
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-wheel/screen
  check_value MODE_POINTER customize-mouse-wheel/pane-mode \
    "$(side_command zz display-message -p -t "=$INNER_SESSION:0.0" '#{pane_in_mode}/#{pane_mode}')" \
    "$(side_command tmux display-message -p -t "=$INNER_SESSION:0.0" '#{pane_in_mode}/#{pane_mode}')"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 3))" "$((top + 18))"
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-preview/screen
  mode_leave_both
}

case_customize_mouse_prompt() {
  CASE_LABEL=customize-mouse-prompt
  local left top
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  send_to_pane_both "=$INNER_SESSION:0.0" f
  settle_pointer
  send_to_pane_both "=$INNER_SESSION:0.0" a b c d e f
  settle_pointer
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 12))" "$((top + ROWS_UNDER_TEST - 1))"
  settle_pointer
  send_to_pane_both "=$INNER_SESSION:0.0" Z
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-prompt/screen
  send_to_pane_both "=$INNER_SESSION:0.0" Escape
  settle_pointer
  mode_leave_both
}

case_switch_mouse() {
  CASE_LABEL=switch-mouse
  local left top
  run_on_both new-session -d -s pointed -n win "$INNER_SHELL"
  run_on_both switch-mode -t "=$INNER_SESSION:0.0"
  mode_is_open_both
  settle_pointer
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  outlast_the_pin_click_timeout
  click_both 0 "$((left + 3))" "$((top + 2))"
  settle_pointer
  check_screen MODE_POINTER switch-mouse-click/screen
  send_wheel_both 64 "$((left + 3))" "$((top + 2))" M
  settle_pointer
  check_screen MODE_POINTER switch-mouse-wheel-up/screen
  send_wheel_both 65 "$((left + 3))" "$((top + 2))" M
  settle_pointer
  check_screen MODE_POINTER switch-mouse-wheel-down/screen
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" M
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" m
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" M
  send_mouse_both 0 "$((left + 3))" "$((top + 2))" m
  wait_at_most client_session_is tmux pointed
  settle_pointer
  check_value MODE_POINTER switch-mouse-double-click/client-session \
    "$(client_session zz)" "$(client_session tmux)"
  run_on_both switch-client -t "=$INNER_SESSION"
  wait_for 'the pin back on its own session' client_session_is tmux "$INNER_SESSION"
  wait_for 'zz back on its own session' client_session_is zz "$INNER_SESSION"
  run_on_both kill-session -t pointed
  settle_pointer
}
client_session() {
  side_command "$1" list-clients -F '#{client_session}' 2>/dev/null | head -1
}
client_session_is() {
  [ "$(client_session "$1")" = "$2" ]
}
MENU_RELEASE_SABOTAGE_SIDE=""
MENU_DRAG_SABOTAGE=""
MENU_TAGALL_SABOTAGE=""
menu_press_both() {
  local button="$1" column="$2" row="$3" side aimed aimed_row
  for side in zz tmux; do
    aimed="$column"
    aimed_row="$row"
    if [ "$side" = "$CLICK_SABOTAGE_SIDE" ]; then
      [ -n "$CLICK_SABOTAGE_COLUMN" ] && aimed="$CLICK_SABOTAGE_COLUMN"
      [ -n "$CLICK_SABOTAGE_ROW" ] && aimed_row="$CLICK_SABOTAGE_ROW"
    fi
    send_mouse "$side" "$button" "$aimed" "$aimed_row" M
  done
}
menu_release_both() {
  local button="$1" column="$2" row="$3" side
  for side in zz tmux; do
    [ "$side" = "$MENU_RELEASE_SABOTAGE_SIDE" ] && continue
    send_mouse "$side" "$button" "$column" "$row" m
  done
}
menu_drag_both() {
  local press_column="$1" press_row="$2" column="$3" row="$4" side
  for side in zz tmux; do
    if [ "$side" = zz ] && [ -n "$MENU_DRAG_SABOTAGE" ]; then
      send_mouse "$side" 34 "$press_column" "$press_row" M
    else
      send_mouse "$side" 34 "$column" "$row" M
    fi
  done
}
menu_drag_release_both() {
  local press_column="$1" press_row="$2" column="$3" row="$4" side
  for side in zz tmux; do
    if [ "$side" = zz ] && [ -n "$MENU_DRAG_SABOTAGE" ]; then
      send_mouse "$side" 2 "$press_column" "$press_row" m
    else
      send_mouse "$side" 2 "$column" "$row" m
    fi
  done
}
case_customize_mouse_menu() {
  CASE_LABEL=customize-mouse-menu
  local left top column row
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  column="$((left + 4))"
  row="$((top + 3))"
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-open/screen
  menu_release_both 2 "$column" "$row"
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-release/screen
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
  CLICK_SABOTAGE_ROW=""
  MENU_RELEASE_SABOTAGE_SIDE=""
  send_mouse_both 2 "$column" "$row" m
  settle_pointer
  menu_press_both 2 "$column" "$((top + 1))"
  both_screen_has 'Tag All' 'the tree menu for Expand'
  settle_pointer
  send_bytes zz $'\033[C'
  send_bytes tmux $'\033[C'
  both_screen_lacks 'Tag All' 'the tree menu after Expand'
  settle_pointer
  check_screen MODE_POINTER customize-mouse-menu-expand/screen
  row="$((top + 2))"
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for Tag'
  settle_pointer
  menu_drag_both "$column" "$row" "$((column - 1))" "$((row + 5))"
  menu_drag_release_both "$column" "$row" "$((column - 1))" "$((row + 5))"
  both_screen_lacks 'Tag All' 'the tree menu after Tag'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-tag/screen
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for Tag None'
  settle_pointer
  send_bytes zz 'T'
  send_bytes tmux 'T'
  both_screen_lacks 'Tag All' 'the tree menu after Tag None'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-tag-none/screen
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for control T'
  settle_pointer
  send_bytes zz $'\024'
  send_bytes tmux $'\024'
  both_screen_has 'Tag All' 'the tree menu kept by control T'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-tag-ctrl/screen
  menu_release_both 2 "$column" "$row"
  settle_pointer
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for Tag All'
  settle_pointer
  if [ -n "$MENU_TAGALL_SABOTAGE" ]; then
    send_mouse zz 34 "$((column - 1))" "$((row + 5))" M
    send_mouse zz 2 "$((column - 1))" "$((row + 5))" m
  else
    send_mouse zz 34 "$((column - 1))" "$((row + 6))" M
    send_mouse zz 2 "$((column - 1))" "$((row + 6))" m
  fi
  send_mouse tmux 34 "$((column - 1))" "$((row + 6))" M
  send_mouse tmux 2 "$((column - 1))" "$((row + 6))" m
  both_screen_lacks 'Tag All' 'the tree menu after Tag All'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-tag-all/screen
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for Select'
  settle_pointer
  send_bytes zz $'\r'
  send_bytes tmux $'\r'
  both_screen_lacks 'Tag All' 'the tree menu after Select'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-select/screen
  send_to_pane_both "=$INNER_SESSION:0.0" Escape
  settle_pointer
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Tag All' 'the tree menu for Cancel'
  settle_pointer
  send_bytes zz 'q'
  send_bytes tmux 'q'
  wait_for 'the pin left its pane mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 0
  wait_for 'zz left its pane mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 0
  settle_pointer
  check_screen MODE_POINTER customize-mouse-menu-cancel/screen
}
case_customize_mouse_menu_outside() {
  CASE_LABEL=customize-mouse-menu-outside
  local left top column row
  customize_open_both
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  column="$((left + 4))"
  row="$((top + 19))"
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Scroll Left' 'the outside menu'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-outside-open/screen
  send_bytes zz '<'
  send_bytes tmux '<'
  both_screen_lacks 'Scroll Left' 'the outside menu after Scroll Left'
  settle_pointer
  check_screen CUSTOMIZE_POINTER customize-mouse-menu-outside-scroll/screen
  menu_press_both 2 "$column" "$row"
  both_screen_has 'Scroll Left' 'the outside menu for Cancel'
  settle_pointer
  send_bytes zz 'q'
  send_bytes tmux 'q'
  wait_for 'the pin left its pane mode' pane_in_mode_is tmux "=$INNER_SESSION:0.0" 0
  wait_for 'zz left its pane mode' pane_in_mode_is zz "=$INNER_SESSION:0.0" 0
  settle_pointer
  check_screen MODE_POINTER customize-mouse-menu-outside-cancel/screen
}

FLOAT_SABOTAGE_COLUMN=""
FLOAT_ROW_OFFSET=0
FLOAT_PREFIX=""
float_geometry() {
  side_command "$1" list-panes -t "=$INNER_SESSION" \
    -F '#{pane_floating_flag} #{pane_index} #{pane_width}x#{pane_height} #{pane_left},#{pane_top}' 2>/dev/null |
    awk '$1 == 1 { printf "%s:%s@%s;", $2, $3, $4 }'
}
float_active() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_index}:#{pane_floating_flag}' 2>/dev/null |
    awk '$1 == 1 { print $2; exit }'
}
float_count_is() {
  [ "$(side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_floating_flag}' 2>/dev/null | grep -c 1)" = "$2" ]
}
float_geometry_settled() {
  local first second
  first="$(float_geometry "$1")"
  sleep 0.2
  second="$(float_geometry "$1")"
  [ -n "$first" ] && [ "$first" = "$second" ]
}
float_gesture() {
  local press="$1" motion="$2" release_button="$3"
  shift 3
  local cells=("$@") index last side column
  last=$((${#cells[@]} - 2))
  for ((index = 1; index < ${#cells[@]}; index += 2)); do
    cells[index]=$((cells[index] + FLOAT_ROW_OFFSET))
  done
  for side in zz tmux; do
    send_mouse "$side" "$press" "${cells[0]}" "${cells[1]}" M
    for ((index = 2; index <= last; index += 2)); do
      column="${cells[index]}"
      if [ "$side" = zz ] && [ "$index" -eq "$last" ] && [ -n "$FLOAT_SABOTAGE_COLUMN" ]; then
        column=$((column + FLOAT_SABOTAGE_COLUMN))
      fi
      if [ "$index" -eq "$last" ]; then
        send_mouse "$side" 32 "$column" "${cells[index + 1]}" M
      else
        send_mouse "$side" "$motion" "$column" "${cells[index + 1]}" M
      fi
      sleep 0.1
    done
    send_mouse "$side" "$release_button" "$column" "${cells[last + 1]}" m
  done
}
float_verdict() {
  local name="$FLOAT_PREFIX$1" floats="$2"
  wait_for "the $name floats on zz" float_count_is zz "$floats"
  wait_for "the $name floats on tmux" float_count_is tmux "$floats"
  wait_for "the $name geometry settled on zz" float_geometry_settled zz
  wait_for "the $name geometry settled on tmux" float_geometry_settled tmux
  check_value FLOAT "$name/geometry" "$(float_geometry zz)" "$(float_geometry tmux)"
  check_value FLOAT "$name/active" "$(float_active zz)" "$(float_active tmux)"
}
kill_floats_both() {
  local side pane
  for side in zz tmux; do
    for pane in $(side_command "$side" list-panes -t "=$INNER_SESSION" \
      -F '#{pane_floating_flag} #{pane_id}' | awk '$1 == 1 { print $2 }'); do
      side_command "$side" kill-pane -t "$pane" >/dev/null 2>&1 || true
    done
  done
  wait_for 'zz floats gone' float_count_is zz 0
  wait_for 'tmux floats gone' float_count_is tmux 0
}
case_float_create_from_pane() {
  CASE_LABEL=float-create-pane
  mark_both floatcreate
  float_gesture 16 48 0 11 5 21 9 46 16 51 17
  float_verdict float-create-pane 1
  kill_floats_both
}
case_float_move() {
  CASE_LABEL=float-move
  mark_both floatmove
  run_on_both new-pane -d -x 30 -y 8 -X 10 -Y 3 "$INNER_SHELL"
  wait_for 'the zz float to move' float_count_is zz 1
  wait_for 'the tmux float to move' float_count_is tmux 1
  float_gesture 8 40 0 21 8 26 10 23 9 31 13
  float_verdict float-move 1
  kill_floats_both
}
case_float_border_resize() {
  CASE_LABEL=float-border-resize
  mark_both floatresize
  run_on_both new-pane -d -x 30 -y 8 -X 10 -Y 3 "$INNER_SHELL"
  wait_for 'the zz float to resize' float_count_is zz 1
  wait_for 'the tmux float to resize' float_count_is tmux 1
  float_gesture 0 32 0 40 7 46 7 51 9
  float_verdict float-border-resize 1
  float_gesture 0 32 0 51 11 56 14 58 16
  float_verdict float-corner-resize 1
  kill_floats_both
}
case_float_top_border_move() {
  CASE_LABEL=float-top-border-move
  mark_both floattop
  run_on_both new-pane -d -x 30 -y 8 -X 10 -Y 3 "$INNER_SHELL"
  wait_for 'the zz float to drag' float_count_is zz 1
  wait_for 'the tmux float to drag' float_count_is tmux 1
  float_gesture 0 32 0 21 4 25 7 31 9
  float_verdict float-top-border-move 1
  kill_floats_both
}
case_float_create_from_empty() {
  CASE_LABEL=float-create-empty
  run_on_both new-window -t "=$INNER_SESSION:1" -n empty "$INNER_SHELL"
  run_on_both new-pane -x 20 -y 6 -X 2 -Y 2 "$INNER_SHELL"
  local side tiled
  for side in zz tmux; do
    tiled="$(side_command "$side" list-panes -t "=$INNER_SESSION:1" -F '#{pane_floating_flag} #{pane_id}' |
      awk '$1 == 0 { print $2; exit }')"
    side_command "$side" kill-pane -t "$tiled" >/dev/null || die "$side refused kill-pane"
  done
  wait_for 'zz down to its float' pane_count_is zz 1
  wait_for 'tmux down to its float' pane_count_is tmux 1
  float_gesture 16 48 0 41 11 50 14 56 17 61 19
  float_verdict float-create-empty 2
  run_on_both kill-window -t "=$INNER_SESSION:1"
  wait_for 'zz back on window 0' pane_count_is zz 1
  wait_for 'tmux back on window 0' pane_count_is tmux 1
}
case_float_status_top() {
  set_on_both status-position top
  FLOAT_ROW_OFFSET=1
  FLOAT_PREFIX=status-top-
  case_float_create_from_pane
  case_float_top_border_move
  FLOAT_ROW_OFFSET=0
  FLOAT_PREFIX=""
  set_on_both status-position bottom
}
run_float_cases() {
  case_float_create_from_pane
  case_float_move
  case_float_border_resize
  case_float_top_border_move
  case_float_create_from_empty
  case_float_status_top
}

USER_BINDING_MODE=same
USER_BINDING_REASON=""
FLOAT_MODE=same
FLOAT_REASON=""
MOUSE_CONTEXT_MODE=same
MOUSE_CONTEXT_REASON=""
WHEEL_MODE=same
WHEEL_REASON=""
DRAG_MODE=same
DRAG_REASON=""
MULTI_CLICK_MODE=same
MULTI_CLICK_REASON=""
BORDER_MODE=same
BORDER_REASON=""
STATUS_MODE=same
STATUS_REASON=""
STATUS_MENU_MODE=same
STATUS_MENU_REASON=""
PASTE_MENU_MODE=same
PASTE_MENU_REASON=""
FOCUS_OFF_MODE=same
FOCUS_OFF_REASON=""
PASTE_COPY_MODE=same
PASTE_COPY_REASON=""
CONTEXT_MENU_MODE=same
CONTEXT_MENU_REASON=""
CONTEXT_FORMAT_MODE=same
CONTEXT_FORMAT_REASON=""
RIGHT_CLICK_MODE=same
RIGHT_CLICK_REASON=""
MODE_POINTER_MODE=same
MODE_POINTER_REASON=""
CUSTOMIZE_POINTER_MODE=same
CUSTOMIZE_POINTER_REASON=""

run_cases() {
  start_both
  case_click_selects_pane
  case_click_user_binding
  case_click_user_binding_target
  case_click_mouse_target
  case_wheel_up_pane
  case_drag_selects
  case_multi_click
  case_right_click_pane
  case_right_click_pane_over_a_word
  case_mouse_context_formats
  case_border_drag
  case_border_click
  run_float_cases
  case_status_clicks
  case_border_user_binding
  case_status_user_binding
  case_app_mouse mouse-on on
  case_app_mouse mouse-off off
  case_app_mouse_drag
  case_app_mouse_double_click
  case_paste_into_pane
  case_paste_into_copy_mode
  case_paste_into_prompt
  case_paste_under_menu
  case_focus events-on on
  case_focus events-off off
  mode_pointer_scene
  case_customize_mouse_click
  case_customize_mouse_double_click
  case_customize_mouse_quiet
  case_customize_mouse_prompt
  case_switch_mouse
  case_customize_mouse_menu
  case_customize_mouse_menu_outside

  printf '%s asserted checks, %s recorded checks\n' "$CHECKS" "$RECORDS"
  printf '%s recorded not asserted (%s)\n' "$RECORDS" "$(owner_tally)"
  if [ "$FAILURES" -ne 0 ]; then
    printf '%s of %s asserted checks differ\n' "$FAILURES" "$CHECKS"
    exit 1
  fi
  printf 'all %s asserted checks identical\n' "$CHECKS"
  exit 0
}

SELF_CHECK_FAILURES=0

customize_sabotage() {
  if [ "$CUSTOMIZE_POINTER_MODE" = same ]; then
    self_check_case "$1" catches "$2"
  else
    printf 'note  self-check %s skipped: %s\n' "$1" "$CUSTOMIZE_POINTER_REASON"
  fi
}
self_check_case() {
  local name="$1" expectation="$2" before="$FAILURES"
  shift 2
  "$@"
  local differed=$((FAILURES - before))
  FAILURES="$before"
  if [ "$expectation" = catches ] && [ "$differed" -gt 0 ]; then
    printf 'ok    self-check %s: caught\n' "$name"
    return 0
  fi
  if [ "$expectation" = quiet ] && [ "$differed" -eq 0 ]; then
    printf 'ok    self-check %s: no difference reported\n' "$name"
    return 0
  fi
  SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
  printf 'FAIL  self-check %s: %s asserted checks differed, expected %s\n' \
    "$name" "$differed" "$expectation"
}

sc_one_sided_binding_value() {
  BINDING_VALUE_ZZ=other
  case_click_user_binding
  BINDING_VALUE_ZZ=""
}
sc_one_sided_border_binding() {
  BINDING_VALUE_ZZ=other
  case_border_user_binding
  BINDING_VALUE_ZZ=""
}
sc_one_sided_click_target() {
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_COLUMN=3
  case_click_selects_pane
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
}
sc_one_sided_paste() {
  PASTE_BYTES_ZZ=$'\033[200~other-text\033[201~'
  case_paste_into_pane
  PASTE_BYTES_ZZ=""
}
sc_one_sided_mouse_encoding() {
  SABOTAGE_ENCODING=zz
  case_app_mouse sc-encoding on
  SABOTAGE_ENCODING=""
}
sc_one_sided_focus() {
  FOCUS_BYTES_ZZ=$'\033[O'
  case_focus sc-focus on
  FOCUS_BYTES_ZZ=""
}
sc_one_sided_status_left() {
  side_command zz set-option -g status-left LL >/dev/null
  case_status_clicks
  side_command zz set-option -g status-left L >/dev/null
}

sc_one_sided_mouse_context() {
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_COLUMN=$(($(pane_field tmux "=$INNER_SESSION:0.0" 1) + 9))
  case_click_user_binding_target
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
}
sc_one_sided_mouse_target() {
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_COLUMN=3
  case_click_mouse_target
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
}
sc_one_sided_border_drag() {
  local right
  right="$(pane_field tmux "=$INNER_SESSION:0.0" 3)"
  BORDER_SABOTAGE_COLUMN=$((right + 2 - 4))
  case_border_drag
  BORDER_SABOTAGE_COLUMN=""
}
sc_one_sided_float_drag() {
  FLOAT_SABOTAGE_COLUMN=3
  case_float_move
  FLOAT_SABOTAGE_COLUMN=""
}
sc_one_sided_status_wheel() {
  side_command zz unbind-key -T root WheelDownStatus >/dev/null 2>&1
  case_status_clicks
  side_command zz bind-key -T root WheelDownStatus next-window >/dev/null 2>&1
}
sc_one_sided_wheel_up_pane() {
  side_command zz unbind-key -T root WheelUpPane >/dev/null 2>&1
  case_wheel_up_pane
  side_command zz bind-key -T root WheelUpPane \
    'if-shell -F "#{||:#{alternate_on},#{pane_in_mode},#{mouse_any_flag}}" { send-keys -M } { copy-mode -e }' \
    >/dev/null 2>&1
}
sc_one_sided_drag_end() {
  side_command zz unbind-key -T copy-mode MouseDragEnd1Pane >/dev/null 2>&1
  case_drag_selects
  side_command zz bind-key -T copy-mode MouseDragEnd1Pane \
    send-keys -X copy-pipe-and-cancel >/dev/null 2>&1
}
sc_one_sided_double_click() {
  side_command zz unbind-key -T root DoubleClick1Pane >/dev/null 2>&1
  case_multi_click
  side_command zz bind-key -T root DoubleClick1Pane \
    'select-pane -t= ; if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" { send-keys -M } { copy-mode -H ; send-keys -X select-word ; run-shell -d 0.3 ; send-keys -X copy-pipe-and-cancel }' \
    >/dev/null 2>&1
}
sc_one_sided_menu_paste_tail() {
  PASTE_BYTES_ZZ=$'\033[200~pasted-text-and-more\033[201~'
  case_paste_under_menu
  PASTE_BYTES_ZZ=""
  respawn_shell_both
}
sc_one_sided_marked_pane() {
  side_command zz select-pane -m -t "=$INNER_SESSION:0.0" >/dev/null 2>&1
  case_right_click_pane
  side_command zz select-pane -M >/dev/null 2>&1
}
sc_one_sided_menu_context_word() {
  side_command zz set-option -g word-separators 'aeiou' >/dev/null
  case_right_click_pane_over_a_word
  side_command zz set-option -gu word-separators >/dev/null
}
sc_one_sided_context_channel() {
  local channel="$1" left top
  left="$(pane_field tmux "=$INNER_SESSION:0.0" 1)"
  top="$(pane_field tmux "=$INNER_SESSION:0.0" 2)"
  CONTEXT_SABOTAGE_CHANNEL="$channel"
  if [ "$channel" = over-a-word ]; then
    CONTEXT_SABOTAGE_COLUMN="$((left + 3))"
    CONTEXT_SABOTAGE_ROW="$((top + 2))"
  else
    CONTEXT_SABOTAGE_COLUMN="$((left + 16))"
    CONTEXT_SABOTAGE_ROW="$((top + 1))"
  fi
  case_mouse_context_formats
  CONTEXT_SABOTAGE_CHANNEL=""
  CONTEXT_SABOTAGE_COLUMN=""
  CONTEXT_SABOTAGE_ROW=""
}

WINDOW_MENU_ITEMS=(
  '#{?#{>:#{session_windows},1},,-}Swap Left' l '{ swap-window -t :-1 }'
  '#{?#{>:#{session_windows},1},,-}Swap Right' r '{ swap-window -t :+1 }'
  '#{?pane_marked_set,,-}Swap Marked' s '{ swap-window }'
  ''
  Kill X '{ kill-window }'
  Respawn R '{ respawn-window -k }'
  '#{?pane_marked,Unmark,Mark}' m '{ select-pane -m }'
  Rename n '{ command-prompt -F -I "#W" { rename-window -t "#{window_id}" "%%" } }'
  ''
  'New After' w '{ new-window -a }'
  'New At End' W '{ new-window }'
)
bind_window_menu_on_zz() {
  local key
  for key in MouseDown3Status M-MouseDown3Status; do
    side_command zz bind-key -T root "$key" display-menu -t = \
      -x "$1" -y "$2" -T '#[align=centre]#{window_index}:#{window_name}' \
      "${WINDOW_MENU_ITEMS[@]}" >/dev/null 2>&1
  done
}
sc_one_sided_status_menu_position() {
  bind_window_menu_on_zz C C
  case_status_clicks
  bind_window_menu_on_zz W W
}
sc_one_sided_border_click() {
  side_command zz unbind-key -T root MouseDown1Border >/dev/null 2>&1
  case_border_click
  side_command zz bind-key -T root MouseDown1Border select-pane -M >/dev/null 2>&1
}
sc_one_sided_drag_release() {
  side_command zz bind-key -T root MouseDragEnd1Pane set-option -g @claimed dragend >/dev/null
  case_app_mouse_drag
  side_command zz unbind-key -T root MouseDragEnd1Pane >/dev/null
  side_command zz set-option -gu @claimed >/dev/null 2>&1
}
sc_one_sided_second_click() {
  side_command zz bind-key -T root SecondClick1Pane set-option -g @claimed second >/dev/null
  case_app_mouse_double_click
  side_command zz unbind-key -T root SecondClick1Pane >/dev/null
  side_command zz set-option -gu @claimed >/dev/null 2>&1
}
sc_one_sided_copy_mode_paste() {
  COPY_PASTE_SABOTAGE_SIDE=zz
  case_paste_into_copy_mode
  COPY_PASTE_SABOTAGE_SIDE=""
}
sc_one_sided_focus_off() {
  FOCUS_BYTES_ZZ=$'\033[O'
  case_focus sc-focus-off off
  FOCUS_BYTES_ZZ=""
}

sc_one_sided_customize_click() {
  mode_pointer_scene
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_ROW=$(($(pane_field tmux "=$INNER_SESSION:0.0" 2) + 3))
  case_customize_mouse_click
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_ROW=""
}
sc_one_sided_switch_wheel() {
  mode_pointer_scene
  WHEEL_SABOTAGE_SIDE=zz
  case_switch_mouse
  WHEEL_SABOTAGE_SIDE=""
}
sc_one_sided_customize_menu_row() {
  mode_pointer_scene
  mode_leave_both
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_ROW=$(($(pane_field tmux "=$INNER_SESSION:0.0" 2) + 4))
  case_customize_mouse_menu
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_ROW=""
}
sc_one_sided_customize_menu_column() {
  mode_pointer_scene
  mode_leave_both
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_COLUMN=$(($(pane_field tmux "=$INNER_SESSION:0.0" 1) + 21))
  case_customize_mouse_menu
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
}
sc_one_sided_customize_menu_release() {
  mode_pointer_scene
  mode_leave_both
  MENU_RELEASE_SABOTAGE_SIDE=zz
  case_customize_mouse_menu
  MENU_RELEASE_SABOTAGE_SIDE=""
}
sc_one_sided_customize_menu_drag() {
  mode_pointer_scene
  mode_leave_both
  MENU_DRAG_SABOTAGE=1
  case_customize_mouse_menu
  MENU_DRAG_SABOTAGE=""
}
sc_one_sided_customize_menu_tagall() {
  mode_pointer_scene
  mode_leave_both
  MENU_TAGALL_SABOTAGE=1
  case_customize_mouse_menu
  MENU_TAGALL_SABOTAGE=""
}
sc_one_sided_customize_menu_outside_column() {
  mode_pointer_scene
  mode_leave_both
  CLICK_SABOTAGE_SIDE=zz
  CLICK_SABOTAGE_COLUMN=$(($(pane_field tmux "=$INNER_SESSION:0.0" 1) + 25))
  case_customize_mouse_menu_outside
  CLICK_SABOTAGE_SIDE=""
  CLICK_SABOTAGE_COLUMN=""
}

run_self_check() {
  start_both
  printf 'self-check: one deliberate one-sided difference per channel\n'
  PIN_D77_MOUSE_TABLE=1
  pin_border_click_clears_mark || PIN_D77_MOUSE_TABLE=0

  self_check_case 'control, a click with nothing sabotaged' quiet \
    case_click_selects_pane
  self_check_case 'control, the same paste on both sides' quiet \
    case_paste_into_pane
  self_check_case 'control, the same focus reports on both sides' quiet \
    case_focus sc-control on

  self_check_case 'a one-sided paste' catches sc_one_sided_paste
  self_check_case 'the SGR mouse extension dropped on zz only' catches \
    sc_one_sided_mouse_encoding
  self_check_case 'only the focus-out report sent to zz' catches sc_one_sided_focus
  self_check_case 'a status-left only zz carries' catches sc_one_sided_status_left
  self_check_case "zz's click aimed at the pane it already sits in" catches \
    sc_one_sided_click_target
  self_check_case 'the same root mouse binding set differently on zz' catches \
    sc_one_sided_binding_value
  if [ "$PIN_D77_MOUSE_TABLE" -eq 1 ]; then
    self_check_case 'MouseDown1Border unbound on zz only' catches \
      sc_one_sided_border_click
  else
    printf 'note  self-check MouseDown1Border unbound on zz only skipped: %s\n' "$BORDER_CLICK_DRIFT"
  fi
  self_check_case 'the border mouse binding set differently on zz' catches \
    sc_one_sided_border_binding
  self_check_case "zz's context click aimed one cell further along" catches \
    sc_one_sided_mouse_context
  self_check_case "zz's border drag released four cells short" catches \
    sc_one_sided_border_drag
  self_check_case "zz's float drag released three cells further" catches \
    sc_one_sided_float_drag
  self_check_case 'a longer paste under the menu on zz only' catches \
    sc_one_sided_menu_paste_tail
  self_check_case "zz's own pane marked and the pin's not" catches \
    sc_one_sided_marked_pane
  self_check_case "zz's own word-separators under the pane menu" catches \
    sc_one_sided_menu_context_word
  self_check_case "zz's word probe aimed at the wrapped row" catches \
    sc_one_sided_context_channel over-a-word
  self_check_case "zz's wrapped-head probe aimed at the word" catches \
    sc_one_sided_context_channel wrapped-head
  self_check_case "zz's wrapped-tail probe aimed at the word" catches \
    sc_one_sided_context_channel wrapped-tail
  self_check_case "zz's hyperlink probe aimed at the word" catches \
    sc_one_sided_context_channel hyperlink
  self_check_case "zz's blank-cell probe aimed at the word" catches \
    sc_one_sided_context_channel blank-cell
  if ! pin_menus_follow_the_window; then
    self_check_case "zz's window menu centred instead of over its status range" \
      catches sc_one_sided_status_menu_position
  else
    printf "note  self-check zz's window menu centred instead of over its status range skipped: %s\n" \
      "$STATUS_MENU_DRIFT"
  fi
  self_check_case 'WheelDownStatus unbound on zz only' catches \
    sc_one_sided_status_wheel
  self_check_case 'WheelUpPane unbound on zz only' catches \
    sc_one_sided_wheel_up_pane
  self_check_case "the copy table's MouseDragEnd1Pane unbound on zz only" catches \
    sc_one_sided_drag_end
  self_check_case 'DoubleClick1Pane unbound on zz only' catches \
    sc_one_sided_double_click
  self_check_case "zz's mouse-target click aimed into the other pane" catches \
    sc_one_sided_mouse_target
  self_check_case 'zz out of copy mode before the paste' catches \
    sc_one_sided_copy_mode_paste
  self_check_case 'MouseDragEnd1Pane bound in zz-s root table only' catches \
    sc_one_sided_drag_release
  self_check_case 'SecondClick1Pane bound in zz-s root table only' catches \
    sc_one_sided_second_click
  self_check_case 'only the focus-out report sent to zz with focus-events off' catches \
    sc_one_sided_focus_off
  customize_sabotage "zz's customize click aimed three rows higher" sc_one_sided_customize_click
  self_check_case "switch-mode's wheel held back on zz only" catches \
    sc_one_sided_switch_wheel
  customize_sabotage "zz's tree menu press aimed one row lower" sc_one_sided_customize_menu_row
  customize_sabotage "zz's tree menu press aimed seventeen cells right" sc_one_sided_customize_menu_column
  customize_sabotage "zz's tree menu release held back" sc_one_sided_customize_menu_release
  customize_sabotage "zz's tree menu drag released on the press cell" sc_one_sided_customize_menu_drag
  customize_sabotage "zz's Tag All drag released on the Tag row" sc_one_sided_customize_menu_tagall
  customize_sabotage "zz's outside menu press aimed twenty cells right" sc_one_sided_customize_menu_outside_column

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check cases did not behave as required\n' "$SELF_CHECK_FAILURES"
    exit 1
  fi
  printf 'self-check: every sabotage caught in its own channel\n'
  exit 0
}

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
else
  run_cases
fi

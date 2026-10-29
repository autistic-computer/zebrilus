#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-indicators.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-indicators.sh\n' >&2
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
SCRATCH_DIR="$(mktemp -d /tmp/zzin.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzino-$TOKEN"
INNER_SOCKET_NAME="zzini-$TOKEN"
ZZ_SOCKET="/tmp/zzin-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="ind"
WINDOW_NAME="win"
PANE_TITLE="indtitle"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
CASE_LABEL=""
ZZ_PID=""
FAILURES=0
CHECKS=0
RECORDS=0
LAST_ROWS_DIFFERED=0
LAST_CURSOR_DIFFERED=0
MESSAGE_HOLD_MS=20000
MESSAGE_EXPIRE_MS=100
COPY_POSITION_FORMAT='#[align=right][#{copy_position}/#{copy_position_limit}]'
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
mkdir -p "$ZZ_HOME" "$TMUX_HOME" "$OUTER_HOME" "$ZZ_LOG_DIR"

scrubbed() {
  env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL \
    -u XDG_STATE_HOME -u ZZ_LOG_DIR \
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
  trap - EXIT ERR INT TERM
  set +e
  tmux_outer_command kill-server >/dev/null 2>&1
  zz_command kill-server >/dev/null 2>&1
  tmux_inner_command kill-server >/dev/null 2>&1
  if [ -n "$ZZ_PID" ]; then
    kill "$ZZ_PID" >/dev/null 2>&1
    wait "$ZZ_PID" >/dev/null 2>&1
  fi
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
  local label="$1"
  local side
  printf 'wait that ran out: %s (case %s)\n' "$label" "${CASE_LABEL:-none yet}" >&2
  for side in zz tmux; do
    printf -- '--- %s screen ---\n' "$side" >&2
    tmux_outer_command capture-pane -p -t "=$OUTER_SESSION:$side" 2>&1 | cat -v >&2 || true
    printf -- '--- %s clients ---\n' "$side" >&2
    side_command "$side" list-clients -F '#{client_name} #{client_prefix} #{client_width}x#{client_height}' >&2 2>&1 || true
    printf -- '--- %s panes ---\n' "$side" >&2
    side_command "$side" list-panes -t "=$INNER_SESSION" -F '#{pane_id} in_mode=#{pane_in_mode} mode=#{pane_mode}' >&2 2>&1 || true
  done
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
  dump_state "$label"
  die "$label did not happen within 10 seconds"
}

CURSOR_FORMAT='#{cursor_x},#{cursor_y} flag=#{cursor_flag} pane=#{pane_width}x#{pane_height} shape=#{cursor_shape} blinking=#{cursor_blinking} very_visible=#{cursor_very_visible} colour=#{cursor_colour}'

capture_screen() {
  tmux_outer_command capture-pane -p -e -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
capture_plain() {
  tmux_outer_command capture-pane -p -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
cursor_tuple() {
  tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" "$CURSOR_FORMAT"
}
last_row() {
  capture_plain "$1" | tail -n 1
}

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" = "$INNER_SESSION" ]
}
client_name() {
  side_command "$1" list-clients -F '#{client_name}' 2>/dev/null | head -n 1
}
active_pane() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_id}' |
    awk '$1 == 1 { print $2; exit }'
}

write_attach() {
  local side="$1"
  local destination="$2"
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp %q --socket %q attach-session -t %q\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" "=$INNER_SESSION" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp %q -L %q attach-session -t %q\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" "=$INNER_SESSION" >>"$destination"
  fi
  chmod +x "$destination"
}

set_on_both() {
  side_command zz set-option -g "$1" "$2" || die "zz refused set-option -g $1"
  side_command tmux set-option -g "$1" "$2" || die "tmux refused set-option -g $1"
}
run_on_both() {
  side_command zz "$@" || die "zz refused $1"
  side_command tmux "$@" || die "tmux refused $1"
}
on_both_active() {
  local side pane argument
  local -a arguments
  for side in zz tmux; do
    pane="$(active_pane "$side")"
    [ -n "$pane" ] || die "$side has no active pane"
    arguments=()
    for argument in "$@"; do
      if [ "$argument" = PANE ]; then
        arguments+=("$pane")
      else
        arguments+=("$argument")
      fi
    done
    side_command "$side" "${arguments[@]}" || die "$side refused $1"
  done
}
send_both() {
  local side pane
  for side in zz tmux; do
    pane="$(active_pane "$side")"
    [ -n "$pane" ] || die "$side has no active pane"
    side_command "$side" send-keys -t "$pane" "$1" Enter || die "$side refused send-keys"
  done
}
type_on_both() {
  local side
  for side in zz tmux; do
    tmux_outer_command send-keys -t "=$OUTER_SESSION:$side" "$@" ||
      die "the outer tmux refused send-keys for $side"
  done
}

attach_both_at() {
  local columns="$1"
  local rows="$2"
  COLUMNS_UNDER_TEST="$columns"
  ROWS_UNDER_TEST="$rows"
  tmux_outer_command kill-server >/dev/null 2>&1 || true
  zz_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  tmux_inner_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  zz_command new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" -x "$columns" -y "$rows" \
    "$INNER_SHELL" || die "could not create the zz session"
  tmux_inner_command -f /dev/null new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" \
    -x "$columns" -y "$rows" "$INNER_SHELL" || die "could not create the tmux session"
  set_on_both status-right ''
  set_on_both status-left L
  set_on_both automatic-rename off
  set_on_both display-time "$MESSAGE_HOLD_MS"
  set_on_both copy-mode-position-format "$COPY_POSITION_FORMAT"
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$columns" -y "$rows" "$SCRATCH_DIR/attach-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/attach-tmux.sh"
  wait_for "outer zz pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  wait_for "zz client attached" client_attached zz
  wait_for "tmux client attached" client_attached tmux
  run_on_both rename-window -t "=$INNER_SESSION:0" "$WINDOW_NAME"
  run_on_both select-pane -t "=$INNER_SESSION:0.0" -T "$PANE_TITLE"
}

screen_has_marker() {
  capture_plain "$1" | grep -Fq "$2"
}
wait_settled() {
  local side="$1"
  local marker="$2"
  local label="$3"
  local previous=""
  local current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_plain "$side" 2>/dev/null || true)"
    if [ -n "$previous" ] && [ "$current" = "$previous" ] &&
      printf '%s' "$current" | grep -Fq "$marker"; then
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

mark_both() {
  local name="$1"
  send_both "printf 'MARK-%s\\n' $name"
  settle_both "MARK-$name" "$name"
}

compare_rows() {
  local name="$1"
  local styled="$2"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing total
  if [ "$styled" = styled ]; then
    mapfile -t zz_rows < <(capture_screen zz)
    mapfile -t tmux_rows < <(capture_screen tmux)
  else
    mapfile -t zz_rows < <(capture_plain zz)
    mapfile -t tmux_rows < <(capture_plain tmux)
  fi
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  total="$ROWS_UNDER_TEST"
  differing=-1
  for ((index = 0; index < total; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      differing="$index"
      break
    fi
  done
  LAST_ROWS_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  [ "$differing" -lt 0 ] || LAST_ROWS_DIFFERED=1
  [ "$zz_cursor" = "$tmux_cursor" ] || LAST_CURSOR_DIFFERED=1
  if [ "$LAST_ROWS_DIFFERED" -eq 0 ] && [ "$LAST_CURSOR_DIFFERED" -eq 0 ]; then
    return 0
  fi
  printf '      case %s\n' "$name"
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf '      first differing row %s of %s\n' "$differing" "$total"
    printf '        tmux: %s\n' "$(printf '%s' "${tmux_rows[differing]-}" | cat -v)"
    printf '        zz:   %s\n' "$(printf '%s' "${zz_rows[differing]-}" | cat -v)"
  else
    printf '      all %s rows identical\n' "$total"
  fi
  printf '      cursor tmux: %s\n' "$tmux_cursor"
  printf '      cursor zz:   %s\n' "$zz_cursor"
  return 1
}

verdict() {
  local name="$1"
  local mode="$2"
  local reason="${3:-}"
  if [ "$mode" = text ]; then
    CHECKS=$((CHECKS + 1))
    RECORDS=$((RECORDS + 1))
    [ -n "$reason" ] || die "recorded style at $name says nothing about why"
    if compare_rows "$name" plain; then
      printf 'ok    %s: every glyph, column and the cursor identical\n' "$name"
    else
      FAILURES=$((FAILURES + 1))
      printf 'DIFF  %s\n' "$name"
    fi
    if compare_rows "$name" styled; then
      printf 'note  %s: styles identical too, the record can close\n' "$name"
    else
      printf 'note  %s: styles recorded, not asserted: %s\n' "$name" "$reason"
    fi
    return 0
  fi
  if [ "$mode" = same ]; then
    CHECKS=$((CHECKS + 1))
  else
    RECORDS=$((RECORDS + 1))
  fi
  if compare_rows "$name" styled; then
    printf 'ok    %s\n' "$name"
    return 0
  fi
  if [ "$mode" = same ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s\n' "$name"
  else
    [ -n "$reason" ] || die "recorded case $name says nothing about why"
    printf 'note  %s recorded, not asserted: %s\n' "$name" "$reason"
  fi
  return 0
}

compare_split_rows() {
  local name="$1"
  local divider="$2"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  differing=-1
  for ((index = 0; index < ROWS_UNDER_TEST; index++)); do
    [ "${zz_rows[index]-}" = "${tmux_rows[index]-}" ] && continue
    differing="$index"
    break
  done
  LAST_ROWS_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  [ "$differing" -lt 0 ] || LAST_ROWS_DIFFERED=1
  [ "$zz_cursor" = "$tmux_cursor" ] || LAST_CURSOR_DIFFERED=1
  if [ "$LAST_ROWS_DIFFERED" -eq 0 ] && [ "$LAST_CURSOR_DIFFERED" -eq 0 ]; then
    return 0
  fi
  printf '      case %s (divider row %s asserted whole)\n' "$name" "$divider"
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf '      first differing row %s of %s\n' "$differing" "$ROWS_UNDER_TEST"
    printf '        tmux: %s\n' "$(printf '%s' "${tmux_rows[differing]-}" | cat -v)"
    printf '        zz:   %s\n' "$(printf '%s' "${zz_rows[differing]-}" | cat -v)"
  fi
  printf '      cursor tmux: %s\n' "$tmux_cursor"
  printf '      cursor zz:   %s\n' "$zz_cursor"
  return 1
}
verdict_split() {
  local name="$1"
  local divider="$2"
  CHECKS=$((CHECKS + 1))
  if compare_split_rows "$name" "$divider"; then
    printf 'ok    %s (divider row %s asserted whole)\n' "$name" "$divider"
    return 0
  fi
  FAILURES=$((FAILURES + 1))
  printf 'DIFF  %s\n' "$name"
}
divider_row_below() {
  local bottom
  bottom="$(side_command tmux display-message -p -t "$1" '#{pane_bottom}')" ||
    die 'tmux refused pane_bottom'
  printf '%s\n' "$((bottom + 1))"
}

pane_in_mode_is() {
  [ "$(side_command "$1" display-message -p -t "$(active_pane "$1")" '#{pane_in_mode}' 2>/dev/null)" = "$2" ]
}
client_prefix_is() {
  [ "$(side_command "$1" display-message -c "$(client_name "$1")" -p '#{client_prefix}' 2>/dev/null)" = "$2" ]
}
last_row_has() {
  case "$(last_row "$1")" in
  *"$2"*) return 0 ;;
  *) return 1 ;;
  esac
}
last_row_starts_with() {
  case "$(last_row "$1")" in
  "$2"*) return 0 ;;
  *) return 1 ;;
  esac
}
screen_has() {
  capture_plain "$1" | grep -Fq "$2"
}
last_row_lacks() {
  last_row_has "$1" "$2" && return 1
  return 0
}
both_pane_in_mode() {
  wait_for "$2 in mode on the zz pane" pane_in_mode_is zz "$1"
  wait_for "$2 in mode on the tmux pane" pane_in_mode_is tmux "$1"
}
both_client_prefix() {
  wait_for "$2 prefix on the zz client" client_prefix_is zz "$1"
  wait_for "$2 prefix on the tmux client" client_prefix_is tmux "$1"
}
both_last_row_has() {
  wait_for "$2 on the zz status row" last_row_has zz "$1"
  wait_for "$2 on the tmux status row" last_row_has tmux "$1"
}
both_last_row_lacks() {
  wait_for "$2 gone from the zz status row" last_row_lacks zz "$1"
  wait_for "$2 gone from the tmux status row" last_row_lacks tmux "$1"
}
both_last_row_starts_with() {
  wait_for "$2 on the zz status row" last_row_starts_with zz "$1"
  wait_for "$2 on the tmux status row" last_row_starts_with tmux "$1"
}
both_screen_has() {
  wait_for "$2 on the zz screen" screen_has zz "$1"
  wait_for "$2 on the tmux screen" screen_has tmux "$1"
}

seed_scrollback() {
  send_both 'seq 1 60'
}

copy_mode_case() {
  CASE_LABEL=copy-mode
  mark_both copy
  on_both_active copy-mode -t PANE
  both_pane_in_mode 1 'copy mode'
  settle_both MARK-copy 'copy mode'
  verdict copy-mode-entered same
  on_both_active send-keys -X -t PANE cancel
  both_pane_in_mode 0 'copy mode left'
  settle_both MARK-copy 'copy mode left'
  verdict copy-mode-restored same
}

view_surface_case() {
  CASE_LABEL=view-surface
  mark_both view
  on_both_active run-shell -t PANE 'printf VIEWLINE-1'
  both_screen_has VIEWLINE-1 'the run-shell output'
  wait_for 'the pin entered view mode' pane_in_mode_is tmux 1
  settle_both VIEWLINE-1 'the view surface'
  verdict view-surface-shown same
  type_on_both Escape
  wait_for 'the pin left view mode' pane_in_mode_is tmux 0
  settle_both MARK-view 'the view surface withdrawn'
  verdict view-surface-restored same
}

view_long_case() {
  CASE_LABEL=view-long
  mark_both vlong
  on_both_active run-shell -t PANE "seq -f 'VIEWLONG-%g' 1 100"
  both_screen_has VIEWLONG-1 'the long run-shell output'
  wait_for 'the pin entered view mode' pane_in_mode_is tmux 1
  settle_both VIEWLONG-1 'the long view surface'
  verdict view-long-shown same
  type_on_both Down Down
  settle_both VIEWLONG-1 'the long view moved'
  verdict view-long-down same
  type_on_both q
  wait_for 'the pin left view mode' pane_in_mode_is tmux 0
  settle_both MARK-vlong 'the long view withdrawn'
  verdict view-long-restored same
}

prefix_case() {
  CASE_LABEL=prefix
  mark_both prefix
  type_on_both C-b
  both_client_prefix 1 'the armed'
  settle_both MARK-prefix 'the armed prefix'
  verdict prefix-armed same
  type_on_both C-g
  both_client_prefix 0 'the released'
  settle_both MARK-prefix 'the released prefix'
  verdict prefix-released same
}

message_case() {
  CASE_LABEL=display-message
  mark_both message
  set_on_both display-time "$MESSAGE_HOLD_MS"
  side_command zz display-message -c "$(client_name zz)" 'INDICATOR-MESSAGE' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'INDICATOR-MESSAGE' ||
    die 'tmux refused display-message'
  both_last_row_has INDICATOR-MESSAGE 'the message'
  settle_both MARK-message 'the message'
  verdict message-shown same
  set_on_both display-time "$MESSAGE_EXPIRE_MS"
  side_command zz display-message -c "$(client_name zz)" 'INDICATOR-EXPIRES' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'INDICATOR-EXPIRES' ||
    die 'tmux refused display-message'
  both_last_row_has INDICATOR-EXPIRES 'the second message'
  both_last_row_lacks INDICATOR-EXPIRES 'the expired message'
  settle_both MARK-message 'the expired message'
  verdict message-restored same
  set_on_both display-time "$MESSAGE_HOLD_MS"
}

binding_error_case() {
  CASE_LABEL=binding-error
  mark_both binderror
  run_on_both bind-key -n F5 kill-pane -t zzind-missing
  set_on_both display-time "$MESSAGE_HOLD_MS"
  type_on_both F5
  both_last_row_has "find pane: zzind-missing" 'the binding error'
  settle_both MARK-binderror 'the binding error'
  verdict binding-error-shown same
  type_on_both x
  both_last_row_lacks "find pane: zzind-missing" 'the binding error a key cleared'
  type_on_both BSpace
  settle_both MARK-binderror 'the binding error a key cleared'
  verdict binding-error-key-cleared same
  set_on_both display-time 1500
  type_on_both F5
  both_last_row_has "find pane: zzind-missing" 'the timed binding error'
  both_last_row_lacks "find pane: zzind-missing" 'the expired binding error'
  settle_both MARK-binderror 'the expired binding error'
  verdict binding-error-expired same
  run_on_both unbind-key -n F5
  set_on_both display-time "$MESSAGE_HOLD_MS"
}

prompt_case() {
  CASE_LABEL=command-prompt
  mark_both prompt
  type_on_both C-b
  both_client_prefix 1 'the armed'
  type_on_both ':'
  both_last_row_starts_with ':' 'the command prompt'
  settle_both MARK-prompt 'the command prompt'
  verdict prompt-opened same
  type_on_both l i s t
  both_last_row_has ':list' 'the typed command'
  settle_both MARK-prompt 'the typed command'
  verdict prompt-typed same
  type_on_both Escape
  both_last_row_lacks ':list' 'the cancelled prompt'
  settle_both MARK-prompt 'the cancelled prompt'
  verdict prompt-cancelled same
}

styled_screen_differs() {
  [ "$(capture_screen "$1" 2>/dev/null)" != "$2" ]
}
select_on_both() {
  local zz_before tmux_before
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  on_both_active send-keys -X -t PANE cursor-up
  on_both_active send-keys -X -t PANE start-of-line
  on_both_active send-keys -X -t PANE begin-selection
  on_both_active send-keys -X -N 4 -t PANE cursor-right
  wait_for 'the selection on the zz screen' styled_screen_differs zz "$zz_before"
  wait_for 'the selection on the tmux screen' styled_screen_differs tmux "$tmux_before"
}
selection_case() {
  local name="$1"
  local keys="$2"
  local style="$3"
  CASE_LABEL="selection-$name"
  set_on_both mode-keys "$keys"
  [ -z "$style" ] || set_on_both copy-mode-selection-style "$style"
  mark_both "sel$name"
  on_both_active copy-mode -t PANE
  both_pane_in_mode 1 'copy mode'
  settle_both "MARK-sel$name" 'copy mode'
  select_on_both
  settle_both "MARK-sel$name" "the $name selection"
  verdict "selection-$name" same
  on_both_active send-keys -X -t PANE cancel
  both_pane_in_mode 0 'copy mode left'
  settle_both "MARK-sel$name" 'copy mode left'
  run_on_both set-option -gu mode-keys
  [ -z "$style" ] || run_on_both set-option -gu copy-mode-selection-style
}

client_key_table_is_a_copy_table() {
  case "$(side_command zz list-clients -F '#{client_key_table}' 2>/dev/null | head -n 1)" in
  copy-mode | copy-mode-vi) return 0 ;;
  *) return 1 ;;
  esac
}
pane_id_in_mode_is() {
  [ "$(side_command "$1" display-message -p -t "$2" '#{pane_in_mode}' 2>/dev/null)" = "$3" ]
}
inactive_pane() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_id}' |
    awk '$1 == 0 { print $2; exit }'
}
window_pane() {
  side_command "$1" list-panes -t "=$INNER_SESSION:$2" -F '#{pane_id}' | head -n 1
}
window_count_is() {
  [ "$(side_command "$1" list-windows -t "=$INNER_SESSION" -F x 2>/dev/null | wc -l)" = "$2" ]
}
pane_count_is() {
  [ "$(side_command "$1" list-panes -t "=$INNER_SESSION" -F x 2>/dev/null | wc -l)" = "$2" ]
}
soft_settled() {
  local side="$1"
  local marker="$2"
  local previous=""
  local current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_plain "$side" 2>/dev/null || true)"
    if [ -n "$previous" ] && [ "$current" = "$previous" ] &&
      printf '%s' "$current" | grep -Fq "$marker"; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  return 1
}
type_command_on_both() {
  local marker="$1"
  local fallback="$2"
  local side
  type_on_both -l "printf 'TYPED-%s\\n' $marker"
  type_on_both Enter
  wait_settled tmux "TYPED-$marker" "the typed command on the tmux screen"
  if ! soft_settled zz "TYPED-$marker"; then
    wait_settled zz "$fallback" "the zz screen after the typed command"
  fi
}
open_view_on() {
  local zz_pane="$1"
  local tmux_pane="$2"
  local text="$3"
  side_command zz run-shell -t "$zz_pane" "printf $text" || die 'zz refused run-shell'
  side_command tmux run-shell -t "$tmux_pane" "printf $text" || die 'tmux refused run-shell'
  wait_for "the pin put $tmux_pane in view mode" pane_id_in_mode_is tmux "$tmux_pane" 1
  wait_for 'zz opened the command output' client_key_table_is_a_copy_table
}

view_hidden_case() {
  local zz_hidden tmux_hidden
  CASE_LABEL=view-hidden
  mark_both hidden
  run_on_both new-window -d -n hid -t "=$INNER_SESSION:" "$INNER_SHELL"
  wait_for 'the second zz window' window_count_is zz 2
  wait_for 'the second tmux window' window_count_is tmux 2
  zz_hidden="$(window_pane zz 1)"
  tmux_hidden="$(window_pane tmux 1)"
  [ -n "$zz_hidden" ] && [ -n "$tmux_hidden" ] || die 'no pane in the hidden window'
  both_last_row_has '1:hid' 'the hidden window in the list'
  settle_both MARK-hidden 'the hidden window'
  open_view_on "$zz_hidden" "$tmux_hidden" HIDDENOUT
  settle_both MARK-hidden 'the view on the hidden pane'
  verdict view-hidden-opened same
  type_command_on_both HIDDEN MARK-hidden
  verdict view-hidden-typed same
  run_on_both kill-window -t "=$INNER_SESSION:1"
  wait_for 'the hidden zz window gone' window_count_is zz 1
  wait_for 'the hidden tmux window gone' window_count_is tmux 1
  both_last_row_lacks '1:hid' 'the hidden window'
  settle_both TYPED-HIDDEN 'the hidden window withdrawn'
}

split_for_inactive() {
  run_on_both split-window -v -t "=$INNER_SESSION:0" "$INNER_SHELL"
  wait_for 'the zz split' pane_count_is zz 2
  wait_for 'the tmux split' pane_count_is tmux 2
  settle_both "$1" 'the split'
}

view_inactive_case() {
  local zz_other tmux_other side divider
  CASE_LABEL=view-inactive
  mark_both inactive
  split_for_inactive MARK-inactive
  zz_other="$(inactive_pane zz)"
  tmux_other="$(inactive_pane tmux)"
  [ -n "$zz_other" ] && [ -n "$tmux_other" ] || die 'no inactive pane after the split'
  divider="$(divider_row_below "$tmux_other")"
  open_view_on "$zz_other" "$tmux_other" NONACTIVEOUT
  settle_both NONACTIVEOUT 'the view on the inactive pane'
  verdict_split view-inactive-opened "$divider"
  type_command_on_both INACTIVE NONACTIVEOUT
  verdict_split view-inactive-typed "$divider"
  for side in zz tmux; do
    side_command "$side" kill-pane -t "$(active_pane "$side")" || die "$side refused kill-pane"
  done
  wait_for 'the zz split withdrawn' pane_count_is zz 1
  wait_for 'the tmux split withdrawn' pane_count_is tmux 1
  run_on_both select-pane -t "=$INNER_SESSION:0.0" -T "$PANE_TITLE"
  type_on_both q
  wait_for 'the pin left view mode' pane_in_mode_is tmux 0
  settle_both MARK-inactive 'the view withdrawn'
}

write_attach zz "$SCRATCH_DIR/attach-zz.sh"
write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"

zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

run_cases() {
  printf 'indicator, message and prompt differential at %sx%s (pin %s)\n' \
    "$COLUMNS_UNDER_TEST" "$ROWS_UNDER_TEST" "$(basename -- "$TMUX_BIN")"
  attach_both_at 80 24
  CASE_LABEL=baseline
  mark_both baseline
  verdict baseline same
  seed_scrollback
  CASE_LABEL=scrollback
  mark_both scrollback
  verdict scrollback same
  copy_mode_case
  view_surface_case
  view_long_case
  prefix_case
  message_case
  binding_error_case
  prompt_case
  selection_case emacs emacs ''
  selection_case vi vi ''
  selection_case styled emacs 'bg=red,fg=white,bold'
  view_hidden_case
  view_inactive_case

  if [ "$FAILURES" -ne 0 ]; then
    printf '%s of %s asserted comparisons differ, %s recorded\n' "$FAILURES" "$CHECKS" "$RECORDS"
    exit 1
  fi
  printf 'all %s asserted comparisons identical, %s recorded not asserted\n' "$CHECKS" "$RECORDS"
}

SELF_CHECK_FAILURES=0

self_check_case() {
  local name="$1"
  local expectation="$2"
  local outcome=ok
  case "$expectation" in
  rows)
    [ "$LAST_ROWS_DIFFERED" -eq 1 ] || outcome='no row difference reported'
    ;;
  none)
    if [ "$LAST_ROWS_DIFFERED" -ne 0 ] || [ "$LAST_CURSOR_DIFFERED" -ne 0 ]; then
      outcome='reported a difference the pin collapses'
    fi
    ;;
  esac
  if [ "$outcome" = ok ]; then
    printf 'ok    self-check %s\n' "$name"
    return 0
  fi
  SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
  printf 'FAIL  self-check %s: %s\n' "$name" "$outcome"
}

run_self_check() {
  printf 'self-check: one deliberate difference per channel, plus one the pin collapses\n'

  CASE_LABEL='self-check prefix hint'
  attach_both_at 80 24
  mark_both hint
  side_command zz set-option -g status-right ' PREFIX ' || die 'zz refused status-right'
  type_on_both C-b
  both_client_prefix 1 'the armed'
  settle_both MARK-hint 'the armed prefix with a hint cell'
  compare_rows self-check-prefix-hint styled || true
  self_check_case 'prefix, an extra hint cell on one status row' rows
  type_on_both C-g

  CASE_LABEL='self-check message text'
  attach_both_at 80 24
  mark_both msg
  set_on_both display-time "$MESSAGE_HOLD_MS"
  side_command zz display-message -c "$(client_name zz)" 'SABOTAGE-MSG-A' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'SABOTAGE-MSG-B' ||
    die 'tmux refused display-message'
  both_last_row_has SABOTAGE-MSG 'the sabotaged message'
  settle_both MARK-msg 'the sabotaged message'
  compare_rows self-check-message plain || true
  self_check_case 'message, one character of the text differs' rows

  CASE_LABEL='self-check prompt residue'
  attach_both_at 80 24
  mark_both residue
  type_on_both C-b
  both_client_prefix 1 'the armed'
  type_on_both ':'
  both_last_row_starts_with ':' 'the command prompt'
  settle_both MARK-residue 'the command prompt'
  tmux_outer_command send-keys -t "=$OUTER_SESSION:tmux" Escape ||
    die 'the outer tmux refused send-keys'
  wait_for 'the pin cancelled its prompt' last_row_starts_with tmux L
  settle_both MARK-residue 'the one-sided cancel'
  compare_rows self-check-prompt-residue styled || true
  self_check_case 'prompt, one side leaves the prompt open as residue' rows

  CASE_LABEL='self-check copy position style'
  attach_both_at 80 24
  side_command zz set-option -g copy-mode-position-style 'bg=red,fg=white' ||
    die 'zz refused copy-mode-position-style'
  mark_both position
  on_both_active copy-mode -t PANE
  both_pane_in_mode 1 'copy mode'
  settle_both MARK-position 'copy mode with one side restyled'
  compare_rows self-check-copy-position styled || true
  self_check_case 'copy mode, one side paints the position in another style' rows
  on_both_active send-keys -X -t PANE cancel
  both_pane_in_mode 0 'copy mode left'
  side_command zz set-option -gu copy-mode-position-style || die 'zz refused -gu'

  CASE_LABEL='self-check view position'
  attach_both_at 80 24
  side_command zz set-option -g copy-mode-position-format '#[align=right]<#{copy_position}>' ||
    die 'zz refused copy-mode-position-format'
  mark_both viewsab
  on_both_active run-shell -t PANE 'printf VIEWLINE-S'
  both_screen_has VIEWLINE-S 'the sabotaged run-shell output'
  wait_for 'the pin entered view mode' pane_in_mode_is tmux 1
  settle_both VIEWLINE-S 'the sabotaged view surface'
  compare_rows self-check-view-position styled || true
  self_check_case 'view surface, one side draws another position format' rows
  type_on_both Escape
  wait_for 'the pin left view mode' pane_in_mode_is tmux 0
  side_command zz set-option -g copy-mode-position-format "$COPY_POSITION_FORMAT" ||
    die 'zz refused copy-mode-position-format'

  CASE_LABEL='self-check message style'
  attach_both_at 80 24
  side_command zz set-option -g message-style 'bg=red,fg=white' ||
    die 'zz refused message-style'
  mark_both mstyle
  set_on_both display-time "$MESSAGE_HOLD_MS"
  side_command zz display-message -c "$(client_name zz)" 'SABOTAGE-STYLE' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'SABOTAGE-STYLE' ||
    die 'tmux refused display-message'
  both_last_row_has SABOTAGE-STYLE 'the restyled message'
  settle_both MARK-mstyle 'the restyled message'
  compare_rows self-check-message-style styled || true
  self_check_case 'message, one side paints the message in another style' rows

  CASE_LABEL='self-check prompt style'
  attach_both_at 80 24
  side_command zz set-option -g message-style 'bg=red,fg=white' ||
    die 'zz refused message-style'
  mark_both pstyle
  type_on_both C-b
  both_client_prefix 1 'the armed'
  type_on_both ':'
  both_last_row_starts_with ':' 'the command prompt'
  settle_both MARK-pstyle 'the restyled prompt'
  compare_rows self-check-prompt-style styled || true
  self_check_case 'prompt, one side paints the prompt in another style' rows
  type_on_both Escape
  both_last_row_starts_with L 'the cancelled prompt'
  side_command zz set-option -gu message-style || die 'zz refused -gu'

  CASE_LABEL='self-check selection keys'
  attach_both_at 80 24
  side_command zz set-option -g mode-keys vi || die 'zz refused mode-keys'
  mark_both selsab
  on_both_active copy-mode -t PANE
  both_pane_in_mode 1 'copy mode'
  settle_both MARK-selsab 'copy mode'
  select_on_both
  settle_both MARK-selsab 'the one-sided vi selection'
  compare_rows self-check-selection-keys styled || true
  self_check_case 'selection, one side selects with the vi table' rows
  on_both_active send-keys -X -t PANE cancel
  both_pane_in_mode 0 'copy mode left'
  side_command zz set-option -gu mode-keys || die 'zz refused -gu'

  CASE_LABEL='self-check view on the active pane'
  attach_both_at 80 24
  mark_both hidsab
  run_on_both new-window -d -n hid -t "=$INNER_SESSION:" "$INNER_SHELL"
  wait_for 'the second zz window' window_count_is zz 2
  wait_for 'the second tmux window' window_count_is tmux 2
  both_last_row_has '1:hid' 'the hidden window in the list'
  settle_both MARK-hidsab 'the hidden window'
  open_view_on "$(active_pane zz)" "$(window_pane tmux 1)" HIDDENOUT
  type_command_on_both HIDDEN ''
  compare_rows self-check-view-owner styled || true
  self_check_case 'view, one side opens it on the active pane and swallows the keys' rows

  CASE_LABEL='self-check view owner in a split'
  attach_both_at 80 24
  mark_both splitsab
  split_for_inactive MARK-splitsab
  open_view_on "$(active_pane zz)" "$(inactive_pane tmux)" NONACTIVEOUT
  type_command_on_both INACTIVE ''
  compare_split_rows self-check-view-owner-split "$(divider_row_below "$(inactive_pane tmux)")" || true
  self_check_case 'view in a split, one side opens it on the active pane' rows

  CASE_LABEL='self-check divider style'
  attach_both_at 80 24
  mark_both divsab
  split_for_inactive MARK-divsab
  side_command zz set-option -g pane-border-style fg=red || die 'zz refused pane-border-style'
  mark_both divset
  settle_both MARK-divset 'the one-sided divider style'
  compare_split_rows self-check-divider-style "$(divider_row_below "$(inactive_pane tmux)")" || true
  self_check_case 'divider, pane-border-style fg=red on one side' rows

  CASE_LABEL='self-check equivalence'
  attach_both_at 80 24
  mark_both equal
  type_on_both C-b
  both_client_prefix 1 'the armed'
  settle_both MARK-equal 'the armed prefix on both sides'
  compare_rows self-check-equivalence styled || true
  self_check_case 'equivalence: the same armed prefix on both sides' none
  type_on_both C-g

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check expectations unmet\n' "$SELF_CHECK_FAILURES"
    exit 1
  fi
  printf 'self-check complete: every sabotage was caught and the equivalence passed\n'
}

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
else
  run_cases
fi

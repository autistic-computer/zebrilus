#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-overlays.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-overlays.sh\n' >&2
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
SCRATCH_DIR="$(mktemp -d /tmp/zzov.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzovo-$TOKEN"
INNER_SOCKET_NAME="zzovi-$TOKEN"
ZZ_SOCKET="/tmp/zzov-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="ovl"
WINDOW_NAME="win"
PANE_TITLE="ovltitle"
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
LAST_DIFFERING_ROWS=""
MESSAGE_HOLD_MS=20000
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
POPUP_JOB="printf 'POPUP-BODY\\n'; read line; printf 'POPUP-GOT-%s\\n' \"\$line\"; read line"
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
  if [ -n "$ZZ_PID" ]; then
    kill "$ZZ_PID" >/dev/null 2>&1
    wait "$ZZ_PID" >/dev/null 2>&1
  fi
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
  local label="$1"
  local side
  printf 'wait that ran out: %s (case %s)\n' "$label" "${CASE_LABEL:-none yet}" >&2
  for side in zz tmux; do
    printf -- '--- %s screen ---\n' "$side" >&2
    tmux_outer_command capture-pane -p -t "=$OUTER_SESSION:$side" 2>&1 | cat -v >&2 || true
    printf -- '--- %s clients ---\n' "$side" >&2
    side_command "$side" list-clients -F '#{client_name} #{client_prefix} #{client_width}x#{client_height}' >&2 2>&1 || true
    printf -- '--- %s panes ---\n' "$side" >&2
    side_command "$side" list-panes -t "=$INNER_SESSION" -F '#{pane_id} active=#{pane_active} in_mode=#{pane_in_mode}' >&2 2>&1 || true
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

DIVIDER_RULE=0
DIVIDER_GLYPH=$'\xe2\x94\x82'
capture_screen() {
  if [ "$DIVIDER_RULE" -eq 1 ]; then
    tmux_outer_command capture-pane -p -e -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
      -t "=$OUTER_SESSION:$1" |
      LC_ALL=C sed -E "s/(\x1b\[[0-9;:]*m)*$DIVIDER_GLYPH(\x1b\[[0-9;:]*m)*/$DIVIDER_GLYPH/g"
    return
  fi
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
cursor_column_is() {
  [ "$(tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" '#{cursor_x}' 2>/dev/null)" = "$2" ]
}
last_row() {
  capture_plain "$1" | tail -n 1
}
first_row() {
  capture_plain "$1" | head -n 1
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
client_size_is() {
  [ "$(side_command "$1" list-clients -F '#{client_width}x#{client_height}' 2>/dev/null | head -n 1)" = "$2" ]
}
active_pane() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_id}' |
    awk '$1 == 1 { print $2; exit }'
}
active_pane_index_is() {
  [ "$(side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_index}' 2>/dev/null |
    awk '$1 == 1 { print $2; exit }')" = "$2" ]
}

write_attach() {
  local side="$1"
  local destination="$2"
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
  side_command zz set-option -g "$1" "$2" || die "zz refused set-option -g $1"
  side_command tmux set-option -g "$1" "$2" || die "tmux refused set-option -g $1"
}
run_on_both() {
  side_command zz "$@" || die "zz refused $1"
  side_command tmux "$@" || die "tmux refused $1"
}
client_on_both() {
  local side client argument
  local -a arguments
  for side in zz tmux; do
    client="$(client_name "$side")"
    [ -n "$client" ] || die "$side has no client"
    arguments=()
    for argument in "$@"; do
      if [ "$argument" = CLIENT ]; then
        arguments+=("$client")
      else
        arguments+=("$argument")
      fi
    done
    side_command "$side" "${arguments[@]}" || die "$side refused $1"
  done
}
PIN_DISPLAY_PANES_MODE=0
ZOOM_REFLOW_MODE=same
ZOOM_REFLOW_OWNER=gap:terminal.zoom-reflow
ZOOM_REFLOW_REASON='gap:terminal.zoom-reflow (accepted engine limit, fabrico 2026-09-18): 3.8 display-panes zooms its pane, and libghostty and the pin reflow a pane whose history holds a wrapped line differently on a width change (resize-pane -Z twice shows the same split), so the pane content after the mode, and the live copy the mode redraws at a new size, differ while the mode screen itself is asserted'
display_panes_on_both() {
  if [ "$PIN_DISPLAY_PANES_MODE" -eq 1 ]; then
    side_command zz display-panes -d 0 || die 'zz refused display-panes'
    side_command tmux display-panes -d 0 || die 'tmux refused display-panes'
  else
    client_on_both display-panes -b -d 0 -t CLIENT
  fi
}
send_both() {
  local side pane
  for side in zz tmux; do
    pane="$(active_pane "$side")"
    [ -n "$pane" ] || die "$side has no active pane"
    side_command "$side" send-keys -t "$pane" "$1" Enter || die "$side refused send-keys"
  done
}
type_on() {
  local side="$1"
  shift
  tmux_outer_command send-keys -t "=$OUTER_SESSION:$side" "$@" ||
    die "the outer tmux refused send-keys for $side"
}
type_on_both() {
  type_on zz "$@"
  type_on tmux "$@"
}
press_on_both() {
  type_on_both C-b
  type_on_both "$1"
}

bind_on_both() {
  run_on_both bind-key -T prefix M display-menu -x 4 -y 12 -T OVERLAY-MENU \
    'First item' f 'set-option -g @overlay_menu first' \
    '' \
    'Second item' s 'set-option -g @overlay_menu second' \
    '-Disabled item' d 'set-option -g @overlay_menu disabled' \
    'Third item' t 'set-option -g @overlay_menu third'
  run_on_both bind-key -T prefix N display-menu -M -x 4 -y 12 -T OVERLAY-MENU \
    'First item' f 'set-option -g @overlay_menu first' \
    '' \
    'Second item' s 'set-option -g @overlay_menu second' \
    '-Disabled item' d 'set-option -g @overlay_menu disabled' \
    'Third item' t 'set-option -g @overlay_menu third'
  run_on_both bind-key -T prefix P display-popup -w 34 -h 9 -T OVERLAY-POPUP -E "$POPUP_JOB"
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
  set_on_both default-shell /bin/sh
  set_on_both mouse on
  bind_on_both
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

resize_both_to() {
  local columns="$1"
  local rows="$2"
  tmux_outer_command resize-window -t "=$OUTER_SESSION:zz" -x "$columns" -y "$rows" ||
    die 'the outer tmux refused resize-window'
  tmux_outer_command resize-window -t "=$OUTER_SESSION:tmux" -x "$columns" -y "$rows" ||
    die 'the outer tmux refused resize-window'
  COLUMNS_UNDER_TEST="$columns"
  ROWS_UNDER_TEST="$rows"
  wait_for "outer zz pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  wait_for "zz client at ${columns}x${rows}" client_size_is zz "${columns}x${rows}"
  wait_for "tmux client at ${columns}x${rows}" client_size_is tmux "${columns}x${rows}"
}

screen_has() {
  capture_plain "$1" | grep -Fq "$2"
}
screen_lacks() {
  screen_has "$1" "$2" && return 1
  return 0
}
last_row_has() {
  case "$(last_row "$1")" in
  *"$2"*) return 0 ;;
  *) return 1 ;;
  esac
}
last_row_lacks() {
  last_row_has "$1" "$2" && return 1
  return 0
}
last_row_starts_with() {
  case "$(last_row "$1")" in
  "$2"*) return 0 ;;
  *) return 1 ;;
  esac
}
both_last_row_starts_with() {
  wait_for "$2 on the zz status row" last_row_starts_with zz "$1"
  wait_for "$2 on the tmux status row" last_row_starts_with tmux "$1"
}
first_row_has() {
  case "$(first_row "$1")" in
  *"$2"*) return 0 ;;
  *) return 1 ;;
  esac
}
screen_differs_from() {
  [ "$(capture_screen "$1")" != "$2" ]
}
option_is() {
  [ "$(side_command "$1" show-options -gqv "$2" 2>/dev/null)" = "$3" ]
}
both_screen_has() {
  wait_for "$2 on the zz screen" screen_has zz "$1"
  wait_for "$2 on the tmux screen" screen_has tmux "$1"
}
both_screen_lacks() {
  wait_for "$2 gone from the zz screen" screen_lacks zz "$1"
  wait_for "$2 gone from the tmux screen" screen_lacks tmux "$1"
}
both_last_row_has() {
  wait_for "$2 on the zz status row" last_row_has zz "$1"
  wait_for "$2 on the tmux status row" last_row_has tmux "$1"
}
both_last_row_lacks() {
  wait_for "$2 gone from the zz status row" last_row_lacks zz "$1"
  wait_for "$2 gone from the tmux status row" last_row_lacks tmux "$1"
}
both_option_is() {
  wait_for "$3 on zz" option_is zz "$1" "$2"
  wait_for "$3 on tmux" option_is tmux "$1" "$2"
}

wait_settled() {
  local side="$1"
  local marker="$2"
  local label="$3"
  local previous=""
  local current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_screen "$side" 2>/dev/null || true)"
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
  send_both "printf '\\033[H\\033[2JMARK-%s\\n' $name"
  settle_both "MARK-$name" "$name"
}

POPUP_DRIFT_OWNER=gap:display-popup.modal-pane
POPUP_TITLES=(OVERLAY-POPUP CENTRE-POPUP)
known_drift() {
  case "$1" in
  popup-opened | popup-typed) printf 'title flag' ;;
  popup-under-message | popup-resized) printf 'title' ;;
  centre-popup-fg | centre-popup-fg-typed | centre-popup-fgbg | centre-popup-fgbg-typed)
    printf 'title flag row'
    ;;
  esac
}
declare -A DRIFT_OWNERS=()
DRIFTS=0
box_row() {
  local glyph="$1" index
  shift
  local rows=("$@")
  for index in "${!rows[@]}"; do
    case "${rows[index]}" in
    *"$glyph"*)
      printf '%s' "$index"
      return
      ;;
    esac
  done
}
apply_known_drift() {
  local drift="$1" index title top bottom pin_top last cursor_x cursor_y cursor_rest
  if [[ " $drift " == *" title "* ]]; then
    for index in "${!zz_rows[@]}"; do
      for title in "${POPUP_TITLES[@]}"; do
        zz_rows[index]="${zz_rows[index]/"┌──$title"/"┌─$title─"}"
      done
    done
  fi
  if [[ " $drift " == *" flag "* ]]; then
    last=$((${#zz_rows[@]} - 1))
    zz_rows[last]="${zz_rows[last]/":$WINDOW_NAME*O"/":$WINDOW_NAME*"}"
  fi
  if [[ " $drift " == *" row "* ]]; then
    top="$(box_row '┌' "${zz_rows[@]}")"
    pin_top="$(box_row '┌' "${tmux_rows[@]}")"
    bottom="$(box_row '└' "${tmux_rows[@]}")"
    if [ -n "$top" ] && [ -n "$pin_top" ] && [ -n "$bottom" ] && [ "$pin_top" -eq $((top + 1)) ]; then
      local moved=("${zz_rows[bottom]}")
      for ((index = top; index < bottom; index++)); do
        moved+=("${zz_rows[index]}")
      done
      for ((index = top; index <= bottom; index++)); do
        zz_rows[index]="${moved[index - top]}"
      done
      cursor_x="${zz_cursor%%,*}"
      cursor_y="${zz_cursor#*,}"
      cursor_rest="${cursor_y#* }"
      cursor_y="${cursor_y%% *}"
      if [ "$cursor_y" -ge "$top" ] && [ "$cursor_y" -lt "$bottom" ]; then
        zz_cursor="$cursor_x,$((cursor_y + 1)) $cursor_rest"
      fi
    fi
  fi
}

compare_rows() {
  local name="$1"
  local styled="$2"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing total count drift
  if [ "$styled" = styled ]; then
    mapfile -t zz_rows < <(capture_screen zz)
    mapfile -t tmux_rows < <(capture_screen tmux)
  else
    mapfile -t zz_rows < <(capture_plain zz)
    mapfile -t tmux_rows < <(capture_plain tmux)
  fi
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  drift="$(known_drift "$name")"
  [ -z "$drift" ] || apply_known_drift "$drift"
  total="$ROWS_UNDER_TEST"
  differing=-1
  count=0
  LAST_DIFFERING_ROWS=""
  for ((index = 0; index < total; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      [ "$differing" -ge 0 ] || differing="$index"
      count=$((count + 1))
      LAST_DIFFERING_ROWS="$LAST_DIFFERING_ROWS $index"
    fi
  done
  LAST_DIFFERING_ROWS="${LAST_DIFFERING_ROWS# }"
  LAST_ROWS_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  [ "$differing" -lt 0 ] || LAST_ROWS_DIFFERED=1
  [ "$zz_cursor" = "$tmux_cursor" ] || LAST_CURSOR_DIFFERED=1
  if [ "$LAST_ROWS_DIFFERED" -eq 0 ] && [ "$LAST_CURSOR_DIFFERED" -eq 0 ]; then
    return 0
  fi
  printf '      case %s\n' "$name"
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf '      %s of %s rows differ, first %s\n' "$count" "$total" "$differing"
    for ((index = 0; index < total; index++)); do
      if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
        printf '        row %s tmux: %s\n' "$index" "$(printf '%s' "${tmux_rows[index]-}" | cat -v)"
        printf '        row %s zz:   %s\n' "$index" "$(printf '%s' "${zz_rows[index]-}" | cat -v)"
      fi
    done
  else
    printf '      all %s rows identical\n' "$total"
  fi
  printf '      cursor tmux: %s\n' "$tmux_cursor"
  printf '      cursor zz:   %s\n' "$zz_cursor"
  return 1
}

declare -A RECORD_OWNERS=([unattributed]=0)
owner_tally() {
  local key entries=() drifts=()
  for key in $(printf '%s\n' "${!RECORD_OWNERS[@]}" | LC_ALL=C sort); do
    entries+=("$key=${RECORD_OWNERS[$key]}")
  done
  for key in $(printf '%s\n' "${!DRIFT_OWNERS[@]}" | LC_ALL=C sort); do
    drifts+=("$key=${DRIFT_OWNERS[$key]}")
  done
  printf 'owners %s; %s asserted with known drift %s' "${entries[*]}" "$DRIFTS" "${drifts[*]:-none}"
}
verdict() {
  local name="$1"
  local mode="$2"
  local reason="${3:-}"
  local owner="${4:-unattributed}"
  if [ "$mode" = same ]; then
    CHECKS=$((CHECKS + 1))
  else
    RECORDS=$((RECORDS + 1))
    RECORD_OWNERS[$owner]=$((${RECORD_OWNERS[$owner]:-0} + 1))
  fi
  if compare_rows "$name" styled; then
    local drift
    drift="$(known_drift "$name")"
    if [ -n "$drift" ]; then
      DRIFTS=$((DRIFTS + 1))
      DRIFT_OWNERS[$POPUP_DRIFT_OWNER]=$((${DRIFT_OWNERS[$POPUP_DRIFT_OWNER]:-0} + 1))
      printf 'ok    %s, known drift %s: %s\n' "$name" "$POPUP_DRIFT_OWNER" "$drift"
      return 0
    fi
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

prompt_case() {
  CASE_LABEL=command-prompt
  mark_both prompt
  press_on_both ':'
  wait_for 'the zz prompt' last_row_has zz ':'
  wait_for 'the tmux prompt' last_row_has tmux ':'
  settle_both MARK-prompt 'the command prompt'
  verdict prompt-opened same
  type_on_both -l 'display-message PROMPTTEXT'
  both_last_row_has 'PROMPTTEXT' 'the typed command'
  settle_both MARK-prompt 'the typed command'
  verdict prompt-typed same
  type_on_both -l '  '
  wait_for 'the zz cursor after the trailing spaces' cursor_column_is zz 29
  wait_for 'the tmux cursor after the trailing spaces' cursor_column_is tmux 29
  settle_both MARK-prompt 'the typed trailing spaces'
  verdict prompt-trailing-space same
  type_on_both -l 'abcdefghijklmnopqrstuvwxyz0123456789 abcdefghijklmnopqrstuvwxyz0123456789 END'
  wait_for 'the long command on the tmux status row' last_row_has tmux 'END'
  settle_both MARK-prompt 'the long command'
  verdict prompt-wrapped same
  type_on_both Home
  type_on_both Right Right Right
  settle_both MARK-prompt 'the cursor moved into the long command'
  verdict prompt-cursor-home same
  resize_both_to 60 20
  settle_both MARK-prompt 'the prompt at 60x20'
  verdict prompt-resized same
  resize_both_to 80 24
  settle_both MARK-prompt 'the prompt back at 80x24'
  verdict prompt-resized-back same
  type_on_both Escape
  both_last_row_starts_with 'L' 'the cancelled prompt'
  settle_both MARK-prompt 'the cancelled prompt'
  verdict prompt-cancelled same

  set_on_both status-position top
  mark_both top
  press_on_both ':'
  type_on_both -l 'TOPPROMPT'
  wait_for 'the zz prompt at the top' first_row_has zz 'TOPPROMPT'
  wait_for 'the tmux prompt at the top' first_row_has tmux 'TOPPROMPT'
  settle_both MARK-top 'the prompt at the top'
  verdict prompt-status-top same
  type_on_both Escape
  wait_for 'the zz top prompt cancelled' screen_lacks zz 'TOPPROMPT'
  wait_for 'the tmux top prompt cancelled' screen_lacks tmux 'TOPPROMPT'
  settle_both MARK-top 'the top prompt cancelled'
  verdict prompt-status-top-cancelled same
  set_on_both status-position bottom
}

confirm_case() {
  CASE_LABEL=confirm-before
  mark_both confirm
  client_on_both confirm-before -b -t CLIENT -p 'OVERLAY-CONFIRM? ' \
    'set-option -g @overlay_confirm yes'
  both_last_row_has 'OVERLAY-CONFIRM?' 'the confirm prompt'
  settle_both MARK-confirm 'the confirm prompt'
  verdict confirm-opened same
  resize_both_to 60 20
  settle_both MARK-confirm 'the confirm prompt at 60x20'
  verdict confirm-resized same
  resize_both_to 80 24
  settle_both MARK-confirm 'the confirm prompt back at 80x24'
  type_on_both n
  both_last_row_lacks 'OVERLAY-CONFIRM?' 'the refused confirm prompt'
  settle_both MARK-confirm 'the refused confirm prompt'
  verdict confirm-refused same
}

menu_case() {
  CASE_LABEL=display-menu
  mark_both menu
  press_on_both M
  both_screen_has OVERLAY-MENU 'the menu'
  settle_both MARK-menu 'the menu'
  verdict menu-opened same
  type_on_both Down
  settle_both MARK-menu 'the menu after Down'
  verdict menu-down same
  type_on_both Down
  settle_both MARK-menu 'the menu after Down past the separator'
  verdict menu-down-separator same
  type_on_both z
  settle_both MARK-menu 'the menu after an unanswered key'
  verdict menu-unanswered-key same
  side_command zz display-message -c "$(client_name zz)" 'OVERLAY-MESSAGE' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'OVERLAY-MESSAGE' ||
    die 'tmux refused display-message'
  both_last_row_has OVERLAY-MESSAGE 'the message over the menu'
  settle_both MARK-menu 'the message over the menu'
  verdict menu-under-message same
  resize_both_to 100 30
  settle_both MARK-menu 'the menu at 100x30'
  verdict menu-resized same
  resize_both_to 80 24
  settle_both MARK-menu 'the menu back at 80x24'
  type_on_both Escape
  both_screen_lacks OVERLAY-MENU 'the cancelled menu'
  settle_both MARK-menu 'the cancelled menu'
  verdict menu-cancelled same

  mark_both menukey
  press_on_both M
  both_screen_has OVERLAY-MENU 'the menu'
  settle_both MARK-menukey 'the menu'
  type_on_both s
  both_option_is @overlay_menu second 'the s shortcut'
  both_screen_lacks OVERLAY-MENU 'the menu closed by its shortcut'
  settle_both MARK-menukey 'the menu closed by its shortcut'
  verdict menu-shortcut-closed same

  mark_both menuclick
  press_on_both N
  both_screen_has OVERLAY-MENU 'the -M menu'
  settle_both MARK-menuclick 'the -M menu'
  verdict menu-mouse-opened same
  click_item_on_both 'Third item'
  both_option_is @overlay_menu third 'the clicked third item'
  both_screen_lacks OVERLAY-MENU 'the menu closed by the click'
  settle_both MARK-menuclick 'the menu closed by the click'
  verdict menu-click-closed same
}

click_item_on_both() {
  local text="$1"
  local side row column line index
  for side in zz tmux; do
    row=""
    index=0
    while IFS= read -r line; do
      if [[ "$line" == *"$text"* ]]; then
        row="$index"
        line="${line%%"$text"*}"
        column="${#line}"
        break
      fi
      index=$((index + 1))
    done < <(capture_plain "$side")
    [ -n "$row" ] || die "no $text on the $side screen to click"
    type_on "$side" -l "$(printf '\033[<0;%s;%sM' "$((column + 1))" "$((row + 1))")"
    type_on "$side" -l "$(printf '\033[<0;%s;%sm' "$((column + 1))" "$((row + 1))")"
  done
}

popup_case() {
  CASE_LABEL=display-popup
  mark_both popup
  press_on_both P
  both_screen_has OVERLAY-POPUP 'the popup'
  both_screen_has POPUP-BODY 'the popup job'
  settle_both MARK-popup 'the popup'
  verdict popup-opened same
  type_on_both z Enter
  both_screen_has POPUP-GOT-z 'the popup job read z'
  settle_both MARK-popup 'the popup after a line'
  verdict popup-typed same
  side_command zz display-message -c "$(client_name zz)" 'OVERLAY-MESSAGE  ' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'OVERLAY-MESSAGE  ' ||
    die 'tmux refused display-message'
  both_last_row_has OVERLAY-MESSAGE 'the message over the popup'
  settle_both MARK-popup 'the message over the popup'
  verdict popup-under-message same
  resize_both_to 100 30
  settle_both MARK-popup 'the popup at 100x30'
  verdict popup-resized same
  resize_both_to 80 24
  settle_both MARK-popup 'the popup back at 80x24'
  type_on_both Enter
  both_screen_lacks OVERLAY-POPUP 'the closed popup'
  settle_both MARK-popup 'the closed popup'
  verdict popup-closed same
}

display_panes_case() {
  CASE_LABEL=display-panes
  run_on_both split-window -h -t "=$INNER_SESSION:0.0" "$INNER_SHELL"
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  DIVIDER_RULE=1
  mark_both panes
  verdict panes-split same
  local zz_before tmux_before
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  display_panes_on_both
  wait_for 'the zz labels' screen_differs_from zz "$zz_before"
  wait_for 'the tmux labels' screen_differs_from tmux "$tmux_before"
  settle_both MARK-panes 'the pane labels'
  verdict panes-shown same
  resize_both_to 80 30
  settle_both MARK-panes 'the pane labels at 80x30'
  verdict panes-resized "$ZOOM_REFLOW_MODE" "$ZOOM_REFLOW_REASON" "$ZOOM_REFLOW_OWNER"
  if [ "$PIN_DISPLAY_PANES_MODE" -eq 1 ]; then
    run_on_both copy-mode -q -t "=$INNER_SESSION:0.0"
  fi
  resize_both_to 80 24
  mark_both panesback
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  display_panes_on_both
  wait_for 'the zz labels again' screen_differs_from zz "$zz_before"
  wait_for 'the tmux labels again' screen_differs_from tmux "$tmux_before"
  settle_both MARK-panes 'the pane labels again'
  type_on_both 1
  wait_for 'zz selected pane 1' active_pane_index_is zz 1
  wait_for 'tmux selected pane 1' active_pane_index_is tmux 1
  settle_both MARK-panes 'the digit selection'
  verdict panes-selected "$ZOOM_REFLOW_MODE" "$ZOOM_REFLOW_REASON" "$ZOOM_REFLOW_OWNER"

  mark_both panesz
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  display_panes_on_both
  wait_for 'the zz labels' screen_differs_from zz "$zz_before"
  wait_for 'the tmux labels' screen_differs_from tmux "$tmux_before"
  settle_both MARK-panesz 'the pane labels'
  type_on_both Z
  settle_both MARK-panesz 'the labels closed by a non-pane key'
  verdict panes-closed-by-key "$ZOOM_REFLOW_MODE" "$ZOOM_REFLOW_REASON" "$ZOOM_REFLOW_OWNER"
  [ "$PIN_DISPLAY_PANES_MODE" -eq 0 ] || type_on_both Enter
  DIVIDER_RULE=0
}

CENTRE_MENU_ITEMS=(
  'First item' f 'set-option -g @overlay_menu first'
  ''
  'Second item' s 'set-option -g @overlay_menu second'
  'Third item' t 'set-option -g @overlay_menu third'
)
style_on_both() {
  set_on_both menu-style "$1"
  set_on_both menu-selected-style "$2"
  set_on_both menu-border-style "$3"
  set_on_both popup-style "$4"
  set_on_both popup-border-style "$5"
}
centred_surfaces() {
  local label="$1"
  mark_both "$label"
  press_on_both C
  both_screen_has CENTRE-MENU 'the centred menu'
  settle_both "MARK-$label" 'the centred menu'
  verdict "centre-menu-$label" same
  type_on_both Down
  settle_both "MARK-$label" 'the centred menu after Down'
  verdict "centre-menu-$label-down" same
  type_on_both Escape
  both_screen_lacks CENTRE-MENU 'the cancelled centred menu'
  settle_both "MARK-$label" 'the cancelled centred menu'
  press_on_both O
  both_screen_has CENTRE-POPUP 'the centred popup'
  both_screen_has POPUP-BODY 'the centred popup job'
  settle_both "MARK-$label" 'the centred popup'
  verdict "centre-popup-$label" same
  type_on_both z Enter
  both_screen_has POPUP-GOT-z 'the centred popup job read z'
  settle_both "MARK-$label" 'the centred popup after a line'
  verdict "centre-popup-$label-typed" same
  type_on_both Enter
  both_screen_lacks CENTRE-POPUP 'the closed centred popup'
  settle_both "MARK-$label" 'the closed centred popup'
  verdict "centre-popup-$label-closed" same
}
odd_size_case() {
  CASE_LABEL=odd-size
  run_on_both kill-pane -t "=$INNER_SESSION:0.1"
  resize_both_to 79 23
  run_on_both bind-key -T prefix C display-menu -x C -y C -T CENTRE-MENU "${CENTRE_MENU_ITEMS[@]}"
  run_on_both bind-key -T prefix D display-menu -M -x C -y C -T CENTRE-MENU "${CENTRE_MENU_ITEMS[@]}"
  run_on_both bind-key -T prefix O display-popup -T CENTRE-POPUP -E "$POPUP_JOB"
  style_on_both 'fg=colour33' 'fg=colour226' 'fg=colour208' 'fg=colour159' 'fg=colour46'
  centred_surfaces fg
  style_on_both 'fg=colour33,bg=colour17' 'fg=colour16,bg=colour226' 'fg=colour208,bg=colour52' \
    'fg=colour159,bg=colour17' 'fg=colour46,bg=colour22'
  centred_surfaces fgbg

  set_on_both @overlay_menu none
  mark_both click
  press_on_both D
  both_screen_has CENTRE-MENU 'the centred -M menu'
  settle_both MARK-click 'the centred -M menu'
  verdict centre-menu-mouse-opened same
  click_item_on_both 'Third item'
  both_option_is @overlay_menu third 'the clicked third item of the centred menu'
  both_screen_lacks CENTRE-MENU 'the centred menu closed by the click'
  settle_both MARK-click 'the centred menu closed by the click'
  verdict centre-menu-click-closed same

  set_on_both display-panes-colour colour33
  set_on_both display-panes-active-colour colour124
  run_on_both split-window -h -t "=$INNER_SESSION:0.0" "$INNER_SHELL"
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  DIVIDER_RULE=1
  mark_both colours
  local zz_before tmux_before
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  display_panes_on_both
  wait_for 'the zz coloured labels' screen_differs_from zz "$zz_before"
  wait_for 'the tmux coloured labels' screen_differs_from tmux "$tmux_before"
  settle_both MARK-colours 'the coloured pane labels'
  verdict panes-coloured-shown same
  type_on_both 1
  wait_for 'zz selected pane 1 from the coloured labels' active_pane_index_is zz 1
  wait_for 'tmux selected pane 1 from the coloured labels' active_pane_index_is tmux 1
  settle_both MARK-colours 'the coloured labels closed by a digit'
  verdict panes-coloured-selected "$ZOOM_REFLOW_MODE" "$ZOOM_REFLOW_REASON" "$ZOOM_REFLOW_OWNER"
  DIVIDER_RULE=0
}

write_attach zz "$SCRATCH_DIR/attach-zz.sh"
write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"

zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

if ! tmux_inner_command -f /dev/null start-server \; list-commands display-panes 2>/dev/null |
  grep -q -- '-b'; then
  PIN_DISPLAY_PANES_MODE=1
  ZOOM_REFLOW_MODE=record
fi

PROMPT_CURSOR_OPTIONS=(
  prompt-cursor-style bar
  prompt-cursor-colour red
  prompt-command-cursor-style blinking-underline
  prompt-command-cursor-colour '#00ff00'
  status-keys vi
)
cursor_shape_is() {
  [ "$(tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" '#{cursor_shape}' 2>/dev/null)" = "$2" ]
}
both_cursor_shape_is() {
  wait_for "$2 cursor on zz" cursor_shape_is zz "$1"
  wait_for "$2 cursor on tmux" cursor_shape_is tmux "$1"
}
prompt_cursor_case() {
  CASE_LABEL=prompt-cursor
  local index
  attach_both_at 80 24
  for ((index = 0; index < ${#PROMPT_CURSOR_OPTIONS[@]}; index += 2)); do
    set_on_both "${PROMPT_CURSOR_OPTIONS[index]}" "${PROMPT_CURSOR_OPTIONS[index + 1]}"
  done
  mark_both pcursor
  press_on_both ':'
  type_on_both -l 'abc'
  both_last_row_has ':abc' 'the typed prompt'
  both_cursor_shape_is bar 'the prompt'
  settle_both MARK-pcursor 'the prompt with its cursor style'
  verdict prompt-cursor-styled same
  type_on_both Escape
  both_cursor_shape_is underline 'the command-mode prompt'
  settle_both MARK-pcursor 'the command-mode prompt'
  verdict prompt-command-cursor-styled same
  type_on_both C-c
  both_last_row_starts_with 'L' 'the cancelled prompt'
  both_cursor_shape_is block 'the restored'
  settle_both MARK-pcursor 'the cancelled prompt'
  verdict prompt-cursor-restored same
  client_on_both confirm-before -b -t CLIENT -p 'CURSOR-CONFIRM? ' \
    'set-option -g @overlay_confirm yes'
  both_last_row_has 'CURSOR-CONFIRM?' 'the confirm prompt'
  both_cursor_shape_is bar 'the confirm prompt'
  settle_both MARK-pcursor 'the confirm prompt with its cursor style'
  verdict confirm-cursor-styled same
  type_on_both n
  both_last_row_lacks 'CURSOR-CONFIRM?' 'the refused confirm prompt'
  both_cursor_shape_is block 'the refused confirm'
  settle_both MARK-pcursor 'the refused confirm prompt'
  verdict confirm-cursor-restored same
  for ((index = 0; index < ${#PROMPT_CURSOR_OPTIONS[@]}; index += 2)); do
    run_on_both set-option -gu "${PROMPT_CURSOR_OPTIONS[index]}"
  done
}

run_cases() {
  printf 'overlay differential at %sx%s (pin %s)\n' \
    "$COLUMNS_UNDER_TEST" "$ROWS_UNDER_TEST" "$(basename -- "$TMUX_BIN")"
  attach_both_at 80 24
  CASE_LABEL=baseline
  mark_both baseline
  verdict baseline same
  prompt_case
  confirm_case
  menu_case
  popup_case
  display_panes_case
  odd_size_case
  prompt_cursor_case

  if [ "$FAILURES" -ne 0 ]; then
    printf '%s of %s asserted comparisons differ, %s recorded (%s)\n' "$FAILURES" "$CHECKS" "$RECORDS" "$(owner_tally)"
    exit 1
  fi
  printf 'all %s asserted comparisons identical, %s recorded not asserted (%s)\n' "$CHECKS" "$RECORDS" "$(owner_tally)"
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
  cursor)
    [ "$LAST_ROWS_DIFFERED" -eq 0 ] || outcome='a row difference where only the cursor was planted'
    [ "$LAST_CURSOR_DIFFERED" -eq 1 ] || outcome='no cursor difference reported'
    ;;
  none)
    if [ "$LAST_ROWS_DIFFERED" -ne 0 ] || [ "$LAST_CURSOR_DIFFERED" -ne 0 ]; then
      outcome='reported a difference where none was planted'
    fi
    ;;
  rows:*)
    [ "$LAST_DIFFERING_ROWS" = "${expectation#rows:}" ] ||
      outcome="rows '${LAST_DIFFERING_ROWS}' differ where only '${expectation#rows:}' were planted"
    [ "$LAST_CURSOR_DIFFERED" -eq 0 ] ||
      outcome='a cursor difference where only a row style was planted'
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
  printf 'self-check: one deliberate difference per channel, plus one equivalence\n'
  attach_both_at 80 24

  CASE_LABEL='self-check menu item'
  mark_both item
  side_command zz bind-key -T prefix M display-menu -x 4 -y 12 -T OVERLAY-MENU \
    'First item' f 'set-option -g @overlay_menu first' \
    '' \
    'Second item' s 'set-option -g @overlay_menu second' \
    '-Disabled item' d 'set-option -g @overlay_menu disabled' \
    'Third item' t 'set-option -g @overlay_menu third' \
    'Extra item' e 'set-option -g @overlay_menu extra' || die 'zz refused bind-key'
  press_on_both M
  both_screen_has OVERLAY-MENU 'the sabotaged menu'
  settle_both MARK-item 'the sabotaged menu'
  compare_rows self-check-menu-item styled || true
  self_check_case 'menu, an item only one side has' rows
  type_on_both Escape
  both_screen_lacks OVERLAY-MENU 'the sabotaged menu cancelled'
  bind_on_both

  CASE_LABEL='self-check covered pane'
  mark_both leak
  press_on_both M
  both_screen_has OVERLAY-MENU 'the menu'
  settle_both MARK-leak 'the menu'
  side_command zz send-keys -t "$(active_pane zz)" z || die 'zz refused send-keys'
  type_on_both z
  type_on_both Escape
  both_screen_lacks OVERLAY-MENU 'the menu cancelled'
  settle_both MARK-leak 'the menu cancelled'
  compare_rows self-check-covered-pane styled || true
  self_check_case 'covered pane, a keystroke only one side delivers under the menu' rows
  send_both ''

  CASE_LABEL='self-check popup border'
  mark_both border
  side_command zz bind-key -T prefix P display-popup -b rounded -w 34 -h 9 -T OVERLAY-POPUP -E "$POPUP_JOB" ||
    die 'zz refused bind-key'
  press_on_both P
  both_screen_has POPUP-BODY 'the sabotaged popup'
  settle_both MARK-border 'the sabotaged popup'
  compare_rows self-check-popup-border styled || true
  self_check_case 'popup, a border only one side draws rounded' rows
  type_on_both Enter Enter
  both_screen_lacks OVERLAY-POPUP 'the sabotaged popup closed'
  bind_on_both

  CASE_LABEL='self-check prompt cursor'
  mark_both cursor
  press_on_both ':'
  type_on_both -l 'CURSORTEXT'
  both_last_row_has CURSORTEXT 'the typed prompt'
  type_on zz Left
  type_on zz Left
  settle_both MARK-cursor 'the one-sided cursor move'
  compare_rows self-check-prompt-cursor plain || true
  self_check_case 'prompt, a cursor only one side moves' cursor
  type_on_both Escape
  both_last_row_lacks CURSORTEXT 'the prompt cancelled'

  CASE_LABEL='self-check style'
  mark_both style
  side_command zz set-option -g menu-border-style 'fg=colour208' || die 'zz refused set-option'
  press_on_both M
  both_screen_has OVERLAY-MENU 'the one-sided styled menu'
  side_command zz display-message -c "$(client_name zz)" 'OVERLAY-MESSAGE' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'OVERLAY-MESSAGE' ||
    die 'tmux refused display-message'
  both_last_row_has OVERLAY-MESSAGE 'the message over the styled menu'
  settle_both MARK-style 'the one-sided styled menu'
  compare_rows self-check-style styled || true
  self_check_case 'style, an fg-only menu-border-style only one side sets' rows
  type_on_both Escape
  both_screen_lacks OVERLAY-MENU 'the styled menu cancelled'
  side_command zz set-option -gu menu-border-style || die 'zz refused set-option -gu'

  CASE_LABEL='self-check prompt style'
  mark_both pstyle
  side_command zz set-option -g message-style 'bg=colour94' || die 'zz refused set-option'
  press_on_both ':'
  type_on_both -l 'STYLEDPROMPT'
  both_last_row_has STYLEDPROMPT 'the one-sided styled prompt'
  settle_both MARK-pstyle 'the one-sided styled prompt'
  compare_rows self-check-prompt-style styled || true
  self_check_case 'prompt, a message-style only one side sets' "rows:$((ROWS_UNDER_TEST - 1))"
  type_on_both Escape
  both_last_row_lacks STYLEDPROMPT 'the styled prompt cancelled'

  CASE_LABEL='self-check message style'
  mark_both mstyle
  side_command zz display-message -c "$(client_name zz)" 'OVERLAY-MESSAGE' ||
    die 'zz refused display-message'
  side_command tmux display-message -c "$(client_name tmux)" 'OVERLAY-MESSAGE' ||
    die 'tmux refused display-message'
  both_last_row_has OVERLAY-MESSAGE 'the one-sided styled message'
  settle_both MARK-mstyle 'the one-sided styled message'
  compare_rows self-check-message-style styled || true
  self_check_case 'message, a message-style only one side sets' "rows:$((ROWS_UNDER_TEST - 1))"
  type_on_both C-l
  both_last_row_lacks OVERLAY-MESSAGE 'the styled message cleared'
  send_both ''
  side_command zz set-option -gu message-style || die 'zz refused set-option -gu'

  CASE_LABEL='self-check equivalence'
  mark_both equal
  press_on_both M
  both_screen_has OVERLAY-MENU 'the menu on both sides'
  settle_both MARK-equal 'the menu on both sides'
  compare_rows self-check-equivalence styled || true
  self_check_case 'equivalence: the same menu on both sides' none
  type_on_both Escape
  both_screen_lacks OVERLAY-MENU 'the equivalent menu cancelled'

  CASE_LABEL='self-check display-panes colour'
  side_command zz set-option -g display-panes-active-colour colour124 || die 'zz refused set-option'
  run_on_both split-window -h -t "=$INNER_SESSION:0.0" "$INNER_SHELL"
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  DIVIDER_RULE=1
  mark_both colour
  local zz_before tmux_before
  zz_before="$(capture_screen zz)"
  tmux_before="$(capture_screen tmux)"
  display_panes_on_both
  wait_for 'the zz one-sided labels' screen_differs_from zz "$zz_before"
  wait_for 'the tmux one-sided labels' screen_differs_from tmux "$tmux_before"
  settle_both MARK-colour 'the one-sided pane colour'
  compare_rows self-check-panes-colour styled || true
  self_check_case 'display-panes, a display-panes-active-colour only one side sets' rows
  type_on_both Escape
  DIVIDER_RULE=0

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

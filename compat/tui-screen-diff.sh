#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-screen-diff.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-screen-diff.sh\n' >&2
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

SIZES=(80x24\|same\|120 100x24\|same\|80 80x10\|same\|100 80x6\|same\|100 109x24\|same\|120 120x24\|same\|100)
PANE_TITLE="screentitle"
WINDOW_NAME="win"
SCRATCH_DIR="$(mktemp -d /tmp/zzsd.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzsdo-$TOKEN"
INNER_SOCKET_NAME="zzsdi-$TOKEN"
ZZ_SOCKET="/tmp/zzsd-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="screen"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
ZZ_CLIENT_STDERR="$SCRATCH_DIR/zz-client.err"
TMUX_CLIENT_STDERR="$SCRATCH_DIR/tmux-client.err"
CAPTURE_DIR="${ZZ_SCREEN_CAPTURE_DIR:-}"
DIAGNOSTICS_DIR=""
COLUMNS_UNDER_TEST=0
ROWS_UNDER_TEST=0
SIZE_LABEL=""
ZZ_PID=""
FAILURES=0
CHECKS=0
RECORDS=0
LAST_ROWS_DIFFERED=0
LAST_CURSOR_DIFFERED=0
mkdir -p "$ZZ_HOME" "$TMUX_HOME" "$OUTER_HOME" "$ZZ_LOG_DIR"
[ -z "$CAPTURE_DIR" ] || mkdir -p "$CAPTURE_DIR"

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
side_home() {
  case "$1" in
  zz) printf '%s\n' "$ZZ_HOME" ;;
  tmux) printf '%s\n' "$TMUX_HOME" ;;
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

diagnostics_dir() {
  if [ -z "$DIAGNOSTICS_DIR" ]; then
    DIAGNOSTICS_DIR="${ZZ_SCREEN_DIAGNOSTICS_DIR:-$(mktemp -d /tmp/zzsd-diag.XXXXXX)}"
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
    printf 'size under test: %s\n' "${SIZE_LABEL:-none yet}"
    printf 'zz: %s\n' "$ZZ_BIN"
    printf 'tmux: %s\n' "$TMUX_BIN"
    printf 'zz socket: %s\n' "$ZZ_SOCKET"
    printf 'outer socket: %s\n' "$OUTER_SOCKET_NAME"
    printf 'inner tmux socket: %s\n' "$INNER_SOCKET_NAME"
    printf 'ZZ_LOG_DIR (%s) holds:\n' "$ZZ_LOG_DIR"
    ls -1 -- "$ZZ_LOG_DIR" 2>&1 || true
  } >"$dir/what-fired.txt" 2>&1 || true
  for side in zz tmux; do
    tmux_outer_command capture-pane -p -e -S - -t "=$OUTER_SESSION:$side" \
      >"$dir/outer-$side.screen.txt" 2>&1 || true
    tmux_outer_command display-message -p -t "=$OUTER_SESSION:$side" \
      "$CURSOR_FORMAT" >"$dir/outer-$side.cursor.txt" 2>&1 || true
    side_command "$side" list-clients \
      -F '#{client_name} session=#{client_session} #{client_width}x#{client_height} flags=#{client_flags}' \
      >"$dir/$side.list-clients.txt" 2>&1 || true
    side_command "$side" list-panes -a \
      -F '#{session_name}:#{window_index}.#{pane_index} #{pane_width}x#{pane_height} dead=#{pane_dead}' \
      >"$dir/$side.list-panes.txt" 2>&1 || true
  done
  cp -f -- "$SCRATCH_DIR/zz-daemon.out" "$dir/zz-daemon.stdout.txt" 2>/dev/null || true
  cp -f -- "$SCRATCH_DIR/zz-daemon.err" "$dir/zz-daemon.stderr.txt" 2>/dev/null || true
  cp -f -- "$ZZ_CLIENT_STDERR" "$dir/zz-client.stderr.txt" 2>/dev/null || true
  cp -f -- "$TMUX_CLIENT_STDERR" "$dir/tmux-client.stderr.txt" 2>/dev/null || true
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

CURSOR_ASSERTED_FORMAT='#{cursor_x},#{cursor_y} flag=#{cursor_flag} pane=#{pane_width}x#{pane_height} shape=#{cursor_shape} blinking=#{cursor_blinking} very_visible=#{cursor_very_visible} colour=#{cursor_colour}'
CURSOR_FORMAT="$CURSOR_ASSERTED_FORMAT"

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" = "$INNER_SESSION" ]
}

capture_screen() {
  tmux_outer_command capture-pane -p -e -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
capture_plain() {
  tmux_outer_command capture-pane -p -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
cursor_tuple() {
  tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" "$CURSOR_ASSERTED_FORMAT"
}

write_attach() {
  local side="$1"
  local destination="$2"
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp %q --socket %q attach-session -t %q 2> >(tee -a %q >&2)\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" "=$INNER_SESSION" "$ZZ_CLIENT_STDERR" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp %q -L %q attach-session -t %q 2> >(tee -a %q >&2)\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" "=$INNER_SESSION" "$TMUX_CLIENT_STDERR" >>"$destination"
  fi
  chmod +x "$destination"
}

set_on_both() {
  side_command zz set-option -g "$1" "$2" || die "zz refused set-option -g $1"
  side_command tmux set-option -g "$1" "$2" || die "tmux refused set-option -g $1"
}
active_pane() {
  side_command "$1" list-panes -t "=$INNER_SESSION" -F '#{pane_active} #{pane_id}' |
    awk '$1 == 1 { print $2; exit }'
}
pin_pane_titles() {
  local side pane
  for side in zz tmux; do
    while read -r pane; do
      [ -n "$pane" ] || continue
      side_command "$side" select-pane -t "$pane" -T "$PANE_TITLE" ||
        die "$side refused select-pane -T"
    done < <(side_command "$side" list-panes -t "=$INNER_SESSION" -F '#{pane_id}')
  done
}

clear_both() {
  send_both "printf '\\033[2J\\033[3J\\033[H'"
}
send_both() {
  local pane
  pane="$(active_pane zz)"
  [ -n "$pane" ] || die "zz has no active pane"
  zz_command send-keys -t "$pane" "$1" Enter || die "zz refused send-keys"
  pane="$(active_pane tmux)"
  [ -n "$pane" ] || die "tmux has no active pane"
  tmux_inner_command send-keys -t "$pane" "$1" Enter || die "tmux refused send-keys"
}
run_on_both_active() {
  local side pane
  for side in zz tmux; do
    pane="$(active_pane "$side")"
    [ -n "$pane" ] || die "$side has no active pane"
    side_command "$side" "$1" -t "$pane" "${@:2}" || die "$side refused $1"
  done
}
run_on_both() {
  side_command zz "$@" || die "zz refused $1"
  side_command tmux "$@" || die "tmux refused $1"
}

OWNED_OPTIONS=(
  status status-position status-style status-left status-right status-justify
  status-left-length status-right-length
  window-status-format window-status-current-format default-command
  automatic-rename pane-border-status pane-border-format pane-border-lines
  pane-border-style pane-active-border-style
)
reset_owned_options() {
  local side="$1"
  local option
  for option in "${OWNED_OPTIONS[@]}"; do
    side_command "$side" set-option -gu "$option" >/dev/null 2>&1 || true
  done
}

INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"

pin_dynamic_values() {
  local side="$1"
  reset_owned_options "$side"
  side_command "$side" set-option -g status-right '' || die "$side refused status-right"
  side_command "$side" set-option -g status-left L || die "$side refused status-left"
  side_command "$side" set-option -g automatic-rename off || die "$side refused automatic-rename"
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
  pin_dynamic_values zz
  pin_dynamic_values tmux
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
pane_holds_marker() {
  local pane
  pane="$(active_pane "$1")"
  [ -n "$pane" ] || return 1
  side_command "$1" capture-pane -p -S -20 -t "$pane" 2>/dev/null | grep -Fq "$2"
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
      { printf '%s' "$current" | grep -Fq "$marker" || pane_holds_marker "$side" "$marker"; }; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  dump_diagnostics "$label"
  die "$label did not settle within 10 seconds"
}
settle_both() {
  wait_settled zz "$1" "$2 settled on the zz screen"
  wait_settled tmux "$1" "$2 settled on the tmux screen"
}


compare_screens() {
  local name="$1"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing total
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  if [ -n "$CAPTURE_DIR" ]; then
    printf '%s\n' "${zz_rows[@]-}" >"$CAPTURE_DIR/$SIZE_LABEL.$name.zz.screen.txt"
    printf '%s\n' "${tmux_rows[@]-}" >"$CAPTURE_DIR/$SIZE_LABEL.$name.tmux.screen.txt"
    {
      printf 'zz:   %s\n' "$zz_cursor"
      printf 'tmux: %s\n' "$tmux_cursor"
    } >"$CAPTURE_DIR/$SIZE_LABEL.$name.cursor.txt"
  fi
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
  printf '      checkpoint %s at %s\n' "$name" "$SIZE_LABEL"
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

compare_plain_screens() {
  local name="$1"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing total
  mapfile -t zz_rows < <(capture_plain zz)
  mapfile -t tmux_rows < <(capture_plain tmux)
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
  if [ "$differing" -lt 0 ] && [ "$zz_cursor" = "$tmux_cursor" ]; then
    return 0
  fi
  printf '      checkpoint %s at %s, text and cursor\n' "$name" "$SIZE_LABEL"
  if [ "$differing" -ge 0 ]; then
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

declare -A RECORD_OWNERS=([unattributed]=0)

owner_tally() {
  local key entries=()
  for key in $(printf '%s\n' "${!RECORD_OWNERS[@]}" | LC_ALL=C sort); do
    entries+=("$key=${RECORD_OWNERS[$key]}")
  done
  printf 'owners %s' "${entries[*]}"
}

checkpoint() {
  local name="$1"
  local mode="$2"
  local reason="${3:-}"
  local owner="${4:-unattributed}"
  send_both "printf 'MARK-%s\\n' $name"
  settle_both "MARK-$name" "$name"
  if [ "$mode" = text ]; then
    CHECKS=$((CHECKS + 1))
    RECORDS=$((RECORDS + 1))
    RECORD_OWNERS[$owner]=$((${RECORD_OWNERS[$owner]:-0} + 1))
    [ -n "$reason" ] || die "recorded style at $name says nothing about why"
    if compare_plain_screens "$name"; then
      printf 'ok    %s %s every glyph, column and the cursor identical\n' "$SIZE_LABEL" "$name"
    else
      FAILURES=$((FAILURES + 1))
      printf 'DIFF  %s %s\n' "$SIZE_LABEL" "$name"
    fi
    if compare_screens "$name"; then
      printf 'note  %s %s styles identical too, the record can close\n' "$SIZE_LABEL" "$name"
    else
      printf 'note  %s %s styles recorded, not asserted: %s\n' "$SIZE_LABEL" "$name" "$reason"
    fi
    return 0
  fi
  if [ "$mode" = same ]; then
    CHECKS=$((CHECKS + 1))
  else
    RECORDS=$((RECORDS + 1))
    RECORD_OWNERS[$owner]=$((${RECORD_OWNERS[$owner]:-0} + 1))
  fi
  if compare_screens "$name"; then
    printf 'ok    %s %s\n' "$SIZE_LABEL" "$name"
    return 0
  fi
  if [ "$mode" = same ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s %s\n' "$SIZE_LABEL" "$name"
  else
    [ -n "$reason" ] || die "recorded checkpoint $name says nothing about why"
    printf 'note  %s %s recorded, not asserted: %s\n' "$SIZE_LABEL" "$name" "$reason"
  fi
  return 0
}

run_size() {
  local size="$1"
  local mode="$2"
  local alternate="$3"
  local columns="${size%x*}"
  local rows="${size#*x}"
  SIZE_LABEL="$size"
  attach_both_at "$columns" "$rows"

  checkpoint fresh "$mode"

  set_on_both status off
  checkpoint status-off "$mode"
  set_on_both status on
  checkpoint status-on "$mode"

  run_on_both_active split-window -v "$INNER_SHELL"
  checkpoint split "$mode"

  run_on_both_active resize-pane -Z
  checkpoint zoom "$mode"
  run_on_both_active resize-pane -Z
  checkpoint unzoom "$mode"

  tmux_outer_command resize-window -t "=$OUTER_SESSION:zz" -x "$alternate" -y "$rows"
  tmux_outer_command resize-window -t "=$OUTER_SESSION:tmux" -x "$alternate" -y "$rows"
  wait_for "outer zz pane at ${alternate}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${alternate}x${rows}"
  wait_for "outer tmux pane at ${alternate}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${alternate}x${rows}"
  COLUMNS_UNDER_TEST="$alternate"
  SIZE_LABEL="${alternate}x${rows}-from-$size"
  checkpoint resized "$mode"

  tmux_outer_command resize-window -t "=$OUTER_SESSION:zz" -x "$columns" -y "$rows"
  tmux_outer_command resize-window -t "=$OUTER_SESSION:tmux" -x "$columns" -y "$rows"
  wait_for "outer zz pane back at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane back at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  COLUMNS_UNDER_TEST="$columns"
  SIZE_LABEL="$size"
  checkpoint restored "$mode"

  set_on_both status 2
  checkpoint status-two-rows "$mode"
  set_on_both status-position top
  checkpoint status-top "$mode"
  set_on_both status-position bottom
  set_on_both status on

  pin_pane_titles
  set_on_both pane-border-status top
  checkpoint pane-border-top "$mode"
  set_on_both pane-border-status bottom
  checkpoint pane-border-bottom "$mode"

  set_on_both pane-active-border-style 'fg=colour1,bg=colour4,bold'
  set_on_both pane-border-style 'fg=#ff8800'
  set_on_both pane-border-lines double
  set_on_both pane-border-status top
  checkpoint pane-border-attributes-top "$mode"
  set_on_both pane-border-lines heavy
  set_on_both pane-border-status bottom
  run_on_both select-pane -t "=$INNER_SESSION:0.0"
  checkpoint pane-border-attributes-bottom "$mode"
  run_on_both select-pane -t "=$INNER_SESSION:0.1"
  run_on_both set-option -gu pane-active-border-style
  run_on_both set-option -gu pane-border-style
  run_on_both set-option -gu pane-border-lines
  set_on_both pane-border-status off
  checkpoint pane-border-off "$mode"

  clear_both
  set_on_both status-style 'bg=red,fg=white'
  checkpoint status-style-named "$mode"
  set_on_both status-style 'bg=colour124,fg=colour231'
  checkpoint status-style-indexed "$mode"
  set_on_both status-style 'bg=#1e2030,fg=#c0caf5'
  checkpoint status-style-rgb "$mode"
  side_command zz set-option -gu status-style >/dev/null 2>&1 || true
  side_command tmux set-option -gu status-style >/dev/null 2>&1 || true

  set_on_both window-status-current-format '#[fg=red]N#[fg=colour196]I#[fg=#010203]R'
  checkpoint format-colour-classes "$mode"
  side_command zz set-option -gu window-status-current-format >/dev/null 2>&1 || true
  side_command tmux set-option -gu window-status-current-format >/dev/null 2>&1 || true

  clear_both
  send_both "printf 'W[%s][%s][%s]|\\n' '你好' 'éä' '🙂'"
  checkpoint wide-glyphs "$mode"

  clear_both
  set_on_both status-style bg=colour4
  checkpoint default-fg "$mode"
  side_command zz set-option -gu status-style >/dev/null 2>&1 || true
  side_command tmux set-option -gu status-style >/dev/null 2>&1 || true

  clear_both
  send_both "printf '\\033[31mNAMED\\033[0m \\033[38;5;196mINDEXED\\033[0m \\033[38;5;1mLOW\\033[0m \\033[38;2;1;2;3mRGB\\033[0m\\n'"
  checkpoint colour-classes "$mode"

  clear_both
  run_on_both set-option -s theme light
  checkpoint theme-light "$mode"
  side_command zz set-option -su theme >/dev/null 2>&1 || true
  side_command tmux set-option -su theme >/dev/null 2>&1 || true

  clear_both
  set_on_both status-left '#[fg=red,bold]LEFT'
  checkpoint styled-left-trim "$mode"
  set_on_both status-left L

  clear_both
  send_both "printf '\\033[5 q'"
  checkpoint cursor-style-request record \
    'the pin forwards a DECSCUSR the pane asked for and zz has nowhere to carry the request'
}

SIDEBAR_MARKER='zz at '
sidebar_on_screen() {
  capture_plain zz | grep -Fq "$SIDEBAR_MARKER"
}
sidebar_off_screen() {
  ! capture_plain zz | grep -Fq "$SIDEBAR_MARKER"
}
show_sidebar_on_zz() {
  zz_command bind-key -n F8 focus-sidebar || die 'zz refused bind-key -n F8'
  tmux_outer_command send-keys -t "=$OUTER_SESSION:zz" F8
  wait_for 'the sidebar on the zz screen' sidebar_on_screen
}

run_sidebar_case() {
  SIZE_LABEL='120x24-sidebar'
  attach_both_at 120 24
  checkpoint sidebar-before same

  show_sidebar_on_zz
  wait_settled zz "$SIDEBAR_MARKER" 'the sidebar settled on the zz screen'
  CHECKS=$((CHECKS + 1))
  compare_screens sidebar-shown || true
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf 'ok    %s sidebar-shown differs from the pin, which is what a drawn sidebar means\n' \
      "$SIZE_LABEL"
  else
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s sidebar-shown: focus-sidebar left the screen identical to the pin\n' \
      "$SIZE_LABEL"
  fi

  tmux_outer_command send-keys -t "=$OUTER_SESSION:zz" q
  wait_for 'the sidebar off the zz screen' sidebar_off_screen
  wait_settled zz "MARK-sidebar-before" 'the withdrawn canvas settled on the zz screen'
  CHECKS=$((CHECKS + 1))
  if compare_screens sidebar-withdrawn; then
    printf 'ok    %s sidebar-withdrawn\n' "$SIZE_LABEL"
  else
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s sidebar-withdrawn\n' "$SIZE_LABEL"
  fi
}

SELF_CHECK_FAILURES=0

plant() {
  local side="$1"
  local name="$2"
  local content="$3"
  printf '%s' "$content" >"$(side_home "$side")/$name"
}

self_check_case() {
  local name="$1"
  local expectation="$2"
  local rows_differed="$LAST_ROWS_DIFFERED"
  local cursor_differed="$LAST_CURSOR_DIFFERED"
  local verdict=ok
  case "$expectation" in
  rows)
    [ "$rows_differed" -eq 1 ] || verdict='no row difference reported'
    ;;
  cursor)
    [ "$cursor_differed" -eq 1 ] || verdict='no cursor difference reported'
    ;;
  none)
    if [ "$rows_differed" -ne 0 ] || [ "$cursor_differed" -ne 0 ]; then
      verdict='reported a difference the pin collapses'
    fi
    ;;
  esac
  if [ "$verdict" = ok ]; then
    printf 'ok    self-check %s\n' "$name"
    return 0
  fi
  SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
  printf 'FAIL  self-check %s: %s\n' "$name" "$verdict"
}

self_check_checkpoint() {
  local name="$1"
  send_both "printf 'MARK-%s\\n' $name"
  settle_both "MARK-$name" "$name"
  compare_screens "$name" || true
}

self_check_settle_on() {
  local marker="$1"
  local name="$2"
  settle_both "$marker" "$name"
  compare_screens "$name" || true
}

run_self_check() {
  printf 'self-check: one deliberate difference per channel, plus three the pin collapses\n'

  SIZE_LABEL='80x24-glyph'
  attach_both_at 80 24
  plant zz glyph 'GLYPH-AA'
  plant tmux glyph 'GLYPH-AB'
  send_both 'cat $HOME/glyph'
  self_check_checkpoint glyph
  self_check_case 'glyph, one character of output differs' rows

  SIZE_LABEL='80x24-colour'
  attach_both_at 80 24
  side_command zz set-option -g status-style bg=red || die 'zz refused status-style'
  self_check_checkpoint colour
  self_check_case 'colour, status-style bg=red on one side' rows

  SIZE_LABEL='80x24-border-style'
  attach_both_at 80 24
  run_on_both_active split-window -v "$INNER_SHELL"
  side_command zz set-option -g pane-border-style fg=red || die 'zz refused pane-border-style'
  self_check_checkpoint border-style
  self_check_case 'border style, pane-border-style fg=red on one side' rows

  SIZE_LABEL='80x24-border-attributes'
  attach_both_at 80 24
  run_on_both_active split-window -v "$INNER_SHELL"
  pin_pane_titles
  set_on_both pane-border-status top
  set_on_both pane-active-border-style 'fg=colour1,bg=colour4'
  side_command zz set-option -g pane-active-border-style 'fg=colour1,bg=colour4,bold' ||
    die 'zz refused pane-active-border-style'
  self_check_checkpoint border-attributes
  self_check_case 'border attributes, bold only in pane-active-border-style on one side' rows
  SIZE_LABEL='80x24-colour-class'
  attach_both_at 80 24
  plant zz class '\033[31mCLASS\033[0m'
  plant tmux class '\033[38;2;205;0;0mCLASS\033[0m'
  send_both 'printf "%b\\n" "$(cat $HOME/class)"'
  self_check_checkpoint colour-class
  self_check_case 'colour class, a named cell against the RGB it resolves to' rows

  SIZE_LABEL='80x24-default-fg'
  attach_both_at 80 24
  side_command zz set-option -g status-style bg=colour4 || die 'zz refused status-style'
  side_command tmux set-option -g status-style 'bg=colour4,fg=#d8dee9' ||
    die 'tmux refused status-style'
  self_check_checkpoint default-fg
  self_check_case 'default foreground, an explicit RGB foreground on one side' rows

  SIZE_LABEL='80x24-cursor'
  attach_both_at 80 24
  plant zz cursorline 'CURSORMARK-WITH-A-LONGER-TAIL'
  plant tmux cursorline 'CURSORMARK'
  send_both 'printf %s "$(cat $HOME/cursorline)"'
  self_check_settle_on CURSORMARK cursor
  self_check_case 'cursor, a different-length unterminated line' cursor

  SIZE_LABEL='120x24-sidebar-shown'
  attach_both_at 120 24
  send_both "printf 'MARK-%s\\n' sidebar-fresh"
  settle_both 'MARK-sidebar-fresh' sidebar-fresh
  show_sidebar_on_zz
  wait_settled zz "$SIDEBAR_MARKER" 'the sidebar settled on the zz screen'
  compare_screens sidebar-shown || true
  self_check_case 'sidebar, focus-sidebar shown on one side' rows

  SIZE_LABEL='80x24-text-mode-glyph'
  attach_both_at 80 24
  plant zz textmode 'TEXTMARK-A'
  plant tmux textmode 'TEXTMARK-B'
  send_both 'cat $HOME/textmode'
  settle_both 'TEXTMARK-' text-mode-glyph
  if compare_plain_screens text-mode-glyph; then
    SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
    printf 'FAIL  self-check text mode, glyph: the plain comparison reported no difference\n'
  else
    printf 'ok    self-check text mode, glyph: caught one character of output\n'
  fi

  SIZE_LABEL='80x24-text-mode-style'
  attach_both_at 80 24
  side_command zz set-option -g status-style bg=red || die 'zz refused status-style'
  send_both "printf 'MARK-%s\\n' textstyle"
  settle_both 'MARK-textstyle' text-mode-style
  if compare_plain_screens text-mode-style; then
    printf 'ok    self-check text mode, style: a pure colour difference is not a text difference\n'
  else
    SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
    printf 'FAIL  self-check text mode, style: the plain comparison reported a colour difference\n'
  fi
  compare_screens text-mode-style-styled || true
  self_check_case 'text mode, the styled comparison still catches that colour' rows

  SIZE_LABEL='80x24-cursor-shape'
  attach_both_at 80 24
  plant zz cstyle 'CSMARK'
  plant tmux cstyle '\033[5 qCSMARK'
  send_both 'printf "%b" "$(cat $HOME/cstyle)"'
  self_check_settle_on CSMARK cursor-shape
  self_check_case 'cursor shape, DECSCUSR on one side' cursor

  SIZE_LABEL='80x24-geometry'
  attach_both_at 80 24
  side_command zz split-window -v -t "$(active_pane zz)" "$INNER_SHELL" ||
    die 'zz refused split-window'
  self_check_checkpoint geometry
  self_check_case 'geometry, split-window on one side' rows

  SIZE_LABEL='80x10-status-rows'
  attach_both_at 80 10
  send_both "printf 'FILL-%s\\n' 1 2 3 4 5 6 7 8"
  settle_both 'FILL-8' status-rows-fill
  run_on_both_active split-window -v "$INNER_SHELL"
  set_on_both status 2
  side_command zz resize-window -t "=$INNER_SESSION" -y 9 ||
    die 'zz refused resize-window'
  self_check_checkpoint status-rows
  self_check_case 'status rows, one side keeps the window a row taller' rows

  SIZE_LABEL='80x24-styled-trim'
  attach_both_at 80 24
  set_on_both status-left '#[fg=red,bold]LEFT'
  side_command zz set-option -g status-left '#[fg=red,b' ||
    die 'zz refused status-left'
  self_check_checkpoint styled-trim
  self_check_case 'styled trim, one side keeps what a byte count leaves' rows

  SIZE_LABEL='80x24-style-order'
  attach_both_at 80 24
  side_command zz set-option -g status-style 'bg=colour1,fg=colour7,bold' ||
    die 'zz refused status-style'
  side_command tmux set-option -g status-style 'bold,fg=colour7,bg=colour1' ||
    die 'tmux refused status-style'
  self_check_checkpoint style-order
  self_check_case 'equivalence: attribute order in a style' none

  SIZE_LABEL='80x24-red-vs-colour1'
  attach_both_at 80 24
  side_command zz set-option -g status-style 'bg=red,fg=colour7' ||
    die 'zz refused status-style'
  side_command tmux set-option -g status-style 'bg=colour1,fg=colour7' ||
    die 'tmux refused status-style'
  self_check_checkpoint red-vs-colour1
  self_check_case 'equivalence: bg=red and bg=colour1 reach the same cell' none

  SIZE_LABEL='80x24-escape-spelling'
  attach_both_at 80 24
  plant zz spell '\033[1mBOLD\033[0m'
  plant tmux spell '\033[01mBOLD\033[m'
  send_both 'printf "%b\\n" "$(cat $HOME/spell)"'
  self_check_checkpoint escape-spelling
  self_check_case 'equivalence: two spellings of the same bold cell' none

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check expectations unmet\n' "$SELF_CHECK_FAILURES"
    return 1
  fi
  printf 'self-check complete: every sabotage was caught in its own channel and every equivalence passed\n'
  return 0
}

write_attach zz "$SCRATCH_DIR/attach-zz.sh"
write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"
zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
  exit $?
fi

printf 'whole-screen differential (pin %s)\n' "$(basename -- "$TMUX_BIN")"
for entry in "${SIZES[@]}"; do
  IFS='|' read -r size mode alternate <<<"$entry"
  run_size "$size" "$mode" "$alternate"
done
run_sidebar_case

if [ "$FAILURES" -ne 0 ]; then
  printf '%s of %s asserted checkpoints differ, %s recorded (%s)\n' \
    "$FAILURES" "$CHECKS" "$RECORDS" "$(owner_tally)"
  exit 1
fi
printf 'all %s asserted checkpoints identical, %s recorded not asserted (%s)\n' \
  "$CHECKS" "$RECORDS" "$(owner_tally)"

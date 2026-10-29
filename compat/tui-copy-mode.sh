#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-copy-mode.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-copy-mode.sh\n' >&2
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

COLUMNS_UNDER_TEST=80
ROWS_UNDER_TEST=24
PANE_TITLE="copymode"
WINDOW_NAME="win"
SESSION_NAME="copy"
SCRATCH_DIR="$(mktemp -d /tmp/zzcm.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzcmo-$TOKEN"
INNER_SOCKET_NAME="zzcmi-$TOKEN"
ZZ_SOCKET="/tmp/zzcm-$TOKEN.sock"
OUTER_SESSION="driver"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
ZZ_CLIENT_STDERR="$SCRATCH_DIR/zz-client.err"
TMUX_CLIENT_STDERR="$SCRATCH_DIR/tmux-client.err"
LINES_FILE="$SCRATCH_DIR/lines.txt"
CLAMP_LINES_FILE="$SCRATCH_DIR/clamp-lines.txt"
CAPTURE_DIR="${ZZ_COPY_CAPTURE_DIR:-}"
DIAGNOSTICS_DIR=""
CASE_LABEL=""
ZZ_PID=""
FAILURES=0
CHECKS=0
RECORDS=0
ASSERTING=0
LAST_ROWS_DIFFERED=0
LAST_TEXT_DIFFERED=0
LAST_CURSOR_DIFFERED=0
LAST_FACTS_DIFFERED=0
LAST_VIEW_DIFFERED=0
LAST_BUFFER_DIFFERED=0
LAST_FORMATS_DIFFERED=0
LAST_MISSING_OBSERVABLE=""
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
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
    DIAGNOSTICS_DIR="${ZZ_COPY_DIAGNOSTICS_DIR:-$(mktemp -d /tmp/zzcm-diag.XXXXXX)}"
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
    printf 'case under test: %s\n' "${CASE_LABEL:-none yet}"
    printf 'zz: %s\n' "$ZZ_BIN"
    printf 'tmux: %s\n' "$TMUX_BIN"
    printf 'zz socket: %s\n' "$ZZ_SOCKET"
    printf 'outer socket: %s\n' "$OUTER_SOCKET_NAME"
    printf 'inner tmux socket: %s\n' "$INNER_SOCKET_NAME"
  } >"$dir/what-fired.txt" 2>&1 || true
  for side in zz tmux; do
    tmux_outer_command capture-pane -p -e -S - -t "=$OUTER_SESSION:$side" \
      >"$dir/outer-$side.screen.txt" 2>&1 || true
    side_command "$side" list-panes -a \
      -F '#{session_name}:#{window_index}.#{pane_index} #{pane_width}x#{pane_height} mode=#{pane_mode}' \
      >"$dir/$side.list-panes.txt" 2>&1 || true
    side_command "$side" display-message -p -t "=$SESSION_NAME:0.0" "$FACTS_FORMAT" \
      >"$dir/$side.facts.txt" 2>&1 || true
    side_command "$side" list-keys -T copy-mode >"$dir/$side.copy-mode-keys.txt" 2>&1 || true
    side_command "$side" list-keys -T copy-mode-vi >"$dir/$side.copy-mode-vi-keys.txt" 2>&1 || true
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

CURSOR_FORMAT='#{cursor_x},#{cursor_y} flag=#{cursor_flag} pane=#{pane_width}x#{pane_height}'
FACTS_FORMAT='mode=#{pane_mode} in=#{pane_in_mode} column=#{copy_cursor_x} selection=#{selection_present} search=[#{pane_search_string}] line=[#{copy_cursor_line}]'
VIEW_FORMAT='row=#{copy_cursor_y} scroll=#{scroll_position}'
MODE_FORMATS='selection_active=[#{selection_active}] rectangle_toggle=[#{rectangle_toggle}] pane_search_string=[#{pane_search_string}]'

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" = "$SESSION_NAME" ]
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
  tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" "$CURSOR_FORMAT"
}
side_facts() {
  side_command "$1" display-message -p -t "=$SESSION_NAME:0.0" "$FACTS_FORMAT" 2>/dev/null ||
    printf 'no answer\n'
}
side_view() {
  side_command "$1" display-message -p -t "=$SESSION_NAME:0.0" "$VIEW_FORMAT" 2>/dev/null ||
    printf 'no answer\n'
}
side_mode_formats() {
  side_command "$1" display-message -p -t "=$SESSION_NAME:0.0" "$MODE_FORMATS" 2>/dev/null ||
    printf 'no answer\n'
}
assert_mode_formats() {
  local label="$1"
  local zz_formats tmux_formats
  CHECKS=$((CHECKS + 1))
  ASSERTING=$((ASSERTING + 1))
  zz_formats="$(side_mode_formats zz)"
  tmux_formats="$(side_mode_formats tmux)"
  LAST_FORMATS_DIFFERED=0
  if [ "$zz_formats" = "$tmux_formats" ]; then
    printf 'ok    %s-formats %s\n' "$label" "$tmux_formats"
    return 0
  fi
  LAST_FORMATS_DIFFERED=1
  FAILURES=$((FAILURES + 1))
  printf 'DIFF  %s-formats\n' "$label"
  printf '      formats tmux: %s\n' "$tmux_formats"
  printf '      formats zz:   %s\n' "$zz_formats"
}
side_buffer() {
  local side="$1"
  local destination="$2"
  side_command "$side" show-buffer >"$destination" 2>/dev/null || : >"$destination"
}

write_attach() {
  local side="$1"
  local destination="$2"
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp %q --socket %q attach-session -t %q 2> >(tee -a %q >&2)\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" "=$SESSION_NAME" "$ZZ_CLIENT_STDERR" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp %q -L %q attach-session -t %q 2> >(tee -a %q >&2)\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" "=$SESSION_NAME" "$TMUX_CLIENT_STDERR" >>"$destination"
  fi
  chmod +x "$destination"
}

OWNED_OPTIONS=(
  status status-left status-right automatic-rename default-command repeat-time
  mode-keys mode-style copy-mode-position-format copy-mode-selection-style
  copy-mode-match-style copy-mode-current-match-style copy-mode-mark-style
  copy-mode-line-numbers copy-mode-current-line-style
  pane-border-style pane-active-border-style message-style status-style
  window-status-current-style
)
PINNED_STYLES=(
  'status-style bg=#00af00,fg=#101010'
  'window-status-current-style bg=#00af00,fg=#101010'
  'pane-border-style fg=#7f7f7f,bg=#101010'
  'pane-active-border-style fg=#00af00,bg=#101010'
  'message-style bg=#cdcd00,fg=#101010'
  'mode-style bg=#cdcd00,fg=#101010'
  'copy-mode-selection-style bg=#cdcd00,fg=#101010'
  'copy-mode-match-style bg=#00cdcd,fg=#101010'
  'copy-mode-current-match-style bg=#cd00cd,fg=#101010'
  'copy-mode-mark-style bg=#cd00cd,fg=#101010'
)
pin_dynamic_values() {
  local side="$1"
  local option entry
  for option in "${OWNED_OPTIONS[@]}"; do
    side_command "$side" set-option -gu "$option" >/dev/null 2>&1 || true
  done
  side_command "$side" set-option -g status-right '' || die "$side refused status-right"
  side_command "$side" set-option -g status-left L || die "$side refused status-left"
  side_command "$side" set-option -g automatic-rename off || die "$side refused automatic-rename"
  side_command "$side" set-option -g default-command "$INNER_SHELL" ||
    die "$side refused default-command"
  side_command "$side" set-option -g copy-mode-position-format '' ||
    die "$side refused copy-mode-position-format"
  side_command "$side" set-option -g status off || die "$side refused status off"
  for entry in "${PINNED_STYLES[@]}"; do
    side_command "$side" set-option -g "${entry%% *}" "${entry#* }" ||
      die "$side refused ${entry%% *}"
  done
}

set_on_both() {
  side_command zz set-option -g "$1" "$2" || die "zz refused set-option -g $1"
  side_command tmux set-option -g "$1" "$2" || die "tmux refused set-option -g $1"
}

kill_every_session() {
  local side="$1"
  local session
  while read -r session; do
    [ -n "$session" ] || continue
    side_command "$side" kill-session -t "=$session" >/dev/null 2>&1 || true
  done < <(side_command "$side" list-sessions -F '#{session_name}' 2>/dev/null || true)
}
delete_every_buffer() {
  local side="$1"
  local buffer
  while read -r buffer; do
    [ -n "$buffer" ] || continue
    side_command "$side" delete-buffer -b "$buffer" >/dev/null 2>&1 || true
  done < <(side_command "$side" list-buffers -F '#{buffer_name}' 2>/dev/null || true)
}

generate_lines() {
  local index
  : >"$LINES_FILE"
  for ((index = 1; index <= 60; index++)); do
    if [ "$index" -eq 12 ] || [ "$index" -eq 48 ]; then
      printf 'line-%02d needle-%02d\n' "$index" "$index" >>"$LINES_FILE"
    else
      printf 'line-%02d filler-%02d\n' "$index" "$index" >>"$LINES_FILE"
    fi
  done
  printf 'SEEDEND\n' >>"$LINES_FILE"
}

generate_clamp_lines() {
  local index
  : >"$CLAMP_LINES_FILE"
  for ((index = 1; index <= 77; index++)); do
    printf 'line-%02d filler-%02d\n' "$index" "$index" >>"$CLAMP_LINES_FILE"
  done
  printf 'aaaaaaaaaaaa\nbbbbbbbbbbbbbbbbbbbb\ncc\n\nSEEDEND\n' >>"$CLAMP_LINES_FILE"
}

send_pane_both() {
  local side
  for side in zz tmux; do
    side_command "$side" send-keys -t "=$SESSION_NAME:0.0" "$1" Enter ||
      die "$side refused send-keys"
  done
}

seed_pane() {
  local file="${1:-$LINES_FILE}"
  local side
  send_pane_both "printf '\\033[2J\\033[3J\\033[H'"
  for side in zz tmux; do
    wait_settled "$side" 'the cleared screen'
  done
  send_pane_both "cat $file"
  for side in zz tmux; do
    if ! await_observable "$side" screen SEEDEND; then
      die "$side never showed the seeded content"
    fi
    wait_settled "$side" 'the seeded screen'
  done
}

attach_both_at() {
  local columns="$COLUMNS_UNDER_TEST"
  local rows="$ROWS_UNDER_TEST"
  tmux_outer_command kill-server >/dev/null 2>&1 || true
  kill_every_session zz
  kill_every_session tmux
  delete_every_buffer zz
  delete_every_buffer tmux
  side_command zz unbind-key -T copy-mode-vi Z >/dev/null 2>&1 || true
  side_command tmux unbind-key -T copy-mode-vi Z >/dev/null 2>&1 || true
  zz_command new-session -d -s "$SESSION_NAME" -n "$WINDOW_NAME" -x "$columns" -y "$rows" \
    "$INNER_SHELL" || die "could not create the zz session"
  tmux_inner_command -f /dev/null new-session -d -s "$SESSION_NAME" -n "$WINDOW_NAME" \
    -x "$columns" -y "$rows" "$INNER_SHELL" || die "could not create the tmux session"
  pin_dynamic_values zz
  pin_dynamic_values tmux
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$columns" -y "$rows" "$SCRATCH_DIR/attach-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off
  tmux_outer_command set-option -g remain-on-exit on
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/attach-tmux.sh"
  wait_for "outer zz pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  wait_for "zz client attached" client_attached zz
  wait_for "tmux client attached" client_attached tmux
  side_command zz rename-window -t "=$SESSION_NAME:0" "$WINDOW_NAME" || die 'zz refused rename-window'
  side_command tmux rename-window -t "=$SESSION_NAME:0" "$WINDOW_NAME" || die 'tmux refused rename-window'
  side_command zz select-pane -t "=$SESSION_NAME:0.0" -T "$PANE_TITLE" || die 'zz refused select-pane'
  side_command tmux select-pane -t "=$SESSION_NAME:0.0" -T "$PANE_TITLE" || die 'tmux refused select-pane'
  wait_settled zz 'the fresh zz screen'
  wait_settled tmux 'the fresh tmux screen'
}

type_side() {
  local side="$1"
  shift
  tmux_outer_command send-keys -t "=$OUTER_SESSION:$side" "$@" ||
    die "the outer tmux refused send-keys for $side"
}
type_both() {
  type_side zz "$@"
  type_side tmux "$@"
}
type_prefix_both() {
  type_both C-b "$@"
}

wait_settled() {
  local side="$1"
  local label="$2"
  local previous=""
  local current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_plain "$side" 2>/dev/null || true)
$(cursor_tuple "$side" 2>/dev/null || true)"
    if [ -n "$previous" ] && [ "$current" = "$previous" ]; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  dump_diagnostics "$label"
  die "$label did not settle within 10 seconds"
}

await_observable() {
  local side="$1"
  local kind="$2"
  local value="$3"
  local attempt format expected
  for ((attempt = 0; attempt < 200; attempt++)); do
    case "$kind" in
    format)
      format="${value%%=*}"
      expected="${value#*=}"
      if [ "$(side_command "$side" display-message -p -t "=$SESSION_NAME:0.0" "$format" 2>/dev/null)" = "$expected" ]; then
        return 0
      fi
      ;;
    screen)
      if capture_plain "$side" 2>/dev/null | grep -Fq "$value"; then return 0; fi
      ;;
    absent)
      if ! capture_plain "$side" 2>/dev/null | grep -Fq "$value"; then return 0; fi
      ;;
    none) return 0 ;;
    esac
    sleep 0.05
  done
  return 1
}

compare_sides() {
  local name="$1"
  local zz_rows tmux_rows zz_cursor tmux_cursor zz_facts tmux_facts index differing total
  local zz_view tmux_view zz_text tmux_text text_differing
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  mapfile -t zz_text < <(capture_plain zz)
  mapfile -t tmux_text < <(capture_plain tmux)
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  zz_facts="$(side_facts zz)"
  tmux_facts="$(side_facts tmux)"
  zz_view="$(side_view zz)"
  tmux_view="$(side_view tmux)"
  side_buffer zz "$SCRATCH_DIR/zz.buffer"
  side_buffer tmux "$SCRATCH_DIR/tmux.buffer"
  if [ -n "$CAPTURE_DIR" ]; then
    printf '%s\n' "${zz_rows[@]-}" >"$CAPTURE_DIR/$name.zz.screen.txt"
    printf '%s\n' "${tmux_rows[@]-}" >"$CAPTURE_DIR/$name.tmux.screen.txt"
    {
      printf 'cursor zz:   %s\n' "$zz_cursor"
      printf 'cursor tmux: %s\n' "$tmux_cursor"
      printf 'facts zz:   %s\n' "$zz_facts"
      printf 'facts tmux: %s\n' "$tmux_facts"
      printf 'view zz:    %s\n' "$zz_view"
      printf 'view tmux:  %s\n' "$tmux_view"
      printf 'buffer zz:\n'
      od -c -- "$SCRATCH_DIR/zz.buffer"
      printf 'buffer tmux:\n'
      od -c -- "$SCRATCH_DIR/tmux.buffer"
    } >"$CAPTURE_DIR/$name.facts.txt"
  fi
  total="$ROWS_UNDER_TEST"
  differing=-1
  for ((index = 0; index < total; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      differing="$index"
      break
    fi
  done
  text_differing=-1
  for ((index = 0; index < total; index++)); do
    if [ "${zz_text[index]-}" != "${tmux_text[index]-}" ]; then
      text_differing="$index"
      break
    fi
  done
  LAST_ROWS_DIFFERED=0
  LAST_TEXT_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  LAST_FACTS_DIFFERED=0
  LAST_VIEW_DIFFERED=0
  LAST_BUFFER_DIFFERED=0
  [ "$differing" -lt 0 ] || LAST_ROWS_DIFFERED=1
  [ "$text_differing" -lt 0 ] || LAST_TEXT_DIFFERED=1
  [ "$zz_cursor" = "$tmux_cursor" ] || LAST_CURSOR_DIFFERED=1
  [ "$zz_facts" = "$tmux_facts" ] || LAST_FACTS_DIFFERED=1
  [ "$zz_view" = "$tmux_view" ] || LAST_VIEW_DIFFERED=1
  cmp -s -- "$SCRATCH_DIR/zz.buffer" "$SCRATCH_DIR/tmux.buffer" || LAST_BUFFER_DIFFERED=1
  if [ "$LAST_ROWS_DIFFERED" -eq 0 ] && [ "$LAST_TEXT_DIFFERED" -eq 0 ] &&
    [ "$LAST_CURSOR_DIFFERED" -eq 0 ] && [ "$LAST_FACTS_DIFFERED" -eq 0 ] &&
    [ "$LAST_VIEW_DIFFERED" -eq 0 ] && [ "$LAST_BUFFER_DIFFERED" -eq 0 ] &&
    [ -z "$LAST_MISSING_OBSERVABLE" ]; then
    return 0
  fi
  printf '      case %s\n' "$name"
  if [ -n "$LAST_MISSING_OBSERVABLE" ]; then
    printf '      never reached the observable: %s\n' "$LAST_MISSING_OBSERVABLE"
  fi
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf '      first differing row %s of %s\n' "$differing" "$total"
    printf '        tmux: %s\n' "$(printf '%s' "${tmux_rows[differing]-}" | cat -v)"
    printf '        zz:   %s\n' "$(printf '%s' "${zz_rows[differing]-}" | cat -v)"
  fi
  if [ "$LAST_TEXT_DIFFERED" -eq 1 ]; then
    printf '      first differing glyph row %s of %s\n' "$text_differing" "$total"
    printf '        tmux text: %s\n' "$(printf '%s' "${tmux_text[text_differing]-}" | cat -v)"
    printf '        zz text:   %s\n' "$(printf '%s' "${zz_text[text_differing]-}" | cat -v)"
  fi
  if [ "$LAST_CURSOR_DIFFERED" -eq 1 ]; then
    printf '      cursor tmux: %s\n' "$tmux_cursor"
    printf '      cursor zz:   %s\n' "$zz_cursor"
  fi
  if [ "$LAST_FACTS_DIFFERED" -eq 1 ]; then
    printf '      facts tmux: %s\n' "$tmux_facts"
    printf '      facts zz:   %s\n' "$zz_facts"
  fi
  if [ "$LAST_VIEW_DIFFERED" -eq 1 ]; then
    printf '      view tmux:  %s\n' "$tmux_view"
    printf '      view zz:    %s\n' "$zz_view"
  fi
  if [ "$LAST_BUFFER_DIFFERED" -eq 1 ]; then
    printf '      buffer tmux: %s\n' "$(od -c -- "$SCRATCH_DIR/tmux.buffer" | head -4 | tr '\n' '|')"
    printf '      buffer zz:   %s\n' "$(od -c -- "$SCRATCH_DIR/zz.buffer" | head -4 | tr '\n' '|')"
  fi
  return 1
}

copy_case() {
  local name="$1"
  local kind="$2"
  local observable="$3"
  local mode="${4:-rows,text,cursor,facts,view,buffer}"
  local reason="${5:-}"
  local side
  CASE_LABEL="$name"
  LAST_MISSING_OBSERVABLE=""
  for side in zz tmux; do
    if ! await_observable "$side" "$kind" "$observable"; then
      LAST_MISSING_OBSERVABLE="${LAST_MISSING_OBSERVABLE}${LAST_MISSING_OBSERVABLE:+, }$side never showed $kind $observable"
    fi
  done
  wait_settled zz "$name on the zz screen"
  wait_settled tmux "$name on the tmux screen"
  CHECKS=$((CHECKS + 1))
  case "$mode" in
  *rows* | *text* | *cursor* | *facts* | *view* | *buffer*) ASSERTING=$((ASSERTING + 1)) ;;
  esac
  if compare_sides "$name"; then
    printf 'ok    %s\n' "$name"
    return 0
  fi
  local asserted=0
  case "$mode" in *rows*) asserted=$((asserted + LAST_ROWS_DIFFERED)) ;; esac
  case "$mode" in *text*) asserted=$((asserted + LAST_TEXT_DIFFERED)) ;; esac
  case "$mode" in *cursor*) asserted=$((asserted + LAST_CURSOR_DIFFERED)) ;; esac
  case "$mode" in *facts*) asserted=$((asserted + LAST_FACTS_DIFFERED)) ;; esac
  case "$mode" in *view*) asserted=$((asserted + LAST_VIEW_DIFFERED)) ;; esac
  case "$mode" in *buffer*) asserted=$((asserted + LAST_BUFFER_DIFFERED)) ;; esac
  [ -z "$LAST_MISSING_OBSERVABLE" ] || asserted=$((asserted + 1))
  if [ "$asserted" -gt 0 ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s\n' "$name"
  else
    [ -n "$reason" ] || die "recorded case $name says nothing about why"
    RECORDS=$((RECORDS + 1))
    if [ "$mode" = none ]; then
      printf 'note  %s asserts no channel, recorded in full: %s\n' "$name" "$reason"
    else
      printf 'note  %s asserted %s, the rest recorded: %s\n' "$name" "$mode" "$reason"
    fi
  fi
  return 0
}

table_keys() {
  case "$1" in
  emacs)
    KEY_DOWN=C-n KEY_UP=C-p KEY_HALFDOWN=M-Down KEY_HALFUP=M-Up
    KEY_PAGEDOWN=C-v KEY_PAGEUP=M-v KEY_SOL=C-a KEY_EOL=C-e
    KEY_RIGHT=C-f
    KEY_BEGINSEL=C-Space KEY_RECT=R KEY_COPY=M-w KEY_CANCEL=q
    KEY_SEARCHFWD=C-s KEY_SEARCHBACK=C-r KEY_COUNT=M-5
    SEND_PREFIX_OBSERVABLE='#{copy_cursor_x}=2'
    SEARCH_OPEN_MODE=rows,text,cursor,facts,view,buffer
    SEARCH_OPEN_REASON=''
    ;;
  vi)
    KEY_DOWN=j KEY_UP=k KEY_HALFDOWN=C-d KEY_HALFUP=C-u
    KEY_PAGEDOWN=NPage KEY_PAGEUP=PPage KEY_SOL=0 KEY_EOL='$'
    KEY_RIGHT=l
    KEY_BEGINSEL=Space KEY_RECT=v KEY_COPY=Enter KEY_CANCEL=q
    KEY_SEARCHFWD=/ KEY_SEARCHBACK='?' KEY_COUNT=5
    SEND_PREFIX_OBSERVABLE='#{scroll_position}=22'
    SEARCH_OPEN_MODE=rows,text,cursor,facts,view,buffer
    SEARCH_OPEN_REASON=''
    ;;
  esac
}

enter_copy_mode() {
  local label="$1"
  type_prefix_both '['
  if ! await_observable zz format '#{pane_in_mode}=1' ||
    ! await_observable tmux format '#{pane_in_mode}=1'; then
    die "$label could not enter copy mode"
  fi
}
reenter_copy_mode() {
  local label="$1"
  type_both q
  if ! await_observable zz format '#{pane_in_mode}=0' ||
    ! await_observable tmux format '#{pane_in_mode}=0'; then
    die "$label could not leave copy mode"
  fi
  enter_copy_mode "$label"
}

run_movement() {
  local table="$1"
  table_keys "$table"

  attach_both_at
  set_on_both mode-keys "$table"
  seed_pane

  type_prefix_both '['
  copy_case "$table-enter" format '#{pane_in_mode}=1'

  type_both "$KEY_UP"
  type_both "$KEY_UP"
  copy_case "$table-cursor-up" none ''

  type_both "$KEY_DOWN"
  copy_case "$table-cursor-down" none ''

  type_both "$KEY_COUNT"
  type_both "$KEY_UP"
  copy_case "$table-count-five-up" none ''

  type_both "$KEY_EOL"
  copy_case "$table-end-of-line" none ''

  type_both "$KEY_SOL"
  copy_case "$table-start-of-line" none ''

  reenter_copy_mode "$table scrolling"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  copy_case "$table-scroll-start" none ''
  type_both "$KEY_HALFUP"
  copy_case "$table-halfpage-up" screen 'line-27 filler-27'
  type_both "$KEY_HALFDOWN"
  copy_case "$table-halfpage-down" none ''
  reenter_copy_mode "$table paging"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  copy_case "$table-page-start" none ''
  type_both "$KEY_PAGEUP"
  copy_case "$table-page-up" none ''
  type_both "$KEY_PAGEDOWN"
  copy_case "$table-page-down" none ''

  reenter_copy_mode "$table selection"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_SOL"
  copy_case "$table-selection-start" none ''
  type_both "$KEY_BEGINSEL"
  copy_case "$table-begin-selection" none ''
  type_both "$KEY_DOWN"
  type_both "$KEY_EOL"
  copy_case "$table-selection-extends" none ''
  assert_mode_formats "$table-selection-extends"
  type_both "$KEY_RECT"
  copy_case "$table-rectangle-on" none ''
  assert_mode_formats "$table-rectangle-on"
  type_both "$KEY_RECT"
  copy_case "$table-rectangle-off" none ''

  type_both "$KEY_COPY"
  copy_case "$table-copy-and-cancel" format '#{pane_in_mode}=0'

  enter_copy_mode "$table rectangle inside the line"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_SOL"
  type_both "$KEY_RECT"
  type_both "$KEY_BEGINSEL"
  type_both "$KEY_DOWN"
  type_both "$KEY_COUNT"
  type_both "$KEY_RIGHT"
  copy_case "$table-rectangle-inside-line" format '#{copy_cursor_x}=5'
  assert_mode_formats "$table-rectangle-inside-line"
  type_both "$KEY_COPY"
  copy_case "$table-rectangle-inside-line-copy" format '#{pane_in_mode}=0'

  enter_copy_mode "$table rectangle past the end of the line"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_SOL"
  type_both "$KEY_RECT"
  type_both "$KEY_BEGINSEL"
  type_both "$KEY_DOWN"
  local step
  for step in 1 2 3 4; do
    type_both "$KEY_COUNT"
    type_both "$KEY_RIGHT"
  done
  copy_case "$table-rectangle-past-end-of-line" format '#{copy_cursor_x}=20'
  assert_mode_formats "$table-rectangle-past-end-of-line"
  type_both "$KEY_COPY"
  copy_case "$table-rectangle-past-end-of-line-copy" format '#{pane_in_mode}=0'
  delete_every_buffer zz
  delete_every_buffer tmux

  type_prefix_both '['
  copy_case "$table-reenter" format '#{pane_in_mode}=1'
  type_both "$KEY_CANCEL"
  copy_case "$table-cancel" format '#{pane_in_mode}=0'

  type_both -l 'echo BACKALIVE'
  type_both Enter
  copy_case "$table-terminal-input-returns" screen 'BACKALIVE'

  type_prefix_both '['
  if ! await_observable zz format '#{pane_in_mode}=1' ||
    ! await_observable tmux format '#{pane_in_mode}=1'; then
    die "$table prefix-precedence could not enter copy mode"
  fi
  type_both "$KEY_UP"
  type_both C-b
  copy_case "$table-prefix-takes-precedence" none ''
  type_both '['
  type_both "$KEY_UP"
  copy_case "$table-prefix-returns-to-the-copy-table" none ''

  reenter_copy_mode "$table send-prefix"
  type_both "$KEY_UP"
  type_both "$KEY_UP"
  type_both "$KEY_SOL"
  type_both "$KEY_RIGHT"
  type_both "$KEY_RIGHT"
  type_both "$KEY_RIGHT"
  copy_case "$table-send-prefix-start" format '#{copy_cursor_x}=3'
  type_prefix_both C-b
  copy_case "$table-send-prefix-runs-the-copy-binding" format "$SEND_PREFIX_OBSERVABLE"

  reenter_copy_mode "$table send-keys"
  side_command zz send-keys -t "=$SESSION_NAME:0.0" -N 2 Z "$KEY_UP" ||
    die 'zz refused send-keys into copy mode'
  side_command tmux send-keys -t "=$SESSION_NAME:0.0" -N 2 Z "$KEY_UP" ||
    die 'tmux refused send-keys into copy mode'
  copy_case "$table-send-keys-runs-the-copy-table" format '#{copy_cursor_y}=21'
  type_both "$KEY_CANCEL"
  copy_case "$table-send-keys-cancel" format '#{pane_in_mode}=0'
}

run_page_clamp() {
  local table="$1"
  table_keys "$table"

  attach_both_at
  set_on_both mode-keys "$table"
  seed_pane "$CLAMP_LINES_FILE"

  type_prefix_both '['
  copy_case "$table-clamp-enter" format '#{pane_in_mode}=1'
  type_both "$KEY_COUNT"
  type_both "$KEY_UP"
  copy_case "$table-clamp-count-five-up" format '#{copy_cursor_line}=aaaaaaaaaaaa'
  type_both "$KEY_HALFUP"
  type_both "$KEY_HALFUP"
  copy_case "$table-clamp-halfpage-up-twice" format '#{scroll_position}=24'
  type_both "$KEY_HALFDOWN"
  type_both "$KEY_HALFDOWN"
  copy_case "$table-clamp-halfpage-down-twice" format '#{scroll_position}=0'
  type_both "$KEY_HALFDOWN"
  copy_case "$table-clamp-halfpage-down-onto-the-prompt" format '#{copy_cursor_y}=23'

  reenter_copy_mode "$table page clamp"
  type_both "$KEY_COUNT"
  type_both "$KEY_UP"
  copy_case "$table-clamp-page-count-five-up" format '#{copy_cursor_line}=aaaaaaaaaaaa'
  type_both "$KEY_PAGEDOWN"
  copy_case "$table-clamp-page-down-onto-the-prompt" format '#{copy_cursor_y}=23'
}

run_search() {
  local table="$1"
  table_keys "$table"

  attach_both_at
  set_on_both mode-keys "$table"
  seed_pane

  type_prefix_both '['
  copy_case "$table-ordinary-pane-enter" format '#{pane_in_mode}=1'

  type_both "$KEY_SEARCHFWD"
  copy_case "$table-ordinary-pane-search-prompt" none ''

  type_both -l 'needlex'
  copy_case "$table-ordinary-pane-search-typed" none ''

  type_both BSpace
  copy_case "$table-ordinary-pane-search-backspace" none '' "$SEARCH_OPEN_MODE" "$SEARCH_OPEN_REASON"

  type_both Enter
  copy_case "$table-ordinary-pane-search-submit" none ''
  assert_mode_formats "$table-ordinary-pane-search-submit"

  type_both n
  copy_case "$table-ordinary-pane-search-again" none ''

  type_both N
  copy_case "$table-ordinary-pane-search-reverse" none ''

  type_both "$KEY_SEARCHBACK"
  type_both -l 'filler-30'
  type_both Enter
  copy_case "$table-ordinary-pane-search-backward" none ''
  assert_mode_formats "$table-ordinary-pane-search-backward"

  type_both q
  copy_case "$table-ordinary-pane-search-cancel" format '#{pane_in_mode}=0'
  assert_mode_formats "$table-ordinary-pane-search-cancel"

  type_prefix_both '['
  copy_case "$table-ordinary-pane-search-reentry" format '#{pane_in_mode}=1'
  assert_mode_formats "$table-ordinary-pane-search-reentry"
  type_both n
  copy_case "$table-ordinary-pane-search-reentry-again" format '#{copy_cursor_line}=line-30 filler-30'

  reenter_copy_mode "$table forward search"
  side_command zz send-keys -t "=$SESSION_NAME:0.0" -X search-forward needle ||
    die 'zz refused search-forward'
  side_command tmux send-keys -t "=$SESSION_NAME:0.0" -X search-forward needle ||
    die 'tmux refused search-forward'
  await_observable zz format '#{copy_cursor_line}=line-12 needle-12' || true
  await_observable tmux format '#{copy_cursor_line}=line-12 needle-12' || true
  reenter_copy_mode "$table forward search re-entry"
  copy_case "$table-forward-search-reentry" format '#{pane_in_mode}=1'
  assert_mode_formats "$table-forward-search-reentry"
  type_both n
  copy_case "$table-forward-search-reentry-searches-up" format '#{copy_cursor_line}=line-48 needle-48'
  type_both q
  copy_case "$table-forward-search-cancel" format '#{pane_in_mode}=0'
  assert_mode_formats "$table-forward-search-cancel"

  enter_copy_mode "$table search prompt on a fresh entry"
  assert_mode_formats "$table-fresh-entry-before-the-prompt"
  type_both "$KEY_SEARCHBACK"
  copy_case "$table-fresh-entry-search-prompt" screen '(search up)'
  type_both Escape
  copy_case "$table-fresh-entry-search-prompt-escape" absent '(search up)'
  type_both q
  copy_case "$table-fresh-entry-search-prompt-cancel" format '#{pane_in_mode}=0'
  assert_mode_formats "$table-fresh-entry-search-prompt-cancel"
}

run_native_search_command() {
  local side output status
  attach_both_at
  set_on_both mode-keys emacs
  set_on_both status on
  seed_pane
  type_prefix_both '['
  if ! await_observable zz format '#{pane_in_mode}=1' ||
    ! await_observable tmux format '#{pane_in_mode}=1'; then
    die 'native-search-command could not enter copy mode'
  fi
  for side in zz tmux; do
    status=0
    output="$(side_command "$side" copy-mode-search-prompt -t "=$SESSION_NAME:0.0" 2>&1)" ||
      status=$?
    printf 'note  native-search-command %s exit %s: %s\n' "$side" "$status" "${output:-<no output>}"
  done
  wait_settled zz 'the zz screen after copy-mode-search-prompt'
  wait_settled tmux 'the tmux screen after copy-mode-search-prompt'
  for side in zz tmux; do
    printf 'note  native-search-command %s last row: %s\n' "$side" \
      "$(capture_plain "$side" | tail -1)"
  done
  type_both q
  if ! await_observable zz format '#{pane_in_mode}=0' ||
    ! await_observable tmux format '#{pane_in_mode}=0'; then
    die 'native-search-command could not leave copy mode'
  fi
}

run_live_mode_keys() {
  attach_both_at
  set_on_both mode-keys emacs
  seed_pane

  type_prefix_both '['
  copy_case 'live-mode-keys-emacs-entry' format '#{pane_in_mode}=1'
  type_both j
  copy_case 'live-mode-keys-emacs-ignores-j' none ''
  type_both q
  copy_case 'live-mode-keys-emacs-cancel' format '#{pane_in_mode}=0'

  set_on_both mode-keys vi
  type_prefix_both '['
  copy_case 'live-mode-keys-vi-entry' format '#{pane_in_mode}=1'
  type_both j
  type_both j
  copy_case 'live-mode-keys-vi-moves-on-j' none ''
  type_both q
  copy_case 'live-mode-keys-vi-cancel' format '#{pane_in_mode}=0'

  type_prefix_both '['
  if ! await_observable zz format '#{pane_in_mode}=1' ||
    ! await_observable tmux format '#{pane_in_mode}=1'; then
    die 'live-binding could not enter copy mode'
  fi
  type_both '$'
  copy_case 'live-binding-before-the-bind' none ''
  type_both Z
  copy_case 'live-binding-unbound-key-does-nothing' none ''
  side_command zz bind-key -T copy-mode-vi Z send-keys -X start-of-line ||
    die 'zz refused bind-key -T copy-mode-vi'
  side_command tmux bind-key -T copy-mode-vi Z send-keys -X start-of-line ||
    die 'tmux refused bind-key -T copy-mode-vi'
  type_both Z
  copy_case 'live-binding-runs-without-a-re-entry' format '#{copy_cursor_x}=0'
  type_both q
  copy_case 'live-binding-cancel' format '#{pane_in_mode}=0'
}

run_presentation() {
  attach_both_at
  set_on_both mode-keys emacs
  set_on_both status on
  side_command zz set-option -gu copy-mode-position-format >/dev/null 2>&1 || true
  side_command tmux set-option -gu copy-mode-position-format >/dev/null 2>&1 || true
  seed_pane

  copy_case 'presentation-live-status-row' none ''
  type_prefix_both '['
  copy_case 'presentation-copy-mode-position' format '#{pane_in_mode}=1'
  type_both q
  copy_case 'presentation-cancel' format '#{pane_in_mode}=0'

  set_on_both copy-mode-current-line-style 'bg=#00cd00,fg=#101010,bold'
  type_prefix_both '['
  copy_case 'presentation-current-line' format '#{pane_in_mode}=1'
  type_both C-p
  type_both C-p
  copy_case 'presentation-current-line-moved' none ''
  type_both q
  copy_case 'presentation-current-line-cancel' format '#{pane_in_mode}=0'
  set_on_both copy-mode-current-line-style default

  type_prefix_both '['
  copy_case 'presentation-current-line-plain' format '#{pane_in_mode}=1'
  set_on_both copy-mode-current-line-style 'fg=#101010,bg=#00cd00,curly-underscore'
  copy_case 'presentation-current-line-set-in-mode' none ''
  type_both C-p
  copy_case 'presentation-current-line-set-moved' none ''
  type_both q
  copy_case 'presentation-current-line-set-cancel' format '#{pane_in_mode}=0'
  set_on_both copy-mode-current-line-style default
}

SELF_CHECK_FAILURES=0

self_check_case() {
  local name="$1"
  local expectation="$2"
  local verdict=ok
  case "$expectation" in
  rows) [ "$LAST_ROWS_DIFFERED" -eq 1 ] || verdict='no row difference reported' ;;
  text) [ "$LAST_TEXT_DIFFERED" -eq 1 ] || verdict='no glyph difference reported' ;;
  style-only)
    if [ "$LAST_ROWS_DIFFERED" -ne 1 ] || [ "$LAST_TEXT_DIFFERED" -ne 0 ]; then
      verdict='the style-only difference did not land in the rows channel alone'
    fi
    ;;
  cursor) [ "$LAST_CURSOR_DIFFERED" -eq 1 ] || verdict='no cursor difference reported' ;;
  facts) [ "$LAST_FACTS_DIFFERED" -eq 1 ] || verdict='no copy-mode facts difference reported' ;;
  view) [ "$LAST_VIEW_DIFFERED" -eq 1 ] || verdict='no view difference reported' ;;
  view-only)
    if [ "$LAST_VIEW_DIFFERED" -ne 1 ] || [ "$LAST_FACTS_DIFFERED" -ne 0 ]; then
      verdict='the same line under a different view did not land in the view channel alone'
    fi
    ;;
  buffer) [ "$LAST_BUFFER_DIFFERED" -eq 1 ] || verdict='no paste-buffer difference reported' ;;
  formats) [ "$LAST_FORMATS_DIFFERED" -eq 1 ] || verdict='no mode-format difference reported' ;;
  none)
    if [ "$LAST_ROWS_DIFFERED" -ne 0 ] || [ "$LAST_TEXT_DIFFERED" -ne 0 ] ||
      [ "$LAST_CURSOR_DIFFERED" -ne 0 ] || [ "$LAST_FACTS_DIFFERED" -ne 0 ] ||
      [ "$LAST_VIEW_DIFFERED" -ne 0 ] || [ "$LAST_BUFFER_DIFFERED" -ne 0 ]; then
      verdict='reported a difference where both sides agree'
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

self_check_compare() {
  LAST_MISSING_OBSERVABLE=""
  CASE_LABEL="$1"
  wait_settled zz "$1 on the zz screen"
  wait_settled tmux "$1 on the tmux screen"
  compare_sides "$1" || true
}

run_self_check() {
  printf 'self-check: one deliberate one-sided difference per asserted channel, plus two the fixture must not report\n'

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side zz k
  await_observable zz format '#{pane_in_mode}=1' || true
  self_check_compare one-sided-movement
  self_check_case 'cursor: one extra cursor-up on the zz side' cursor
  self_check_case 'facts: one extra cursor-up on the zz side' facts
  self_check_case 'view: one extra cursor-up on the zz side' view

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side tmux PPage
  await_observable tmux format '#{scroll_position}=22' || true
  self_check_compare one-sided-page-up
  self_check_case 'rows: a page-up typed on the pin side only' rows
  self_check_case 'text: a page-up typed on the pin side only' text

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  side_command tmux set-option -g copy-mode-selection-style 'bg=#0000ff,fg=#ffffff' ||
    die 'tmux refused a one-sided copy-mode-selection-style'
  type_both Space
  type_both j
  type_both '$'
  self_check_compare one-sided-mode-style
  self_check_case 'rows alone: copy-mode-selection-style changed on the pin side only' style-only

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  side_command zz set-buffer 'zz-copied-bytes' || die 'zz refused set-buffer'
  side_command tmux set-buffer 'tmux-copied-bytes' || die 'tmux refused set-buffer'
  self_check_compare one-sided-buffer
  self_check_case 'buffer: a different byte string in the paste buffer' buffer

  attach_both_at
  set_on_both mode-keys emacs
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both C-p
  type_both C-p
  type_both C-p
  type_both C-p
  type_both C-a
  type_both R
  type_both C-Space
  type_both C-n
  type_both M-5
  type_both C-f
  type_side zz C-n
  type_both M-w
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  self_check_compare one-sided-rectangle-copy
  self_check_case 'buffer: one extra line inside a rectangle copy on the zz side' buffer

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side zz 5
  type_side zz k
  type_side tmux k
  type_side tmux k
  type_side tmux k
  type_side tmux k
  type_side tmux k
  self_check_compare counted-or-repeated
  self_check_case 'equivalence: 5k and five k land on the same cell' none

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both q
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  type_side zz C-b '['
  side_command tmux copy-mode -t "=$SESSION_NAME:0.0" || die 'tmux refused copy-mode'
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  self_check_compare chord-or-command
  self_check_case 'equivalence: prefix [ and the copy-mode command open the same mode' none

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both k
  type_both k
  type_side zz C-u
  type_side tmux 1
  type_side tmux 2
  type_side tmux k
  await_observable zz format '#{scroll_position}=12' || true
  await_observable tmux format '#{copy_cursor_y}=9' || true
  self_check_compare half-page-or-cursor-rows
  self_check_case 'view alone: a half page on the zz side, twelve cursor-ups on the pin side' view-only

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both k
  type_both k
  type_side zz PPage
  type_side tmux 2
  type_side tmux 4
  type_side tmux k
  await_observable zz format '#{scroll_position}=22' || true
  await_observable tmux format '#{scroll_position}=3' || true
  self_check_compare page-or-twenty-four-rows
  self_check_case 'facts: a page on the zz side, twenty-four cursor-ups on the pin side' facts

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both k
  type_both k
  type_both k
  type_both k
  type_both 0
  type_both v
  type_both Space
  type_both j
  local step
  for step in 1 2 3 4; do
    type_both 5
    type_both l
  done
  await_observable zz format '#{copy_cursor_x}=20' || true
  await_observable tmux format '#{copy_cursor_x}=20' || true
  side_command tmux set-option -g mode-keys emacs || die 'tmux refused a one-sided mode-keys'
  side_command zz send-keys -t "=$SESSION_NAME:0.0" -X copy-pipe-and-cancel ||
    die 'zz refused copy-pipe-and-cancel'
  side_command tmux send-keys -t "=$SESSION_NAME:0.0" -X copy-pipe-and-cancel ||
    die 'tmux refused copy-pipe-and-cancel'
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  self_check_compare rectangle-final-newline
  self_check_case 'buffer: the vi rectangle final newline kept on the zz side only' buffer

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both k
  type_both k
  type_side zz PPage
  type_side tmux C-b
  await_observable zz format '#{scroll_position}=22' || true
  self_check_compare copy-binding-or-prefix
  self_check_case 'facts: C-b as page-up on the zz side, as the prefix on the pin side' facts

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side zz /
  type_side zz -l needle
  type_side zz Enter
  type_side zz v
  type_side zz Space
  await_observable zz format '#{selection_active}=1' || true
  self_check_compare one-sided-mode-formats
  assert_mode_formats one-sided-mode-formats
  self_check_case 'formats: selection_active, rectangle_toggle and pane_search_string on the zz side only' formats

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side zz /
  await_observable zz screen '(search down)' || true
  self_check_compare one-sided-open-prompt
  self_check_case 'text: a search prompt open on the zz side only' text

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side zz /
  type_side zz -l needle
  type_side zz Enter
  await_observable zz format '#{copy_cursor_line}=line-12 needle-12' || true
  type_both q
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  assert_mode_formats one-sided-search-after-cancel
  self_check_case 'formats: pane_search_string after cancel, searched on the zz side only' formats
  self_check_compare one-sided-search-after-cancel-facts
  self_check_case 'facts: pane_search_string after cancel, searched on the zz side only' facts
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  assert_mode_formats one-sided-search-on-reentry
  self_check_case 'formats: pane_search_string on a fresh entry, searched on the zz side only' formats

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both /
  type_both -l needle
  type_both Enter
  await_observable zz format '#{copy_cursor_line}=line-12 needle-12' || true
  await_observable tmux format '#{copy_cursor_line}=line-12 needle-12' || true
  type_both q
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side tmux n
  await_observable tmux format '#{copy_cursor_line}=line-48 needle-48' || true
  self_check_compare one-sided-reentry-search-again
  self_check_case 'cursor: n on a fresh entry on the pin side only' cursor
  self_check_case 'facts: n on a fresh entry on the pin side only' facts

  type_both q
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_side tmux n
  type_side zz N
  await_observable tmux format '#{copy_cursor_line}=line-48 needle-48' || true
  await_observable zz format '#{copy_cursor_line}=line-12 needle-12' || true
  self_check_compare reentry-direction
  self_check_case 'facts: n (up) on the pin side, N (down) on the zz side on a fresh entry' facts

  attach_both_at
  set_on_both mode-keys vi
  seed_pane "$CLAMP_LINES_FILE"
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both 5
  type_both k
  await_observable zz format '#{copy_cursor_line}=aaaaaaaaaaaa' || true
  await_observable tmux format '#{copy_cursor_line}=aaaaaaaaaaaa' || true
  type_both NPage
  await_observable zz format '#{copy_cursor_y}=23' || true
  await_observable tmux format '#{copy_cursor_y}=23' || true
  type_side zz h
  await_observable zz format '#{copy_cursor_x}=0' || true
  self_check_compare page-clamp
  self_check_case 'cursor: the page landing on column 0 on the zz side only' cursor
  self_check_case 'facts: the page landing on column 0 on the zz side only' facts

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  type_both k
  type_both k
  type_side tmux C-b C-b
  await_observable tmux format '#{scroll_position}=22' || true
  self_check_compare send-prefix-one-sided
  self_check_case 'facts: C-b C-b on the pin side only' facts
  self_check_case 'view: C-b C-b on the pin side only' view

  attach_both_at
  set_on_both mode-keys vi
  seed_pane
  type_prefix_both '['
  await_observable zz format '#{pane_in_mode}=1' || true
  await_observable tmux format '#{pane_in_mode}=1' || true
  side_command tmux send-keys -t "=$SESSION_NAME:0.0" -N 2 Z k || die 'tmux refused send-keys'
  side_command tmux send-keys -t "=$SESSION_NAME:0.0" -X cancel || die 'tmux refused cancel'
  side_command zz send-keys -t "=$SESSION_NAME:0.0" -X cancel || die 'zz refused cancel'
  await_observable zz format '#{pane_in_mode}=0' || true
  await_observable tmux format '#{pane_in_mode}=0' || true
  side_command zz send-keys -t "=$SESSION_NAME:0.0" -N 2 Z k || die 'zz refused send-keys'
  await_observable zz screen 'ZkZk' || true
  self_check_compare send-keys-reach-the-shell
  self_check_case 'text: keys sent into copy mode reach the shell on the zz side only' text

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check expectations unmet\n' "$SELF_CHECK_FAILURES"
    return 1
  fi
  printf 'self-check complete: every sabotage was caught in its own channel and every equivalence passed\n'
  return 0
}

generate_lines
generate_clamp_lines
write_attach zz "$SCRATCH_DIR/attach-zz.sh"
write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"
zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
  exit $?
fi

printf 'copy mode and search through the client stdin at %sx%s (pin %s)\n' \
  "$COLUMNS_UNDER_TEST" "$ROWS_UNDER_TEST" "$(basename -- "$TMUX_BIN")"
run_movement emacs
run_movement vi
run_page_clamp emacs
run_page_clamp vi
run_search emacs
run_search vi
run_native_search_command
run_live_mode_keys
run_presentation

if [ "$FAILURES" -ne 0 ]; then
  printf '%s of %s cases differ in a channel they assert, %s recorded elsewhere\n' \
    "$FAILURES" "$CHECKS" "$RECORDS"
  exit 1
fi
printf 'all %s cases agree on every channel they assert, %s recorded a difference elsewhere\n' \
  "$CHECKS" "$RECORDS"
printf '%s of those cases assert at least one channel; %s assert none and are recorded in full\n' \
  "$ASSERTING" "$((CHECKS - ASSERTING))"

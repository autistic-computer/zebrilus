#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-choosers.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-choosers.sh\n' >&2
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
SCRATCH_DIR="$(mktemp -d /tmp/zzch.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzcho-$TOKEN"
INNER_SOCKET_NAME="zzchi-$TOKEN"
ZZ_SOCKET="/tmp/zzch-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="cho"
WINDOW_NAME="win"
PANE_TITLE="ptitle"
MANY_WINDOWS=12
BUFFER_MINUTE_MARGIN=15
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
CASE_LABEL=""
ZZ_PID=""
FAILURES=0
CHECKS=0
RECORDS=0
SIBLINGS=0
LAST_ROWS_DIFFERED=0
LAST_CURSOR_DIFFERED=0
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
COPY_POSITION_FORMAT='#[align=right][#{copy_position}/#{copy_position_limit}]'
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
  rm -f -- "$ZZ_SOCKET" "$ZZ_SOCKET.lock" "/tmp/tmux-$(id -u)/$OUTER_SOCKET_NAME" "/tmp/tmux-$(id -u)/$INNER_SOCKET_NAME"
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
screen_of() {
  capture_plain "$1" 2>/dev/null || true
}
styled_screen_of() {
  capture_screen "$1" 2>/dev/null || true
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
client_prefix_is() {
  [ "$(side_command "$1" display-message -c "$(client_name "$1")" -p '#{client_prefix}' 2>/dev/null)" = "$2" ]
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
type_on_side() {
  local side="$1"
  shift
  tmux_outer_command send-keys -t "=$OUTER_SESSION:$side" "$@" ||
    die "the outer tmux refused send-keys for $side"
}
type_on_both() {
  type_on_side zz "$@"
  type_on_side tmux "$@"
}

every_pane_output_is() {
  local side="$1"
  local pane
  for pane in $(side_command "$side" list-panes -a -F '#{pane_id}'); do
    side_command "$side" capture-pane -p -t "$pane" | grep -Fq "SCENE-$2" || return 1
  done
}

build_scene() {
  local side="$1"
  local index pane name
  side_command "$side" set-option -g automatic-rename off
  side_command "$side" new-window -d -t "=$INNER_SESSION:1" -n two "$INNER_SHELL" ||
    die "$side refused new-window"
  side_command "$side" split-window -d -h -t "=$INNER_SESSION:1" "$INNER_SHELL" ||
    die "$side refused split-window"
  side_command "$side" new-session -d -s many -n m00 -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" \
    "$INNER_SHELL" || die "$side refused new-session many"
  for ((index = 1; index < MANY_WINDOWS; index++)); do
    printf -v name 'm%02d' "$index"
    side_command "$side" new-window -d -t "=many:$index" -n "$name" "$INNER_SHELL" ||
      die "$side refused new-window $name"
  done
  side_command "$side" new-session -d -s other -n far -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" \
    "$INNER_SHELL" || die "$side refused new-session other"
  for pane in $(side_command "$side" list-panes -a -F '#{pane_id}'); do
    side_command "$side" select-pane -t "$pane" -T "$PANE_TITLE" || die "$side refused select-pane -T"
    side_command "$side" send-keys -t "$pane" "printf 'SCENE-%s\\n' $pane" Enter ||
      die "$side refused send-keys"
  done
}

seconds_into_minute_below() {
  [ "$((10#$(date +%S)))" -lt "$1" ]
}

make_buffers() {
  local attempt
  for ((attempt = 0; attempt < 400; attempt++)); do
    seconds_into_minute_below "$((60 - BUFFER_MINUTE_MARGIN))" && break
    sleep 0.05
  done
  seconds_into_minute_below "$((60 - BUFFER_MINUTE_MARGIN))" ||
    die 'a wall-clock minute with room for both buffers did not come within 20 seconds'
  run_on_both set-buffer -b alpha 'ALPHA-ONE'
  run_on_both set-buffer -b beta "$(printf 'BETA-ONE\nBETA-TWO\nBETA-THREE')"
}

attach_both_at() {
  local columns="$1"
  local rows="$2"
  local side pane
  COLUMNS_UNDER_TEST="$columns"
  ROWS_UNDER_TEST="$rows"
  tmux_outer_command kill-server >/dev/null 2>&1 || true
  for side in zz tmux; do
    side_command "$side" kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
    side_command "$side" kill-session -t =many >/dev/null 2>&1 || true
    side_command "$side" kill-session -t =other >/dev/null 2>&1 || true
    side_command "$side" delete-buffer -b alpha >/dev/null 2>&1 || true
    side_command "$side" delete-buffer -b beta >/dev/null 2>&1 || true
  done
  zz_command new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" -x "$columns" -y "$rows" \
    "$INNER_SHELL" || die "could not create the zz session"
  tmux_inner_command -f /dev/null new-session -d -s "$INNER_SESSION" -n "$WINDOW_NAME" \
    -x "$columns" -y "$rows" "$INNER_SHELL" || die "could not create the tmux session"
  set_on_both status-right ''
  set_on_both status-left L
  set_on_both automatic-rename off
  set_on_both copy-mode-position-format "$COPY_POSITION_FORMAT"
  build_scene zz
  build_scene tmux
  wait_for 'every zz pane printed its scene line' every_pane_output_is zz '%'
  wait_for 'every tmux pane printed its scene line' every_pane_output_is tmux '%'
  make_buffers
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$columns" -y "$rows" "$SCRATCH_DIR/attach-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/attach-tmux.sh"
  wait_for "outer zz pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane at ${columns}x${rows}" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  wait_for "zz client attached" client_attached zz
  wait_for "tmux client attached" client_attached tmux
  for side in zz tmux; do
    pane="$(active_pane "$side")"
    side_command "$side" select-pane -t "$pane" -T "$PANE_TITLE" || die "$side refused select-pane -T"
  done
}

wait_screen() {
  local side="$1"
  local hardness="$2"
  local label="$3"
  local before="$4"
  local needle="$5"
  local previous="" current plain attempt still=0
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(styled_screen_of "$side")"
    if [ -n "$previous" ] && [ "$current" = "$previous" ] &&
      { [ -z "$before" ] || [ "$current" != "$before" ]; }; then
      plain="$(screen_of "$side")"
      if [ -z "$needle" ] || printf '%s' "$plain" | grep -Fq -- "$needle"; then
        return 0
      fi
      still=$((still + 1))
      if [ "$hardness" = soft ] && [ "$still" -ge 40 ]; then
        printf 'note  %s: the %s screen changed and then held still for 2 seconds without it\n' "$label" "$side"
        return 0
      fi
    else
      still=0
    fi
    previous="$current"
    sleep 0.05
  done
  if [ "$hardness" = hard ]; then
    dump_state "$label"
    die "$label did not settle within 10 seconds"
  fi
  printf 'note  %s: the %s screen never settled on it within 10 seconds\n' "$label" "$side"
}

step() {
  local needle="$1"
  shift
  local before_zz before_tmux
  before_zz="$(styled_screen_of zz)"
  before_tmux="$(styled_screen_of tmux)"
  type_on_both "$@"
  wait_screen tmux hard "$CASE_LABEL: $needle on the tmux screen" "$before_tmux" "$needle"
  wait_screen zz soft "$CASE_LABEL: $needle on the zz screen" "$before_zz" "$needle"
}

prefix_step() {
  local needle="$1"
  local key="$2"
  type_on_both C-b
  wait_for 'the armed prefix on the zz client' client_prefix_is zz 1
  wait_for 'the armed prefix on the tmux client' client_prefix_is tmux 1
  step "$needle" "$key"
}

mark_both() {
  local name="$1"
  send_both "printf 'MARK-%s\\n' $name"
  wait_screen zz hard "MARK-$name on the zz screen" '' "MARK-$name"
  wait_screen tmux hard "MARK-$name on the tmux screen" '' "MARK-$name"
}

client_row_mask() {
  local name
  name="$(client_name "$1")"
  [ -n "$name" ] || die "the $1 side has no client name to mask"
  ZZ_MASK_NAME="$name" python3 -c '
import os, re, sys

name = re.escape(os.environ["ZZ_MASK_NAME"])
title = re.compile(
    r"^((?:\x1b\[[0-9;]*m)*\u250c (?:\x1b\[[0-9;]*m)*/dev/CLIENT)([^\u2502]*)(?=\u2502)"
)
TITLE_KEPT = 13
for line in sys.stdin.read().split("\n"):
    if re.search(name, line):
        line = re.sub(name, "/dev/CLIENT", line)
        line = title.sub(lambda cut: cut.group(1) + cut.group(2)[:TITLE_KEPT], line, count=1)
        line = re.sub(r"\d\d:\d\d", "NN:NN", line)
        line = re.sub("\u2500{2,}", "\u2500", line)
    sys.stdout.write(line + "\n")
'
}

client_info_mask() {
  local name
  name="$(client_name "$1")"
  [ -n "$name" ] || die "the $1 side has no client name to mask"
  ZZ_MASK_NAME="$name" python3 -c '
import os, re, sys

name = re.escape(os.environ["ZZ_MASK_NAME"])
stamp = r"[A-Z][a-z]{2} [A-Z][a-z]{2} [ 0-9]?[0-9] \d\d:\d\d:\d\d \d{4}"
TOKEN_FILL = (
    r"((?:/dev/CLIENT|\(PID NNNN\)|\(NN discarded\)|\(REL\)|TIMESTAMP|NN:NN|NNNN)(?:\x1b\[[0-9;]*m)*)( {2,})"
)
for line in sys.stdin.read().split("\n"):
    masked = re.sub(name, "/dev/CLIENT", line)
    masked = re.sub(r"\(PID \d+\)", "(PID NNNN)", masked)
    masked = re.sub(stamp, "TIMESTAMP", masked)
    masked = re.sub(r"(TIMESTAMP(?:\s|\x1b\[[0-9;]*m)*)\([^)]*\)", r"\g<1>(REL)", masked)
    masked = re.sub(r"(Bytes Written.*?\s)\d+", r"\g<1>NNNN", masked)
    masked = re.sub(r"\(\d+ discarded\)", "(NN discarded)", masked)
    masked = re.sub(r"\d\d:\d\d", "NN:NN", masked)
    if masked != line:
        masked = re.sub(TOKEN_FILL, r"\g<1> ", masked)
        masked = re.sub("\u2500{2,}", "\u2500", masked)
    sys.stdout.write(masked + "\n")
'
}

compare_rows() {
  local name="$1"
  local styled="$2"
  local zz_rows tmux_rows zz_cursor tmux_cursor index differing total count
  if [ "$styled" = styled ]; then
    mapfile -t zz_rows < <(capture_screen zz)
    mapfile -t tmux_rows < <(capture_screen tmux)
  else
    mapfile -t zz_rows < <(capture_plain zz)
    mapfile -t tmux_rows < <(capture_plain tmux)
  fi
  if [ -n "${ROW_MASK:-}" ]; then
    mapfile -t zz_rows < <(printf '%s\n' "${zz_rows[@]}" | "$ROW_MASK" zz)
    mapfile -t tmux_rows < <(printf '%s\n' "${tmux_rows[@]}" | "$ROW_MASK" tmux)
  fi
  zz_cursor="$(cursor_tuple zz)"
  tmux_cursor="$(cursor_tuple tmux)"
  total="$ROWS_UNDER_TEST"
  differing=-1
  count=0
  for ((index = 0; index < total; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      [ "$differing" -ge 0 ] || differing="$index"
      count=$((count + 1))
    fi
  done
  LAST_ROWS_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  [ "$differing" -lt 0 ] || LAST_ROWS_DIFFERED=1
  [ "$zz_cursor" = "$tmux_cursor" ] || LAST_CURSOR_DIFFERED=1
  if [ -n "${ZZ_CHOOSER_CAPTURE_DIR:-}" ]; then
    mkdir -p "$ZZ_CHOOSER_CAPTURE_DIR"
    printf '%s\n' "${tmux_rows[@]}" "cursor $tmux_cursor" >"$ZZ_CHOOSER_CAPTURE_DIR/$name.$styled.tmux.txt"
    printf '%s\n' "${zz_rows[@]}" "cursor $zz_cursor" >"$ZZ_CHOOSER_CAPTURE_DIR/$name.$styled.zz.txt"
  fi
  if [ "$LAST_ROWS_DIFFERED" -eq 0 ] && [ "$LAST_CURSOR_DIFFERED" -eq 0 ]; then
    return 0
  fi
  printf '      case %s\n' "$name"
  if [ "$LAST_ROWS_DIFFERED" -eq 1 ]; then
    printf '      %s of %s rows differ, first differing row %s\n' "$count" "$total" "$differing"
    for ((index = 0; index < total; index++)); do
      if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
        printf '        %2d tmux: %s\n' "$index" "$(printf '%s' "${tmux_rows[index]-}" | cat -v)"
        printf '        %2d zz:   %s\n' "$index" "$(printf '%s' "${zz_rows[index]-}" | cat -v)"
      fi
    done
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
  case "$reason" in
  SIBLING:*) SIBLINGS=$((SIBLINGS + 1)) ;;
  esac
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
    [ "$mode" = same ] || printf 'note  %s is identical, the record can close\n' "$name"
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

session_tree_case() {
  CASE_LABEL=session-tree
  mark_both sessions
  prefix_step "┌ $INNER_SESSION (sort: index)" s
  verdict session-tree-open same
  step '┌ many (sort: index)' j
  verdict session-tree-next same
  step '└─→ ' Right
  verdict session-tree-expanded same
  step 'MARK-sessions' q
  verdict session-tree-closed same
}

window_tree_case() {
  CASE_LABEL=window-tree
  mark_both windows
  prefix_step '┌ 0 (sort: index)' w
  verdict window-tree-open same
  step '0*: ' t
  verdict window-tree-tagged same
  step '0: win' T
  verdict window-tree-untagged same
  step '(sort: name)' O
  verdict window-tree-sorted same
  step '(sort: index)' O O O
  verdict window-tree-sort-wrapped same
  step '(search) ' F1 j /
  verdict window-tree-unfit-help-then-search same
  step '┌ 1 (sort: index)' t w o Enter
  verdict window-tree-searched same
  step '(M-h)' v
  verdict window-tree-preview-off same
  step '┌ 1 (sort' v
  verdict window-tree-preview-big same
  step '┌ 1 (sort' v
  verdict window-tree-preview-normal same
  step 'MARK-windows' q
  verdict window-tree-closed same
}

tall_case() {
  CASE_LABEL=tall
  attach_both_at 100 40
  mark_both tall
  prefix_step '┌ 0 (sort: index)' w
  verdict tall-window-tree-open same
  step 'Exit mode' F1
  verdict tall-window-tree-help same
  step '┌ 0 (sort: index)' j
  verdict tall-window-tree-help-closed same
  step 'MARK-tall' q
  prefix_step '(sort: creation)' =
  verdict tall-buffer-tree-open same
  step 'Exit mode' C-h
  verdict tall-buffer-tree-help same
  step '(sort: creation)' j
  verdict tall-buffer-tree-help-closed same
  step 'MARK-tall' q
  ROW_MASK=client_row_mask
  prefix_step 'session cho' D
  verdict tall-client-tree-open same
  step 'Suspend selected client' F1
  verdict tall-client-tree-help same
  step '(sort: name)' j
  verdict tall-client-tree-help-closed same
  step 'MARK-tall' q
  ROW_MASK=
  verdict tall-closed same
}

FILTER_FORMAT='#{==:#{window_name},two}'
LONG_FILTER=0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789
LONG_FILTER_EDITED="${LONG_FILTER:0:98}${LONG_FILTER:99}"
filter_case() {
  CASE_LABEL=filter
  mark_both filter
  prefix_step '┌ 0 (sort: index)' w
  step '(filter) ' f
  verdict filter-prompt same
  step "(filter) $FILTER_FORMAT" -l "$FILTER_FORMAT"
  verdict filter-typed same
  step '(filter: active)' Enter
  verdict filter-applied same
  step '0: win' c
  verdict filter-cleared same
  step "(filter) $FILTER_FORMAT" f Up
  verdict filter-history-up same
  step '(filter) ' Down
  verdict filter-history-down same
  step '0: win' C-c
  verdict filter-history-cancelled same
  step '(search) two' / t w o
  verdict search-typed same
  step '0: win' Enter
  verdict search-applied same
  step '(filter) two' f Up
  verdict filter-history-search same
  step '0: win' C-c
  verdict filter-history-search-cancelled same
  step '(filter) abXcd' f a b c d Left Left X
  verdict filter-edit-insert same
  step '(filter) YabXcdZ' C-a Y C-e Z
  verdict filter-edit-ends same
  step '(filter) YabXcd' Left C-k
  verdict filter-edit-kill same
  step '(filter) abXcd' C-a C-d
  verdict filter-edit-delete same
  step '(filter) abXc' End BSpace
  verdict filter-edit-backspace same
  step '(filter) aXc' Home Right DC
  verdict filter-edit-forward-delete same
  step '(filter) Xc' C-w
  verdict filter-edit-word same
  step '(filter) ' C-u
  verdict filter-edit-clear same
  step '(filter) ab cd-ef' -l 'ab cd-ef'
  step '(filter) ab cd-' C-w
  verdict filter-edit-word-separator same
  step '0: win' C-c
  verdict filter-edit-cancelled same
  step '(filter) ' f
  step "${LONG_FILTER:30}" -l "$LONG_FILTER"
  verdict filter-long-typed same
  step "${LONG_FILTER:29}" Left
  verdict filter-long-left same
  step "${LONG_FILTER_EDITED:28}" BSpace
  verdict filter-long-backspace same
  step '0: win' C-c
  verdict filter-long-cancelled same
  step '(search) abcd' / a b c d
  step '(search) Xabcd' C-a X
  verdict search-edit-start same
  step '0: win' C-c
  verdict search-edit-cancelled same
  set_on_both status-keys vi
  step '(filter) abcd' f a b c d
  verdict filter-vi-typed same
  step '(filter) abcd' Escape
  verdict filter-vi-command-mode same
  step '(filter) abd' h x
  verdict filter-vi-delete same
  step '(filter) Zabd' 0 i Z
  verdict filter-vi-insert same
  step '(filter) Zabd' Escape
  verdict filter-vi-command-again same
  step '0: win' q
  verdict filter-vi-cancelled same
  run_on_both set-option -gu status-keys
  step 'MARK-filter' q
  verdict filter-closed same
}

scroll_case() {
  CASE_LABEL=scroll
  mark_both scroll
  prefix_step '┌ 0 (sort: index)' w
  step '(M-h)' G
  verdict tree-scrolled-bottom same
  step '(0)' g
  verdict tree-scrolled-top same
  step 'MARK-scroll' q
  verdict tree-scroll-closed same
}

buffer_case() {
  CASE_LABEL=buffer-tree
  mark_both buffers
  prefix_step '(sort: creation)' =
  verdict buffer-tree-open same
  step '(sort: creation)' j
  verdict buffer-tree-next same
  step '*: ' t
  verdict buffer-tree-tagged same
  step 'MARK-buffers' q
  verdict buffer-tree-closed same
}

BUFFER_FILTER_FORMAT='#{==:#{buffer_name},alpha}'
buffer_filter_case() {
  CASE_LABEL=buffer-filter
  mark_both bufferfilter
  prefix_step '(sort: creation)' =
  step '(filter) ' f
  verdict buffer-filter-prompt same
  step "(filter) $BUFFER_FILTER_FORMAT" -l "$BUFFER_FILTER_FORMAT"
  verdict buffer-filter-typed same
  step '(filter: active)' Enter
  verdict buffer-filter-applied same
  step 'beta' c
  verdict buffer-filter-cleared same
  step 'MARK-bufferfilter' q
  verdict buffer-filter-closed same
}

find_window_case() {
  CASE_LABEL=find-window
  mark_both find
  prefix_step '(find-window)' f
  verdict find-window-prompt same
  step '(find-window) two' -l two
  verdict find-window-typed same
  step '(filter: active)' Enter
  verdict find-window-tree same
  step 'MARK-find' q
  verdict find-window-closed same
}

OUTPUT_REASON='the copied text is compared as a note because the view surfaces above it are compared whole'
command_output_case() {
  CASE_LABEL=command-output
  mark_both output
  local before_zz before_tmux
  before_zz="$(styled_screen_of zz)"
  before_tmux="$(styled_screen_of tmux)"
  on_both_active run-shell -t PANE 'seq 1 200'
  wait_screen tmux hard 'the run-shell output on the tmux screen' "$before_tmux" '/177]'
  wait_screen zz soft 'the run-shell output on the zz screen' "$before_zz" '/177]'
  verdict output-shown same
  step '(search down)' C-s
  verdict output-search-prompt same
  step '(search down)' -l 77
  verdict output-search-typed same
  step '' Enter
  verdict output-searched same
  type_on_both C-Space
  step '' Right Right
  verdict output-selected same
  step 'MARK-output' M-w
  verdict output-closed same
  wait_for 'the copied selection on the tmux side' top_buffer_is tmux buffer0
  if wait_soft top_buffer_is zz buffer0 &&
    [ "$(side_command zz show-buffer 2>/dev/null)" = "$(side_command tmux show-buffer 2>/dev/null)" ]; then
    printf 'note  output-copied: the copied selection is the same text on both sides\n'
  else
    printf 'note  output-copied recorded, not asserted: %s: tmux copied %q, zz copied %q\n' \
      "$OUTPUT_REASON" "$(side_command tmux show-buffer 2>/dev/null)" "$(side_command zz show-buffer 2>/dev/null)"
  fi
}
top_buffer_is() {
  [ "$(side_command "$1" list-buffers -F '#{buffer_name}' 2>/dev/null | head -n 1)" = "$2" ]
}
wait_soft() {
  local attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    if "$@" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

client_case() {
  CASE_LABEL=client-tree
  ROW_MASK=client_row_mask
  mark_both clients
  prefix_step 'session cho' D
  verdict client-tree-open same
  step '(sort: size)' O
  verdict client-sort-size same
  step '(sort: creation)' O
  verdict client-sort-creation same
  step '(sort: activity)' O
  verdict client-sort-activity same
  step '(sort: name)' O
  verdict client-sort-name same
  step '(sort: name, reversed)' r
  verdict client-sort-reversed same
  step '(sort: name)' r
  verdict client-sort-forward same
  step '*: ' t
  verdict client-tagged same
  step '(sort: name)' T
  verdict client-untagged same
  step '*: ' C-t
  verdict client-tag-all same
  step '(sort: name)' T
  verdict client-tag-all-cleared same
  ROW_MASK=client_info_mask
  step '(view: info)' i
  verdict client-info-view same
  step '(view: preview)' i
  verdict client-info-view-off same
  ROW_MASK=client_row_mask
  step '(filter) ' f
  verdict client-filter-prompt same
  step '(sort: name)' Escape
  verdict client-filter-cancelled same
  step 'MARK-clients' q
  verdict client-tree-closed same
  run_on_both bind-key -T prefix Y choose-client -Z "display-message -d 0 CHOSE-%%"
  prefix_step 'session cho' Y
  verdict client-run-open same
  local zz_name tmux_name before_zz before_tmux
  zz_name="$(client_name zz)"
  tmux_name="$(client_name tmux)"
  [ -n "$zz_name" ] && [ -n "$tmux_name" ] || die 'a selected client has no name'
  before_zz="$(styled_screen_of zz)"
  before_tmux="$(styled_screen_of tmux)"
  type_on_both Enter
  wait_screen tmux hard "$CASE_LABEL: CHOSE-$tmux_name on the tmux screen" "$before_tmux" "CHOSE-$tmux_name"
  wait_screen zz soft "$CASE_LABEL: CHOSE-$zz_name on the zz screen" "$before_zz" "CHOSE-$zz_name"
  verdict client-run-chosen same
  step 'MARK-clients' C-c
  verdict client-run-dismissed same
  run_on_both unbind-key -T prefix Y
  ROW_MASK=
}

PREVIEW_FLAG_KEY=Y
preview_flag_step() {
  local command="$1" flags="$2" open_needle="$3" cycled_needle="$4" name="$5"
  run_on_both bind-key -T prefix "$PREVIEW_FLAG_KEY" "$command" "$flags"
  prefix_step "$open_needle" "$PREVIEW_FLAG_KEY"
  verdict "preview-flag-$name-open" same
  step "$cycled_needle" v
  verdict "preview-flag-$name-cycled" same
  step 'MARK-previewflags' q
  verdict "preview-flag-$name-closed" same
}
preview_flags_case() {
  CASE_LABEL=preview-flags
  mark_both previewflags
  preview_flag_step choose-tree -N '(0)' '(sort: index)' tree-off
  preview_flag_step choose-tree -NN '(sort: index)' '(sort: index)' tree-big
  preview_flag_step choose-buffer -N '(0)' '(sort: creation)' buffer-off
  preview_flag_step choose-buffer -NN '(sort: creation)' '(sort: creation)' buffer-big
  ROW_MASK=client_row_mask
  preview_flag_step choose-client -N 'session cho' '(sort: name)' client-off
  preview_flag_step choose-client -NN '(sort: name)' '(sort: name)' client-big
  ROW_MASK=
  run_on_both unbind-key -T prefix "$PREVIEW_FLAG_KEY"
}

ZOOMED_STATUS="L0:$WINDOW_NAME*Z"
UNZOOMED_STATUS="L0:$WINDOW_NAME* 1:two"
zoom_case() {
  CASE_LABEL=zoom
  local side pane
  for side in zz tmux; do
    side_command "$side" split-window -d -h -t "=$INNER_SESSION:$WINDOW_NAME" "$INNER_SHELL" ||
      die "$side refused split-window"
    pane="$(side_command "$side" list-panes -t "=$INNER_SESSION:$WINDOW_NAME" \
      -F '#{pane_active} #{pane_id}' | awk '$1 == 0 { print $2; exit }')"
    [ -n "$pane" ] || die "$side has no second pane in $WINDOW_NAME"
    side_command "$side" select-pane -t "$pane" -T "$PANE_TITLE" || die "$side refused select-pane -T"
    side_command "$side" send-keys -t "$pane" "printf 'ZOOM-SCENE\\n'" Enter ||
      die "$side refused send-keys"
  done
  wait_screen tmux hard 'the split pane on the tmux screen' '' 'ZOOM-SCENE'
  wait_screen zz hard 'the split pane on the zz screen' '' 'ZOOM-SCENE'
  mark_both zoom
  prefix_step "$ZOOMED_STATUS" w
  verdict zoom-tree-open same
  step "$UNZOOMED_STATUS" q
  verdict zoom-tree-closed same
  prefix_step "$ZOOMED_STATUS" z
  verdict zoom-manual same
  prefix_step '┌ 0 (sort: index)' w
  verdict zoom-prezoomed-open same
  step 'MARK-zoom' q
  verdict zoom-prezoomed-closed same
  prefix_step "$UNZOOMED_STATUS" z
  verdict zoom-manual-released same
}

write_attach zz "$SCRATCH_DIR/attach-zz.sh"
write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"

zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

STYLED_TREE_OPTIONS=(
  tree-mode-selection-style 'bg=red,fg=white,bold'
  tree-mode-border-style 'bg=blue,fg=yellow'
  tree-mode-preview-style 'fg=#{?pane_format,green,magenta},underscore'
  tree-mode-preview-format '#{?pane_format,P#{pane_index},W#{window_index}}-#{window_name}'
)
styles_case() {
  CASE_LABEL=styles
  local index
  for ((index = 0; index < ${#STYLED_TREE_OPTIONS[@]}; index += 2)); do
    set_on_both "${STYLED_TREE_OPTIONS[index]}" "${STYLED_TREE_OPTIONS[index + 1]}"
  done
  mark_both styles
  prefix_step "┌ $INNER_SESSION (sort: index)" s
  verdict styles-session-tree same
  step '┌ many (sort: index)' j
  verdict styles-session-tree-next same
  step 'MARK-styles' q
  prefix_step '┌ 0 (sort: index)' w
  verdict styles-window-tree same
  step 'P1-two' j
  verdict styles-window-tree-panes same
  step 'Exit mode' F1
  verdict styles-help same
  step '┌ 1 (sort: index)' j
  step 'MARK-styles' q
  attach_both_at 80 24
  mark_both styledclients
  ROW_MASK=client_row_mask
  prefix_step 'session cho' D
  verdict styles-client-tree same
  ROW_MASK=client_info_mask
  step '(view: info)' i
  verdict styles-client-info same
  ROW_MASK=client_row_mask
  step 'MARK-styledclients' q
  ROW_MASK=
  for ((index = 0; index < ${#STYLED_TREE_OPTIONS[@]}; index += 2)); do
    run_on_both set-option -gu "${STYLED_TREE_OPTIONS[index]}"
  done
  verdict styles-closed same
}

run_cases() {
  printf 'chooser and command-output differential at %sx%s (pin %s)\n' \
    "$COLUMNS_UNDER_TEST" "$ROWS_UNDER_TEST" "$(basename -- "$TMUX_BIN")"
  attach_both_at 80 24
  CASE_LABEL=baseline
  mark_both baseline
  verdict baseline same
  session_tree_case
  window_tree_case
  filter_case
  scroll_case
  buffer_case
  buffer_filter_case
  find_window_case
  command_output_case
  client_case
  preview_flags_case
  zoom_case
  tall_case
  styles_case

  if [ "$FAILURES" -ne 0 ]; then
    printf '%s of %s asserted comparisons differ, %s recorded (%s for a sibling lane)\n' \
      "$FAILURES" "$CHECKS" "$RECORDS" "$SIBLINGS"
    exit 1
  fi
  printf 'all %s asserted comparisons identical, %s recorded not asserted (%s for a sibling lane)\n' \
    "$CHECKS" "$RECORDS" "$SIBLINGS"
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
    if [ "$LAST_CURSOR_DIFFERED" -ne 1 ]; then
      outcome='no cursor difference reported'
    elif [ "$LAST_ROWS_DIFFERED" -ne 0 ]; then
      outcome='a cursor-only sabotage was reported in the rows'
    fi
    ;;
  none)
    if [ "$LAST_ROWS_DIFFERED" -ne 0 ] || [ "$LAST_CURSOR_DIFFERED" -ne 0 ]; then
      outcome='reported a difference where both sides are the same'
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

planted_on_zz() {
  side_command zz capture-pane -p -t "=$INNER_SESSION:1.1" | grep -q '^PLANTED'
}
outer_cursor_is() {
  [ "$(cursor_tuple zz)" = "$1" ]
}
outer_cursor_is_not() {
  [ "$(cursor_tuple zz)" != "$1" ]
}

self_check_info_mask_case() {
  local name="$1" expectation="$2" left="$3" right="$4"
  LAST_ROWS_DIFFERED=0
  LAST_CURSOR_DIFFERED=0
  if [ "$(printf '%s\n' "$left" | client_info_mask zz)" != \
    "$(printf '%s\n' "$right" | client_info_mask zz)" ]; then
    LAST_ROWS_DIFFERED=1
  fi
  self_check_case "info mask, $name" "$expectation"
}

run_self_check() {
  printf 'self-check: one deliberate difference per channel, plus equivalences and info-mask checks\n'
  attach_both_at 80 24

  local info_short info_long
  printf -v info_short '│ Bytes Written │ 12345 (0 discarded)\033[39m%41s│' ''
  printf -v info_long '│ Bytes Written │ 123456789 (0 discarded)\033[39m%37s│' ''
  self_check_info_mask_case 'styled counter widths' none "$info_short" "$info_long"
  self_check_info_mask_case 'plain counter widths' none \
    "${info_short//$'\033[39m'/}" "${info_long//$'\033[39m'/}"
  self_check_info_mask_case 'colour' rows "$info_short" "${info_short/$'\033[39m'/$'\033[31m'}"
  self_check_info_mask_case 'label alignment' rows "$info_short" "${info_short/Written │/Written  │}"
  self_check_info_mask_case 'label text' rows "$info_short" "${info_short/Written/WrittenX}"
  self_check_info_mask_case 'border' rows "$info_short" "${info_short%│}┃"
  self_check_info_mask_case 'extra reset' rows "$info_short" "${info_short/$'\033[39m'/$'\033[0m\033[39m'}"

  CASE_LABEL='self-check equivalence'
  mark_both equal
  prefix_step '┌ 0 (sort: index)' w
  compare_rows self-check-equivalence styled || true
  self_check_case 'equivalence: the same window tree on both sides' none

  CASE_LABEL='self-check tag'
  local before
  before="$(styled_screen_of tmux)"
  type_on_side tmux t
  wait_screen tmux hard 'the one-sided tag' "$before" '0*: '
  compare_rows self-check-tag styled || true
  self_check_case 'tag, a mark on one side only' rows
  type_on_side tmux T k
  wait_screen tmux hard 'the one-sided tag withdrawn' '' '┌ 0 (sort: index)'
  step 'MARK-equal' q

  CASE_LABEL='self-check preview'
  side_command zz send-keys -t "=$INNER_SESSION:1.1" "printf 'PLANTED\\n'" Enter ||
    die 'zz refused send-keys'
  wait_for 'the planted preview line' planted_on_zz
  mark_both preview
  prefix_step '┌ 0 (sort: index)' w
  step '┌ 1 (sort: index)' j
  compare_rows self-check-preview styled || true
  self_check_case 'preview, one cell of one previewed pane differs' rows
  step 'MARK-preview' q

  CASE_LABEL='self-check row'
  side_command zz rename-window -t "=$INNER_SESSION:1" twx || die 'zz refused rename-window'
  mark_both row
  prefix_step '┌ 0 (sort: index)' w
  compare_rows self-check-row styled || true
  self_check_case 'row, one tree row differs on one side' rows
  step 'MARK-row' q
  side_command zz rename-window -t "=$INNER_SESSION:1" two || die 'zz refused rename-window'

  CASE_LABEL='self-check cursor'
  attach_both_at 80 24
  mark_both cursor
  prefix_step '┌ 0 (sort: index)' w
  step 'MARK-cursor' q
  local zz_before
  zz_before="$(cursor_tuple zz)"
  type_on_side zz Space
  wait_for 'the one-sided space' outer_cursor_is_not "$zz_before"
  compare_rows self-check-cursor styled || true
  self_check_case 'cursor, one column at one prompt' cursor
  type_on_side zz BSpace

  CASE_LABEL='self-check restoration'
  wait_for 'the space withdrawn' outer_cursor_is "$zz_before"
  compare_rows self-check-restoration styled || true
  self_check_case 'equivalence: the pane restored after a chooser' none

  CASE_LABEL='self-check client info view'
  ROW_MASK=client_row_mask
  mark_both clientinfo
  prefix_step 'session cho' D
  before="$(styled_screen_of tmux)"
  type_on_side tmux i
  wait_screen tmux hard 'the one-sided info view' "$before" 'Client Name'
  compare_rows self-check-client-info styled || true
  self_check_case 'client info, the pin client tree on its info view alone' rows
  type_on_side tmux i
  wait_screen tmux hard 'the one-sided info view withdrawn' '' 'session cho'

  ROW_MASK=client_info_mask
  before="$(styled_screen_of zz)"
  type_on_side zz i
  wait_screen zz hard 'the one-sided info view on zz' "$before" 'Client Name'
  compare_rows self-check-client-info-zz styled || true
  self_check_case 'client info, the zz client tree on its info view alone' rows
  type_on_side zz i
  wait_screen zz hard 'the one-sided info view on zz withdrawn' '' 'session cho'
  ROW_MASK=client_row_mask
  step 'MARK-clientinfo' q
  ROW_MASK=

  CASE_LABEL='self-check zoom'
  local side pane
  for side in zz tmux; do
    side_command "$side" split-window -d -h -t "=$INNER_SESSION:$WINDOW_NAME" "$INNER_SHELL" ||
      die "$side refused split-window"
    pane="$(side_command "$side" list-panes -t "=$INNER_SESSION:$WINDOW_NAME" \
      -F '#{pane_active} #{pane_id}' | awk '$1 == 0 { print $2; exit }')"
    [ -n "$pane" ] || die "$side has no second pane in $WINDOW_NAME"
    side_command "$side" select-pane -t "$pane" -T "$PANE_TITLE" || die "$side refused select-pane -T"
    side_command "$side" send-keys -t "$pane" "printf 'ZOOM-SCENE\\n'" Enter ||
      die "$side refused send-keys"
  done
  wait_screen tmux hard 'the split pane on the tmux screen' '' 'ZOOM-SCENE'
  wait_screen zz hard 'the split pane on the zz screen' '' 'ZOOM-SCENE'
  mark_both zoomed
  before="$(styled_screen_of tmux)"
  type_on_side tmux C-b
  wait_for 'the armed prefix on the tmux client' client_prefix_is tmux 1
  type_on_side tmux z
  wait_screen tmux hard 'the one-sided zoom' "$before" "$ZOOMED_STATUS"
  compare_rows self-check-zoom styled || true
  self_check_case 'zoom, the attached window zoomed on one side only' rows
  before="$(styled_screen_of tmux)"
  type_on_side tmux C-b
  wait_for 'the armed prefix on the tmux client' client_prefix_is tmux 1
  type_on_side tmux z
  wait_screen tmux hard 'the one-sided zoom withdrawn' "$before" "$UNZOOMED_STATUS"

  CASE_LABEL='self-check command output'
  attach_both_at 80 24
  mark_both outputsab
  before="$(styled_screen_of tmux)"
  on_both_active run-shell -t PANE 'seq 1 200'
  wait_screen tmux hard 'the run-shell output on the tmux screen' "$before" '/177]'
  wait_screen zz hard 'the run-shell output on the zz screen' '' '/177]'
  before="$(styled_screen_of tmux)"
  type_on_side tmux C-s
  wait_screen tmux hard 'the one-sided pane prompt' "$before" '(search down)'
  compare_rows self-check-pane-prompt styled || true
  self_check_case 'pane prompt, the stock search prompt over one pane only' rows
  before="$(styled_screen_of tmux)"
  type_on_side tmux -l 77
  type_on_side tmux Enter
  wait_screen tmux hard 'the one-sided search marks' "$before" ''
  compare_rows self-check-search-marks styled || true
  self_check_case 'search marks, the current match painted on one side only' rows
  type_on_side tmux q
  wait_screen tmux hard 'the pin back on its pane' '' 'MARK-outputsab'
  type_on_side zz q
  wait_screen zz hard 'zz back on its pane' '' 'MARK-outputsab'

  CASE_LABEL='self-check buffer filter prompt'
  mark_both prompt
  prefix_step '(sort: creation)' =
  before="$(styled_screen_of tmux)"
  type_on_side tmux f
  wait_screen tmux hard 'the one-sided filter prompt' "$before" '(filter) '
  compare_rows self-check-buffer-filter-prompt styled || true
  self_check_case 'prompt, the buffer tree filter prompt on one side only' rows

  CASE_LABEL='self-check title cut'
  attach_both_at 100 40
  ROW_MASK=client_row_mask
  mark_both titlecut
  prefix_step 'session cho' D
  before="$(styled_screen_of zz)"
  type_on_side zz O
  wait_screen zz hard 'the sort order changed on the zz side alone' "$before" '(sort: size)'
  step 'Exit mode' F1
  compare_rows self-check-title-cut styled || true
  self_check_case 'title cut, the sort label inside the span a box cuts' rows
  ROW_MASK=

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check expectations unmet\n' "$SELF_CHECK_FAILURES"
    exit 1
  fi
  printf 'self-check complete: every sabotage was caught and all equivalences passed\n'
}

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
else
  run_cases
fi

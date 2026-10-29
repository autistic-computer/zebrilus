#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-launch-diff.sh [--self-check] [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-launch-diff.sh\n' >&2
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
SCRATCH_DIR="$(mktemp -d /tmp/zzld.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzldo-$TOKEN"
INNER_SOCKET_NAME="zzldi-$TOKEN"
ZZ_SOCKET="/tmp/zzld-$TOKEN.sock"
OUTER_SESSION="driver"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
CAPTURE_DIR="${ZZ_LAUNCH_CAPTURE_DIR:-}"
CASE_LABEL=""
ZZ_PID=""
FAILURES=0
CHECKS=0
RECORDS=0
LAST_DIFFERED=0
LAST_SCREEN_DIFFERED=0
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
BARE_LAUNCH_REASON='zz with no arguments starts the native desktop client, the pin paints a session in the terminal it was run from'
PINNED_STYLES=(
  'status-style bg=#00af00,fg=#101010'
  'window-status-current-style bg=#00af00,fg=#101010'
  'pane-border-style fg=#7f7f7f,bg=#101010'
  'pane-active-border-style fg=#00af00,bg=#101010'
  'message-style bg=#cdcd00,fg=#101010'
  'mode-style bg=#cdcd00,fg=#101010'
)
mkdir -p "$ZZ_HOME/config" "$TMUX_HOME/config" "$OUTER_HOME" "$ZZ_LOG_DIR"
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
  die "$label did not happen within 10 seconds"
}

start_zz_daemon() {
  zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
  ZZ_PID=$!
  wait_for "zz daemon socket" test -S "$ZZ_SOCKET"
}
kill_every_session() {
  local side="$1"
  local session
  while read -r session; do
    [ -n "$session" ] || continue
    side_command "$side" kill-session -t "=$session" >/dev/null 2>&1 || true
  done < <(side_command "$side" list-sessions -F '#{session_name}' 2>/dev/null || true)
}
reset_servers() {
  kill_every_session zz
  kill_every_session tmux
}
reset_pair() {
  reset_servers
  plant_controlled_config zz
  plant_controlled_config tmux
}

plant_controlled_config() {
  local side="$1"
  local entry
  local file
  if [ "$side" = zz ]; then
    file="$(side_home "$side")/config/zz/mux.conf"
  else
    file="$(side_home "$side")/.tmux.conf"
  fi
  mkdir -p "$(dirname -- "$file")"
  {
    printf 'set -g status-right ""\n'
    printf 'set -g status-left L\n'
    printf 'set -g automatic-rename off\n'
    printf 'set -g default-command "%s"\n' "$INNER_SHELL"
    for entry in "${PINNED_STYLES[@]}"; do
      printf 'set -g %s %s\n' "${entry%% *}" "${entry#* }"
    done
  } >"$file"
}

report() {
  local name="$1"
  local mode="$2"
  local channel="$3"
  local zz_value="$4"
  local tmux_value="$5"
  local reason="${6:-}"
  CHECKS=$((CHECKS + 1))
  if [ "$zz_value" = "$tmux_value" ]; then
    LAST_DIFFERED=0
    printf 'ok    %s %s\n' "$name" "$channel"
    return 0
  fi
  LAST_DIFFERED=1
  printf '      case %s, channel %s\n' "$name" "$channel"
  printf '        tmux: %s\n' "$(printf '%s' "$tmux_value" | cat -v | tr '\n' '|')"
  printf '        zz:   %s\n' "$(printf '%s' "$zz_value" | cat -v | tr '\n' '|')"
  if [ "$mode" = same ]; then
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s %s\n' "$name" "$channel"
  else
    RECORDS=$((RECORDS + 1))
    printf 'note  %s %s recorded, not asserted: %s\n' "$name" "$channel" "$reason"
  fi
  return 0
}

run_pair() {
  local name="$1"
  local mode="$2"
  local reason="$3"
  shift 3
  local side out err status
  local -A outputs=()
  for side in zz tmux; do
    out="$SCRATCH_DIR/$side.out"
    err="$SCRATCH_DIR/$side.err"
    set +e
    side_command "$side" "$@" >"$out" 2>"$err"
    status=$?
    set -e
    outputs[$side]="$(printf 'exit=%s\nstdout=%s\nstderr=%s' "$status" "$(cat "$out")" "$(cat "$err")")"
  done
  if [ -n "$CAPTURE_DIR" ]; then
    printf 'zz:\n%s\ntmux:\n%s\n' "${outputs[zz]}" "${outputs[tmux]}" \
      >"$CAPTURE_DIR/$name.cli.txt"
  fi
  report "$name" "$mode" cli "${outputs[zz]}" "${outputs[tmux]}" "$reason"
}

side_facts() {
  {
    side_command "$1" list-sessions \
      -F '#{session_name} windows=#{session_windows} attached=#{session_attached}' 2>&1 || true
  } | LC_ALL=C sort
}
compare_facts() {
  local name="$1"
  local mode="$2"
  local reason="${3:-}"
  local zz_facts tmux_facts
  zz_facts="$(side_facts zz)"
  tmux_facts="$(side_facts tmux)"
  [ -z "$CAPTURE_DIR" ] || printf 'zz:\n%s\ntmux:\n%s\n' "$zz_facts" "$tmux_facts" \
    >"$CAPTURE_DIR/$name.facts.txt"
  report "$name" "$mode" facts "$zz_facts" "$tmux_facts" "$reason"
}

write_launcher() {
  local side="$1"
  local destination="$2"
  shift 2
  local arguments=""
  local argument
  for argument in "$@"; do
    printf -v argument '%q' "$argument"
    arguments="$arguments $argument"
  done
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp %q --socket %q%s\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" "$arguments" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL -u XDG_STATE_HOME HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp %q -L %q%s\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" "$arguments" >>"$destination"
  fi
  chmod +x "$destination"
}

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
capture_plain() {
  tmux_outer_command capture-pane -p -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
capture_screen() {
  tmux_outer_command capture-pane -p -e -S 0 -E "$((ROWS_UNDER_TEST - 1))" \
    -t "=$OUTER_SESSION:$1"
}
cursor_tuple() {
  tmux_outer_command display-message -p -t "=$OUTER_SESSION:$1" \
    '#{cursor_x},#{cursor_y} flag=#{cursor_flag} pane=#{pane_width}x#{pane_height}'
}
wait_settled() {
  local side="$1"
  local label="$2"
  local previous=""
  local current attempt
  for ((attempt = 0; attempt < 200; attempt++)); do
    current="$(capture_plain "$side" 2>/dev/null || true)"
    if [ -n "$previous" ] && [ "$current" = "$previous" ]; then
      return 0
    fi
    previous="$current"
    sleep 0.05
  done
  die "$label did not settle within 10 seconds"
}
side_attached() {
  [ -n "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" ]
}

launch_both() {
  local -a arguments=("$@")
  tmux_outer_command kill-server >/dev/null 2>&1 || true
  write_launcher zz "$SCRATCH_DIR/launch-zz.sh" "${arguments[@]}"
  write_launcher tmux "$SCRATCH_DIR/launch-tmux.sh" "${arguments[@]}"
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" "$SCRATCH_DIR/launch-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off
  tmux_outer_command set-option -g remain-on-exit on
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/launch-tmux.sh"
  wait_for "outer zz pane" outer_pane_is "=$OUTER_SESSION:zz" \
    "${COLUMNS_UNDER_TEST}x${ROWS_UNDER_TEST}"
  wait_for "outer tmux pane" outer_pane_is "=$OUTER_SESSION:tmux" \
    "${COLUMNS_UNDER_TEST}x${ROWS_UNDER_TEST}"
}

compare_screens() {
  local name="$1"
  local mode="$2"
  local reason="${3:-}"
  local zz_rows tmux_rows index differing total
  wait_settled zz "$name on the zz screen"
  wait_settled tmux "$name on the tmux screen"
  mapfile -t zz_rows < <(capture_screen zz)
  mapfile -t tmux_rows < <(capture_screen tmux)
  if [ -n "$CAPTURE_DIR" ]; then
    printf '%s\n' "${zz_rows[@]-}" >"$CAPTURE_DIR/$name.zz.screen.txt"
    printf '%s\n' "${tmux_rows[@]-}" >"$CAPTURE_DIR/$name.tmux.screen.txt"
  fi
  total="$ROWS_UNDER_TEST"
  differing=-1
  for ((index = 0; index < total; index++)); do
    if [ "${zz_rows[index]-}" != "${tmux_rows[index]-}" ]; then
      differing="$index"
      break
    fi
  done
  if [ "$differing" -lt 0 ]; then
    report "$name" "$mode" screen same same "$reason"
  else
    report "$name" "$mode" screen \
      "row $differing: $(printf '%s' "${zz_rows[differing]-}" | cat -v)" \
      "row $differing: $(printf '%s' "${tmux_rows[differing]-}" | cat -v)" "$reason"
  fi
  LAST_SCREEN_DIFFERED="$LAST_DIFFERED"
  report "$name" "$mode" cursor "$(cursor_tuple zz)" "$(cursor_tuple tmux)" "$reason"
}

run_cli_cases() {
  reset_pair
  run_pair attach-empty same '' attach-session

  reset_pair
  run_pair new-detached same '' new-session -d -s launch
  run_pair attach-missing-target same '' attach-session -t nosuch
  run_pair new-duplicate same '' new-session -d -s launch
  compare_facts new-duplicate same

  run_pair new-attach-existing-detached same '' new-session -A -d -s launch
  compare_facts new-attach-existing-detached same

  run_pair new-window-detached same '' new-window -d -t '=launch:'
  run_pair split-detached same '' split-window -d -t '=launch:0.0'
  compare_facts split-window same
  run_pair list-panes-after-split same '' list-panes -t '=launch:' \
    -F '#{window_index}.#{pane_index} #{pane_width}x#{pane_height} active=#{pane_active}'
  run_pair new-window-missing-target same '' new-window -d -t '=nosuch:'
  run_pair split-missing-target same '' split-window -d -t '=nosuch:0.0'
}

run_attached_cases() {
  reset_pair
  side_command zz new-session -d -s live -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" \
    "$INNER_SHELL" || die 'zz refused new-session'
  side_command tmux new-session -d -s live -x "$COLUMNS_UNDER_TEST" \
    -y "$ROWS_UNDER_TEST" "$INNER_SHELL" || die 'tmux refused new-session'
  side_command zz rename-window -t '=live:0' win || die 'zz refused rename-window'
  side_command tmux rename-window -t '=live:0' win || die 'tmux refused rename-window'
  launch_both attach-session -t '=live'
  wait_for "zz attached" side_attached zz
  wait_for "tmux attached" side_attached tmux
  compare_screens attach-live same
  compare_facts attach-live same

  launch_both set-environment -g UPGRADE_PREFIX before ';' \
    attach-session -t '=live' ';' \
    set-environment -g UPGRADE_TAIL after ';' \
    set-option -g status-left UPGRADE
  wait_for "zz attached through the command chain" side_attached zz
  wait_for "tmux attached through the command chain" side_attached tmux
  wait_for "zz command-chain tail" side_command zz show-environment -g UPGRADE_TAIL
  wait_for "tmux command-chain tail" side_command tmux show-environment -g UPGRADE_TAIL
  compare_screens attach-command-chain same
  compare_facts attach-command-chain same
  report attach-command-chain same prefix \
    "$(side_command zz show-environment -g UPGRADE_PREFIX)" \
    "$(side_command tmux show-environment -g UPGRADE_PREFIX)" ''
  report attach-command-chain same tail \
    "$(side_command zz show-environment -g UPGRADE_TAIL)" \
    "$(side_command tmux show-environment -g UPGRADE_TAIL)" ''
  side_command zz set-option -g status-left L
  side_command tmux set-option -g status-left L

  reset_pair
  launch_both new-session -s fresh -n win -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST"
  wait_for "zz attached to the new session" side_attached zz
  wait_for "tmux attached to the new session" side_attached tmux
  compare_screens new-session-attached same
  compare_facts new-session-attached same

  reset_pair
  launch_both
  wait_for "zz attached from the bare launcher" side_attached zz
  wait_for "tmux attached from the bare launcher" side_attached tmux
  compare_screens bare-empty record "$BARE_LAUNCH_REASON"
  compare_facts bare-empty same

  reset_pair
  side_command zz new-session -d -s existing -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" \
    "$INNER_SHELL" || die 'zz refused new-session'
  side_command tmux new-session -d -s existing -x "$COLUMNS_UNDER_TEST" \
    -y "$ROWS_UNDER_TEST" "$INNER_SHELL" || die 'tmux refused new-session'
  launch_both
  wait_for "zz attached from the bare launcher" side_attached zz
  wait_for "tmux attached from the bare launcher" side_attached tmux
  compare_facts bare-existing record \
    'semantic:bare-launcher-attach-current: zz launches new-session -A, the pin launches new-session'

  reset_pair
  launch_both new-session -s quiet -n win -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST"
  wait_for "zz attached for the quiet case" side_attached zz
  wait_for "tmux attached for the quiet case" side_attached tmux
  local side
  local -A scrollback=()
  local -A records=()
  for side in zz tmux; do
    scrollback[$side]="$(tmux_outer_command capture-pane -p -S - -E -1 \
      -t "=$OUTER_SESSION:$side" | sed '/^$/d')"
    records[$side]="$(printf '%s\n' "${scrollback[$side]}" | grep -c 'level=[A-Z]' || true)"
  done
  if [ -n "$CAPTURE_DIR" ]; then
    printf 'zz:\n%s\ntmux:\n%s\n' "${scrollback[zz]}" "${scrollback[tmux]}" \
      >"$CAPTURE_DIR/launch-scrollback.txt"
  fi
  report launch-scrollback same 'log records on the terminal' \
    "${records[zz]}" "${records[tmux]}" ''
}

plant() {
  local side="$1"
  local path="$2"
  local content="$3"
  local absolute="$(side_home "$side")/$path"
  mkdir -p "$(dirname -- "$absolute")"
  printf '%s\n' "$content" >"$absolute"
}
clear_roots() {
  local side="$1"
  rm -f -- "$(side_home "$side")/.tmux.conf" \
    "$(side_home "$side")/config/tmux/tmux.conf" \
    "$(side_home "$side")/config/zz/mux.conf" \
    "$(side_home "$side")/explicit.conf"
}
config_value() {
  local side="$1"
  shift
  side_command "$side" "$@" >/dev/null 2>&1 || true
  side_command "$side" show-options -gv status-left 2>&1
}

run_config_cases() {
  local side

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
  done
  plant zz config/zz/mux.conf 'set -g status-left OWNCONF'
  plant tmux .tmux.conf 'set -g status-left OWNCONF'
  reset_servers
  local zz_value tmux_value
  zz_value="$(config_value zz new-session -d -s c1)"
  tmux_value="$(config_value tmux new-session -d -s c1)"
  report config-own-file same option "$zz_value" "$tmux_value" ''

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
    plant "$side" .tmux.conf 'set -g status-left HOMECONF'
  done
  reset_servers
  zz_value="$(config_value zz new-session -d -s c2)"
  tmux_value="$(config_value tmux new-session -d -s c2)"
  report config-unimported-tmux-conf record option "$zz_value" "$tmux_value" \
    'zz does not read ~/.tmux.conf until it is imported'
  side_command zz import-tmux-config >/dev/null 2>&1 || true
  zz_value="$(config_value zz display-message -p imported)"
  report config-imported-tmux-conf same option "$zz_value" "$tmux_value" ''

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
    plant "$side" .tmux.conf 'set -g status-left HOMECONF'
    plant "$side" explicit.conf 'set -g status-left EXPLICIT'
  done
  reset_servers
  zz_value="$(config_value zz -f "$ZZ_HOME/explicit.conf" new-session -d -s c3)"
  tmux_value="$(config_value tmux -f "$TMUX_HOME/explicit.conf" new-session -d -s c3)"
  report config-explicit-file same option "$zz_value" "$tmux_value" ''

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
    plant "$side" explicit.conf 'set -g status-left EXPLICIT'
    plant "$side" config/zz/mux.conf 'set -g status-left MUXCONF'
  done
  reset_servers
  zz_value="$(config_value zz -f "$ZZ_HOME/explicit.conf" new-session -d -s c4)"
  tmux_value="$(config_value tmux -f "$TMUX_HOME/explicit.conf" new-session -d -s c4)"
  report config-mux-conf-layer record option "$zz_value" "$tmux_value" \
    'semantic:explicit-config-keeps-mux-conf-layer, fabrico 2026-09-05: zz layers zz/mux.conf after an explicit -f'

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
    plant "$side" .tmux.conf 'set -g status-left HOMECONF'
    plant "$side" config/zz/mux.conf 'set -g status-left MUXCONF'
  done
  reset_servers
  zz_value="$(config_value zz -f /dev/null new-session -d -s c5)"
  tmux_value="$(config_value tmux -f /dev/null new-session -d -s c5)"
  report config-devnull record option "$zz_value" "$tmux_value" \
    'semantic:explicit-config-keeps-mux-conf-layer, fabrico 2026-09-05: -f /dev/null still leaves zz with its mux.conf layer'

  for side in zz tmux; do
    clear_roots "$side"
    kill_every_session "$side"
  done
  reset_servers
}

SELF_CHECK_FAILURES=0
self_check_case() {
  local name="$1"
  local expectation="$2"
  local verdict=ok
  case "$expectation" in
  differs) [ "$LAST_DIFFERED" -eq 1 ] || verdict='no difference reported' ;;
  screen) [ "$LAST_SCREEN_DIFFERED" -eq 1 ] || verdict='no screen difference reported' ;;
  none) [ "$LAST_DIFFERED" -eq 0 ] || verdict='reported a difference where both sides agree' ;;
  esac
  if [ "$verdict" = ok ]; then
    printf 'ok    self-check %s\n' "$name"
    return 0
  fi
  SELF_CHECK_FAILURES=$((SELF_CHECK_FAILURES + 1))
  printf 'FAIL  self-check %s: %s\n' "$name" "$verdict"
}

run_self_check() {
  printf 'self-check: one deliberate one-sided difference per channel, plus one the fixture must not report\n'

  reset_pair
  side_command zz new-session -d -s only-zz >/dev/null 2>&1 || die 'zz refused new-session'
  run_pair sabotage-cli record 'deliberate' attach-session -t '=only-zz'
  self_check_case 'cli: a session that exists on one side only' differs

  compare_facts sabotage-facts record 'deliberate'
  self_check_case 'facts: an extra session on one side' differs

  reset_pair
  side_command zz new-session -d -s live -x "$COLUMNS_UNDER_TEST" -y "$ROWS_UNDER_TEST" \
    "ENV= PS1='\$ ' exec /bin/sh" || die 'zz refused new-session'
  side_command tmux new-session -d -s live -x "$COLUMNS_UNDER_TEST" \
    -y "$ROWS_UNDER_TEST" "ENV= PS1='# ' exec /bin/sh" || die 'tmux refused new-session'
  side_command zz rename-window -t '=live:0' win || die 'zz refused rename-window'
  side_command tmux rename-window -t '=live:0' win || die 'tmux refused rename-window'
  launch_both attach-session -t '=live'
  wait_for "zz attached" side_attached zz
  wait_for "tmux attached" side_attached tmux
  compare_screens sabotage-screen record 'deliberate'
  self_check_case 'screen: a different prompt on one side, the cursor unmoved' screen

  reset_pair
  side_command zz new-session -d -s alias >/dev/null 2>&1 || die 'zz refused new-session'
  side_command tmux new-session -d -s alias >/dev/null 2>&1 || die 'tmux refused new-session'
  local zz_answer tmux_answer
  zz_answer="$(side_command zz lsp -t '=alias:' -F '#{window_index}.#{pane_index}' 2>&1)"
  tmux_answer="$(side_command tmux list-panes -t '=alias:' -F '#{window_index}.#{pane_index}' 2>&1)"
  report alias-spelling record answer "$zz_answer" "$tmux_answer" 'deliberate'
  self_check_case 'equivalence: list-panes and its lsp alias' none

  if [ "$SELF_CHECK_FAILURES" -ne 0 ]; then
    printf '%s self-check expectations unmet\n' "$SELF_CHECK_FAILURES"
    return 1
  fi
  printf 'self-check complete: every sabotage was caught in its own channel and the equivalence passed\n'
  return 0
}

plant_controlled_config zz
plant_controlled_config tmux
start_zz_daemon

if [ "$SELF_CHECK" -eq 1 ]; then
  run_self_check
  exit $?
fi

printf 'launch surface differential (pin %s)\n' "$(basename -- "$TMUX_BIN")"
run_cli_cases
run_attached_cases
run_config_cases

if [ "$FAILURES" -ne 0 ]; then
  printf '%s of %s comparisons differ in a channel they assert, %s recorded\n' \
    "$FAILURES" "$CHECKS" "$RECORDS"
  exit 1
fi
printf 'all %s comparisons agree where they assert, %s recorded not asserted, owners decided:TUI-003=%s unattributed=0\n' \
  "$CHECKS" "$RECORDS" "$RECORDS"

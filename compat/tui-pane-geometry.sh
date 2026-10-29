#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-pane-geometry.sh [ZZ_BIN [TMUX_BIN]]\n' >&2
  printf '       ZZ_BIN=path TMUX_BIN=path compat/tui-pane-geometry.sh\n' >&2
}

COMPAT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd -- "$COMPAT_DIR/.." && pwd)"
ZZ_INPUT="${1:-${ZZ_BIN:-$REPO_DIR/target/debug/zz_cli}}"
TMUX_INPUT="${2:-${TMUX_BIN:-${ZZ_COMPAT_TMUX:-$COMPAT_DIR/.cache/tmux-src/tmux}}}"
[ "$#" -le 2 ] || { usage; exit 2; }

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

SIZES=(80x24 100x24 120x24)
SCRATCH_DIR="$(mktemp -d /tmp/zzgeo.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzgeoo-$TOKEN"
INNER_SOCKET_NAME="zzgeoi-$TOKEN"
ZZ_SOCKET="/tmp/zzgeo-$TOKEN.sock"
OUTER_SESSION="driver"
INNER_SESSION="geo"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
ZZ_CLIENT_STDERR="$SCRATCH_DIR/zz-client.err"
TMUX_CLIENT_STDERR="$SCRATCH_DIR/tmux-client.err"
DIAGNOSTICS_DIR=""
SIZE_UNDER_TEST=""
ZZ_PID=""
FAILURES=0
CHECKS=0
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

diagnostics_dir() {
  if [ -z "$DIAGNOSTICS_DIR" ]; then
    DIAGNOSTICS_DIR="${ZZ_GEO_DIAGNOSTICS_DIR:-$(mktemp -d /tmp/zzgeo-diag.XXXXXX)}"
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
    printf 'size under test: %s\n' "${SIZE_UNDER_TEST:-none yet}"
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

outer_pane_is() {
  [ "$(tmux_outer_command display-message -p -t "$1" '#{pane_width}x#{pane_height}' 2>/dev/null)" = "$2" ]
}
client_attached() {
  [ "$(side_command "$1" list-clients -F '#{client_session}' 2>/dev/null)" = "$INNER_SESSION" ]
}
file_has_two_fields() {
  local words
  words="$(cat "$1" 2>/dev/null)" || return 1
  [ "$(printf '%s\n' "$words" | wc -w)" = 2 ]
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

measure() {
  local side="$1"
  local out="$SCRATCH_DIR/$side.geometry"
  local pane
  rm -f -- "$out"
  pane="$(side_command "$side" list-panes -t "=$INNER_SESSION" -F '#{pane_id}' | head -n 1)"
  [ -n "$pane" ] || die "$side has no pane in =$INNER_SESSION"
  side_command "$side" send-keys -t "$pane" \
    "printf '%s %s\\n' \"\$(tput cols)\" \"\$(tput lines)\" >$out" Enter
  wait_for "$side geometry report" file_has_two_fields "$out"
  cat "$out"
}

zz_command -f /dev/null daemon >"$SCRATCH_DIR/zz-daemon.out" 2>"$SCRATCH_DIR/zz-daemon.err" &
ZZ_PID=$!
wait_for "zz daemon socket" test -S "$ZZ_SOCKET"

printf 'pane geometry differential (pin %s)\n' "$(basename -- "$TMUX_BIN")"
for size in "${SIZES[@]}"; do
  columns="${size%x*}"
  rows="${size#*x}"
  SIZE_UNDER_TEST="$size"
  zz_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  tmux_inner_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  tmux_outer_command kill-server >/dev/null 2>&1 || true

  zz_command new-session -d -s "$INNER_SESSION" -x "$columns" -y "$rows" ||
    die "could not create the zz session"
  tmux_inner_command -f /dev/null new-session -d -s "$INNER_SESSION" -x "$columns" -y "$rows" ||
    die "could not create the tmux session"

  write_attach zz "$SCRATCH_DIR/attach-zz.sh"
  write_attach tmux "$SCRATCH_DIR/attach-tmux.sh"
  tmux_outer_command -f /dev/null new-session -d -s "$OUTER_SESSION" -n zz \
    -x "$columns" -y "$rows" "$SCRATCH_DIR/attach-zz.sh" ||
    die "could not create the outer session"
  tmux_outer_command set-option -g status off
  tmux_outer_command new-window -d -n tmux "$SCRATCH_DIR/attach-tmux.sh"
  wait_for "outer zz pane at $size" outer_pane_is "=$OUTER_SESSION:zz" "${columns}x${rows}"
  wait_for "outer tmux pane at $size" outer_pane_is "=$OUTER_SESSION:tmux" "${columns}x${rows}"
  wait_for "zz client attached" client_attached zz
  wait_for "tmux client attached" client_attached tmux

  zz_geometry="$(measure zz)"
  tmux_geometry="$(measure tmux)"
  zz_columns="${zz_geometry%% *}"
  zz_rows="${zz_geometry##* }"
  tmux_columns="${tmux_geometry%% *}"
  tmux_rows="${tmux_geometry##* }"

  CHECKS=$((CHECKS + 1))
  if [ "$zz_columns" = "$tmux_columns" ]; then
    printf 'ok    %s columns: both %s\n' "$size" "$zz_columns"
  else
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s columns: tmux %s, zz %s\n' "$size" "$tmux_columns" "$zz_columns"
  fi
  CHECKS=$((CHECKS + 1))
  if [ "$zz_rows" = "$tmux_rows" ]; then
    printf 'ok    %s rows: both %s\n' "$size" "$zz_rows"
  else
    FAILURES=$((FAILURES + 1))
    printf 'DIFF  %s rows: tmux %s, zz %s\n' "$size" "$tmux_rows" "$zz_rows"
  fi

  zz_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  tmux_inner_command kill-session -t "=$INNER_SESSION" >/dev/null 2>&1 || true
  tmux_outer_command kill-server >/dev/null 2>&1 || true
done

if [ "$FAILURES" -ne 0 ]; then
  printf '%s of %s asserted measurements differ\n' "$FAILURES" "$CHECKS"
  exit 1
fi
printf 'all %s asserted measurements identical\n' "$CHECKS"

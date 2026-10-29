#!/usr/bin/env bash
set -eEuo pipefail

usage() {
  printf 'usage: compat/tui-popup-attach.sh [ZZ_BIN [TMUX_BIN]]\n' >&2
}

COMPAT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd -- "$COMPAT_DIR/.." && pwd)"
[ "$#" -le 2 ] || { usage; exit 2; }
ZZ_INPUT="${1:-${ZZ_BIN:-$REPO_DIR/target/debug/zz_cli}}"
TMUX_INPUT="${2:-${TMUX_BIN:-${ZZ_COMPAT_TMUX:-$COMPAT_DIR/.cache/tmux-src/tmux}}}"

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

SCRATCH_DIR="$(mktemp -d /tmp/zzpa.XXXXXX)"
TOKEN="${SCRATCH_DIR##*.}"
OUTER_SOCKET_NAME="zzpao-$TOKEN"
INNER_SOCKET_NAME="zzpai-$TOKEN"
ZZ_SOCKET="/tmp/zzpa-$TOKEN.sock"
ZZ_HOME="$SCRATCH_DIR/zz-home"
TMUX_HOME="$SCRATCH_DIR/tmux-home"
OUTER_HOME="$SCRATCH_DIR/outer-home"
ZZ_LOG_DIR="$SCRATCH_DIR/zz-logs"
INNER_SHELL="ENV= PS1='\$ ' exec /bin/sh"
FAILURES=0
CHECKS=0
mkdir -p "$ZZ_HOME" "$TMUX_HOME" "$OUTER_HOME" "$ZZ_LOG_DIR"

scrubbed() {
  env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u EDITOR -u VISUAL \
    -u XDG_STATE_HOME -u ZZ_LOG_DIR -u XDG_RUNTIME_DIR \
    TMUX_TMPDIR=/tmp LANG=C.UTF-8 LC_CTYPE=C.UTF-8 "$@"
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
inner_client() {
  case "$1" in
  zz) printf '%q --socket %q' "$ZZ_BIN" "$ZZ_SOCKET" ;;
  tmux) printf '%q -L %q' "$TMUX_BIN" "$INNER_SOCKET_NAME" ;;
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
  printf '      wait that ran out: %s\n' "$label"
  return 1
}

client_sessions() {
  side_command "$1" list-clients -F '#{client_session}' 2>/dev/null | sort | tr '\n' ' '
}
sessions_are() {
  [ "$(client_sessions "$1")" = "$2" ]
}
file_has() {
  [ -s "$1" ] && grep -q '^rc=' "$1"
}

write_attach() {
  local side="$1"
  local destination="$SCRATCH_DIR/attach-$side.sh"
  printf '#!/usr/bin/env bash\n' >"$destination"
  if [ "$side" = zz ]; then
    printf 'exec env -u TMUX -u TMUX_PANE -u ZZ_SOCKET -u ZZ_SESSION -u ZZ_PANE -u XDG_STATE_HOME -u XDG_RUNTIME_DIR HOME=%q XDG_CONFIG_HOME=%q ZZ_LOG_DIR=%q TMUX_TMPDIR=/tmp LANG=C.UTF-8 LC_CTYPE=C.UTF-8 %q --socket %q attach-session -t =main\n' \
      "$ZZ_HOME" "$ZZ_HOME/config" "$ZZ_LOG_DIR" "$ZZ_BIN" "$ZZ_SOCKET" >>"$destination"
  else
    printf 'exec env -u TMUX -u TMUX_PANE -u XDG_STATE_HOME -u XDG_RUNTIME_DIR HOME=%q XDG_CONFIG_HOME=%q TMUX_TMPDIR=/tmp LANG=C.UTF-8 LC_CTYPE=C.UTF-8 %q -L %q attach-session -t =main\n' \
      "$TMUX_HOME" "$TMUX_HOME/config" "$TMUX_BIN" "$INNER_SOCKET_NAME" >>"$destination"
  fi
  chmod +x "$destination"
}

verdict() {
  local name="$1" zz_fact="$2" tmux_fact="$3" expected="$4"
  CHECKS=$((CHECKS + 1))
  if [ "$zz_fact" = "$tmux_fact" ] && [ "$tmux_fact" = "$expected" ]; then
    printf 'ok    %s\n' "$name"
    return 0
  fi
  FAILURES=$((FAILURES + 1))
  printf 'DIFF  %s\n      expected: %s\n      tmux:     %s\n      zz:       %s\n' \
    "$name" "$expected" "$tmux_fact" "$zz_fact"
}

start_servers() {
  local side
  tmux_outer_command kill-server >/dev/null 2>&1 || true
  zz_command kill-server >/dev/null 2>&1 || true
  tmux_inner_command kill-server >/dev/null 2>&1 || true
  for side in zz tmux; do
    if [ "$side" = tmux ]; then
      tmux_inner_command -f /dev/null new-session -d -s main -x 80 -y 24 "$INNER_SHELL" ||
        die 'could not create the tmux session'
    else
      zz_command new-session -d -s main -x 80 -y 24 "$INNER_SHELL" ||
        die 'could not create the zz session'
    fi
    side_command "$side" new-session -d -s scratch -x 80 -y 24 "$INNER_SHELL" ||
      die "could not create the $side scratch session"
    side_command "$side" set-option -g default-shell /bin/sh
    side_command "$side" set-option -g status-right ''
    write_attach "$side"
  done
  tmux_outer_command -f /dev/null new-session -d -s driver -n zz -x 80 -y 24 \
    "$SCRATCH_DIR/attach-zz.sh" || die 'could not create the outer session'
  tmux_outer_command new-window -d -t driver -n tmux "$SCRATCH_DIR/attach-tmux.sh"
  for side in zz tmux; do
    wait_for "$side client on main" sessions_are "$side" 'main ' || die "$side never attached"
  done
}

popup_case() {
  local name="$1" inner="$2" side expected='main scratch '
  for side in zz tmux; do
    side_command "$side" bind-key P display-popup -E -w 70 -h 18 "$(inner_client "$side") $inner"
    tmux_outer_command send-keys -t "=driver:$side" C-b P
  done
  for side in zz tmux; do
    wait_for "$name: a $side client on scratch" sessions_are "$side" "$expected" || true
  done
  verdict "$name" "$(client_sessions zz)" "$(client_sessions tmux)" "$expected"
  for side in zz tmux; do
    side_command "$side" detach-client -s scratch >/dev/null 2>&1 || true
    wait_for "$name: the $side popup client gone" sessions_are "$side" 'main ' || true
  done
}

modal_case() {
  local side report expected
  expected='sessions should be nested with care, unset $TMUX to force|rc=1'
  for side in zz tmux; do
    report="$SCRATCH_DIR/modal-$side.txt"
    side_command "$side" new-pane -O -t =main: \
      "$(inner_client "$side") attach-session -t =scratch 2>$report; echo rc=\$? >>$report"
  done
  for side in zz tmux; do
    wait_for "modal-refused: the $side report" file_has "$SCRATCH_DIR/modal-$side.txt" || true
  done
  verdict modal-refused \
    "$(tr '\n' '|' <"$SCRATCH_DIR/modal-zz.txt" 2>/dev/null | sed 's/|$//')" \
    "$(tr '\n' '|' <"$SCRATCH_DIR/modal-tmux.txt" 2>/dev/null | sed 's/|$//')" \
    "$expected"
}

start_servers
popup_case popup-attach 'attach-session -t =scratch'
popup_case popup-new-session 'new-session -A -s scratch'
modal_case

if [ "$FAILURES" -eq 0 ]; then
  printf 'all %s cases agree\n' "$CHECKS"
  exit 0
fi
printf '%s of %s cases differ\n' "$FAILURES" "$CHECKS"
exit 1

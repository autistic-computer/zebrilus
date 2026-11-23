#!/usr/bin/env bash
set -uo pipefail
PROVIDER=${1:-} SCENARIO=${2:-}
case $PROVIDER in
  agy) BIN=agy MODEL=${MODEL-gemini-3.8-flash} VENDOR=(.gemini/antigravity-cli/conversations .gemini/antigravity-cli/brain .gemini/antigravity-cli/implicit) ;;
  grok) BIN=grok MODEL=${MODEL-grok-4.7-build-fast} VENDOR=(.grok/sessions) ;;
  cursor) BIN=cursor-agent MODEL=${MODEL-gpt-5.4-nano} VENDOR=(.cursor/acp-sessions) ;;
  claude) BIN=claude MODEL=${MODEL-haiku} VENDOR=(.claude/projects) ;;
  codex) BIN=codex MODEL=${MODEL-gpt-6-luna} VENDOR=(.codex/sessions .codex/archived_sessions) ;;
  *) echo "usage: record.sh agy|grok|cursor|claude|codex open|hello|tools|stop|queue|reload" >&2; exit 2 ;;
esac
case $SCENARIO in open | hello | tools | stop | queue | reload) ;; *) echo "unknown scenario $SCENARIO" >&2; exit 2 ;; esac
ON_BLOCK=deny
[ "$SCENARIO" = tools ] && ON_BLOCK=allow
S=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$S/../../.." && pwd)
ZB=${ZZ_BINARY:-$ROOT/target/debug/zz_cli}
CANCEL=${ZZ_AGENT_CANCEL:-$ROOT/target/debug/examples/agent_cancel}
TMO=${TMO:-180}
[ -x "$ZB" ] && [ -x "$CANCEL" ] || {
  echo "build first: cargo build -p zz-cli && cargo build -p zz-daemon-client --example agent_cancel" >&2
  exit 2
}
mkdir -p /private/tmp/zz-tapes
RUN=$(mktemp -d "/private/tmp/zz-tapes/$PROVIDER-$SCENARIO-XXXXXX")
H=$RUN/home WS=$RUN/work SOCK=/tmp/zzt-${RUN##*-}.sock
mkdir -p "$H/Library" "$WS" "$RUN/tapes" "$RUN/vendor" "$RUN/tmp"
ln -s "$HOME/Library/Keychains" "$H/Library/Keychains"
case $PROVIDER in
  grok) mkdir -p "$H/.grok" && ln -s "$HOME/.grok/auth.json" "$H/.grok/auth.json" ;;
  cursor) mkdir -p "$H/.cursor" && jq '.approvalMode = "allowlist"' "$HOME/.cursor/cli-config.json" > "$H/.cursor/cli-config.json" ;;
  agy) mkdir -p "$H/.gemini/antigravity-cli" ;;
  claude) mkdir -p "$H/.claude" && printf '{\n  "model": "claude-haiku-5-5"\n}\n' > "$H/.claude/settings.json" ;;
  codex) mkdir -p "$H/.codex" && ln -s "$HOME/.codex/auth.json" "$H/.codex/auth.json" &&
    printf 'model = "%s"\n\n[features]\napps = false\nplugins = false\nremote_plugin = false\n' "$MODEL" > "$H/.codex/config.toml" ;;
esac
cd "$WS" || exit 2
printf 'hello from the notes\nsecond line\n' > "$WS/notes.txt"
printf 'one\ntwo\nthree\n' > "$WS/list.txt"
printf '%s %s %s\n' "$PROVIDER" "$SCENARIO" "$MODEL" > "$RUN/meta"
"$BIN" --version > "$RUN/version" 2>&1

CLEAN_PATH=$(tr ':' '\n' <<<"$PATH" | rg -v 'zz-tmux-shim|/zz-tapes/' | paste -sd: -)
case $PROVIDER in claude | codex) CLEAN_PATH="$(dirname "$(command -v "$BIN")"):/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin" ;; esac
Z() {
  env -i HOME="$H" USER="$USER" LOGNAME="$USER" PATH="$CLEAN_PATH" SHELL=/bin/zsh TERM=xterm-256color TZ=UTC \
    LANG=en_US.UTF-8 TMPDIR="$RUN/tmp" ZZ_SOCKET="$SOCK" ZZ_TRAY=0 ZZ_UPDATE_CHECK=0 NO_OPEN_BROWSER=1 \
    CURSOR_AGENT_CLI_ASSUME_SSH=0 DISABLE_AUTOUPDATER=1 ENABLE_CLAUDEAI_MCP_SERVERS=false ZZ_AGENT_TAPE_DIR="$RUN/tapes" "$@"
}
log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$RUN/rig.log"; }
stop_daemon() {
  Z "$ZB" capture-pane -p -S - -t "${ME:-%0}" > "$RUN/chat.txt" 2>&1
  Z "$ZB" kill-server > /dev/null 2>&1
  rm -f "$SOCK"
  [ "$PROVIDER" = codex ] && Z timeout 30 codex app-server daemon stop >> "$RUN/rig.log" 2>&1
  for _ in $(seq 20); do pgrep -f "$RUN/" > /dev/null || break; sleep 0.5; done
  pgrep -fl "$RUN/" >> "$RUN/left.txt" && pkill -f "$RUN/" && log "stopped processes left running, see left.txt"
  rm -rf "$H/.codex/packages" "$H/.codex/tmp"
  for dir in "${VENDOR[@]}"; do [ -e "$H/$dir" ] && cp -R "$H/$dir" "$RUN/vendor/"; done
  log "run in $RUN"
}
fail() {
  log "FAILED: $*"
  stop_daemon
  exit 1
}
idle() {
  local state
  for _ in $(seq $((${1:-90} * 2))); do
    state=$(Z "$ZB" display-message -p -t "$ME" '#{agent_state}' 2> /dev/null)
    case $state in idle) return 0 ;; failed) return 1 ;; esac
    sleep 0.5
  done
  return 1
}
working() {
  for _ in $(seq 60); do
    [ "$(Z "$ZB" display-message -p -t "$ME" '#{agent_state}' 2> /dev/null)" = working ] && return 0
    sleep 0.5
  done
  return 1
}
ask() {
  log "prompt: $*"
  Z "$ZB" agent-send -t "$ME" --wait --final --on-block "$ON_BLOCK" --timeout "$TMO" "$@" >> "$RUN/replies.txt" 2>> "$RUN/rig.log"
}
open_chat() {
  local model=()
  [ -n "$MODEL" ] && model=(--model "$MODEL")
  ME=$(Z "$ZB" new-window -d -P -F '#{pane_id}' -t tape -c "$WS" --kind agent "${model[@]}" "$@" "$PROVIDER") || fail "could not open the chat"
  idle 120 || fail "the chat did not become idle: $(Z "$ZB" display-message -p -t "$ME" '#{agent_state}')"
}

Z "$ZB" new-session -d -s tape -x 200 -y 50 -c "$WS" || fail "daemon start failed"
Z "$ZB" set-option -g agent-auto-approve off
open_chat
log "chat $ME open"
case $SCENARIO in
  open) ;;
  hello) ask 'Reply with exactly: hello from the tape' || fail "hello" ;;
  tools) ask 'In this folder: read notes.txt, write its first line to copy.txt, run ls, then run cat missing.txt (it fails). Reply in one short sentence.' || fail "tools" ;;
  stop)
    log "prompt: count"
    Z "$ZB" agent-send -t "$ME" "Count from 1 to ${COUNT:-300}, one number per line." >> "$RUN/rig.log" 2>&1
    working || fail "the turn did not start"
    sleep 4
    log "stop"
    Z "$CANCEL" "$ME" || fail "stop"
    idle 60 || fail "the turn did not stop"
    ask 'Reply with the word done.' || fail "after stop"
    ;;
  queue)
    ask 'Write the numbers one to forty in words, one per line, then the word first.' &
    FIRST=$!
    working || fail "the first turn did not start"
    ask 'Reply with the word second.' || fail "queue"
    wait "$FIRST" || fail "the first turn"
    idle 60 || fail "the queue did not drain"
    ;;
  reload)
    ask 'Remember the word kumquat and reply with ok.' || fail "before reload"
    SESSION=$(jq -r 'select(.dir == "out") | .msg | objects | (.result | objects | .sessionId // .thread.id) // .session_id // empty' "$RUN"/tapes/*.ndjson | head -1)
    [ -n "$SESSION" ] || fail "no session id"
    log "session $SESSION, closing $ME"
    Z "$ZB" kill-pane -t "$ME"
    sleep 2
    open_chat --resume "$SESSION"
    log "reopened as $ME"
    sleep 3
    ;;
esac
stop_daemon

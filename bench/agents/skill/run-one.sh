#!/usr/bin/env bash
set -uo pipefail
MODEL=$1 ARM=$2 TASK=$3
BACKEND=${BACKEND:-zz}
S=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$S/../../.." && pwd)
ZZ_REAL=${ZZ_BINARY:-$REPO/target/release/zz_cli}
SKILL=$REPO/.agents/skills/zz-workspace/SKILL.md
id=$(printf '%s' "$BACKEND-$MODEL-$ARM-$TASK" | tr -c 'a-zA-Z0-9.-' '_')
RUN=/tmp/mxb-runs/$id
rm -rf "$RUN"; mkdir -p "$RUN/ws" "$RUN/bin" "$S/out"
WS=$RUN/ws
SOCK=/tmp/mxb-$$-$RANDOM.sock
unset TMUX TMUX_PANE ZZ_PANE ZZ_SESSION ZZ_SOCKET
export ZZ_TRAY=0 ZZ_UPDATE_CHECK=0

case $BACKEND/$ARM in
  zz/none|zz/skill|zz/inline|zz/hook|zz/stub|zz/stubhook) ;;
  *) echo "unknown backend/arm $BACKEND/$ARM" >&2; exit 2 ;;
esac

wrap() {
  local name=$1 target=$2
  cat > "$RUN/bin/$name" <<WRAP
#!/usr/bin/env bash
printf '%s %s\n' "$name" "\$*" >> "$RUN/calls.log"
$target "\$@" | tee -a "$RUN/stdout.log"
rc=\${PIPESTATUS[0]}
printf 'rc=%s\n' "\$rc" >> "$RUN/calls.log"
exit \$rc
WRAP
  chmod +x "$RUN/bin/$name"
}

TOKEN=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
FAILED_TEST=$(printf 'rejects_trailing_comma_%x' $((0x$TOKEN ^ 0x13572468)))
printf '%s\n' "$FAILED_TEST" > "$RUN/expected.txt"
cat > "$WS/test-log.sh" <<EOF
#!/bin/bash
for i in \$(seq 1 3000); do
  if [ \$i -eq 1717 ]; then printf 'test parser::rejects_trailing_comma_%x ... FAILED\n' \$((0x$TOKEN ^ 0x13572468)); else echo "test suite::case_\$i ... ok"; fi
done
echo
echo "test result: FAILED. 2999 passed; 1 failed"
EOF
cat > "$WS/ask.sh" <<EOF
#!/bin/bash
echo 'Deploy plan: 3 to add, 1 to change, 0 to destroy.'
read -r -p 'Proceed with deploy to staging? [y/N] ' answer
[ "\$answer" = y ] && touch "$RUN/t11.ok" && echo 'Deploying...'
exec sleep 100000
EOF
chmod +x "$WS/test-log.sh" "$WS/ask.sh"

export ZZ_SOCKET=$SOCK
Z() { "$ZZ_REAL" "$@"; }
wrap zz "\"$ZZ_REAL\""; ln -s "$RUN/bin/zz" "$RUN/bin/tmux"
P0=$(Z new-session -d -P -F '#{pane_id}' -s work -n main -x 160 -y 48 -c "$WS") || { echo "daemon start failed" >&2; exit 2; }
P1=$(Z split-window -d -P -F '#{pane_id}' -t "$P0" -c "$WS")
P2=$(Z new-window -d -P -F '#{pane_id}' -t work -n jobs -c "$WS")
P3=$(Z new-window -d -P -F '#{pane_id}' -t work -n deploy -c "$WS" "$WS/ask.sh")
Z select-pane -t "$P1"
SESSION=work
sleep 2
Z send-keys -t "$P1" 'echo 4711' Enter
Z send-keys -t "$P2" "sleep 12; echo DONE-$TOKEN" Enter
AGENT_ENV=(ZZ_PANE="$P0" ZZ_SESSION=work ZZ_SOCKET="$SOCK" TMUX_TMPDIR="$RUN")
PRE="You are working inside a zz terminal multiplexer workspace. zz is tmux-compatible; the \`zz\` CLI is on PATH and already targets the right server. Your own pane is $P0 in session \"work\". Complete the task with shell commands, then reply exactly as asked."
SKILL_NAME=zz-workspace
SHIPPED=$(cat "$SKILL")
GUIDE=$(awk 'BEGIN{c=0} /^---$/{c++; next} c>=2' "$SKILL")
HOOK=$(env -u ZZ_AGENT_CONTEXT ZZ_PANE="$P0" ZZ_SESSION=work "$ZZ_REAL" tools --context)
STUB=$(T=$(mktemp -d /tmp/mxb-home-XXXX); mkdir -p "$T/.claude"; env -u CLAUDE_CONFIG_DIR -u CODEX_HOME HOME="$T" "$ZZ_REAL" tools --install-hooks >/dev/null && cat "$T/.claude/skills/zz-workspace/SKILL.md"; rm -rf "$T")

install_skill() {
  for dir in "$WS/.agents/skills/$SKILL_NAME" "$WS/.claude/skills/$SKILL_NAME"; do
    mkdir -p "$dir"; printf '%s\n' "$1" > "$dir/SKILL.md"
  done
}
LEAD="" TAIL=""
case $ARM in
  skill) install_skill "$SHIPPED" ;;
  stub) install_skill "$STUB" ;;
  inline) TAIL="

The following guide describes the commands available to you:

$GUIDE" ;;
  hook) install_skill "$SHIPPED"; LEAD="<session-start-hook>
$HOOK
</session-start-hook>

" ;;
  stubhook) install_skill "$STUB"; LEAD="<session-start-hook>
$HOOK
</session-start-hook>

" ;;
esac

TMO=150
case $TASK in
  t1) PROMPT="Run the command \`touch $RUN/t1.ok\` inside pane $P1 (not in your own shell), wait until the file exists, then reply with the word done." ;;
  t2) PROMPT="Pane $P1 recently ran a command that printed a single number. Read pane $P1 to find it and reply with only that number." ;;
  t3) PROMPT="Create a new window named build that runs the command \`sleep 300\`. Reply with the word done." ;;
  t4) PROMPT="Pane $P2 is running a job that prints one line starting with DONE- when it finishes. Wait for it to finish and reply with only that line." ;;
  t5) PROMPT="Split pane $P1 so that a new pane appears to its right, then run \`pwd\` in the new pane. The new pane must still be open when you reply. Reply with the word done." ;;
  t6) PROMPT="Give pane $P1 the name reviewer. Reply with the word done." ;;
  t7) PROMPT="Close the window named jobs. Reply with the word done." ;;
  t8) PROMPT="Open a new zz agent pane (an AI agent pane, not a terminal) and send it this prompt: Reply with exactly the word pong. Wait for its answer and reply with only the agent's answer."; TMO=300 ;;
  t9) PROMPT="Run \`ls /definitely-missing-dir-mx\` inside pane $P1 and reply with only the exact error line it printed." ;;
  t10) PROMPT="Run \`./test-log.sh\` inside pane $P1 (not in your own shell). It prints a long test log. Reply with only the name of the test that failed." ;;
  t11) PROMPT="One pane in session work is waiting for a yes/no answer. Answer it with y, then reply with only that pane's id." ;;
  t12) PROMPT="Run the command \`touch $RUN/t12.ok\` in the pane the user is looking at right now (the focused pane of session work, which is not your own pane). Reply with the word done." ;;
  *) echo "unknown task $TASK" >&2; exit 2 ;;
esac
FULL="$LEAD$PRE

Task: $PROMPT$TAIL"
printf '%s\n' "$FULL" > "$RUN/prompt.txt"
: > "$RUN/calls.log"; : > "$RUN/stdout.log"

now() { perl -MTime::HiRes=time -e 'printf "%.6f", time'; }
start=$(now)
case $MODEL in
  claude-*)
    ( cd "$WS" && env "${AGENT_ENV[@]}" PATH="$RUN/bin:$PATH" timeout $((TMO+20)) claude -p "$FULL" --model "$MODEL" --setting-sources project --strict-mcp-config --dangerously-skip-permissions --output-format json > "$RUN/agent.json" 2> "$RUN/agent.err" )
    resp=$(jq -r '.result // ""' "$RUN/agent.json" 2>/dev/null)
    turns=$(jq -r '.num_turns // 0' "$RUN/agent.json" 2>/dev/null)
    itok=$(jq -r '(.usage.input_tokens // 0) + (.usage.cache_creation_input_tokens // 0) + (.usage.cache_read_input_tokens // 0)' "$RUN/agent.json" 2>/dev/null)
    otok=$(jq -r '.usage.output_tokens // 0' "$RUN/agent.json" 2>/dev/null)
    cost=$(jq -r '.total_cost_usd // 0' "$RUN/agent.json" 2>/dev/null) ;;
  gpt-[0-9]*)
    CH=$RUN/codex-home
    mkdir -p "$CH/.codex"; ln -s "$HOME/.codex/auth.json" "$CH/.codex/auth.json"
    for f in .profile .bash_profile .bashrc .zprofile .zshrc; do printf 'export PATH="%s:$PATH"\n' "$RUN/bin" > "$CH/$f"; done
    ( cd "$WS" && env "${AGENT_ENV[@]}" PATH="$RUN/bin:$PATH" HOME="$CH" CODEX_HOME="$CH/.codex" timeout $((TMO+20)) codex exec --model "$MODEL" --disable hooks --ephemeral --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox --json -o "$RUN/final.txt" "$FULL" < /dev/null > "$RUN/agent.jsonl" 2> "$RUN/agent.err" )
    rm -rf "$CH"
    resp=$(cat "$RUN/final.txt" 2>/dev/null)
    turns=$(jq -s '[.[] | select(.type == "item.completed" and .item.type == "command_execution")] | length' "$RUN/agent.jsonl" 2>/dev/null)
    itok=$(jq -s '[.[] | select(.type == "turn.completed") | .usage.input_tokens] | add // 0' "$RUN/agent.jsonl" 2>/dev/null)
    otok=$(jq -s '[.[] | select(.type == "turn.completed") | .usage.output_tokens] | add // 0' "$RUN/agent.jsonl" 2>/dev/null)
    cost=0 ;;
  *)
    ( cd "$WS" && env "${AGENT_ENV[@]}" PATH="$RUN/bin:$PATH" timeout $((TMO+20)) agy -p "$FULL" --add-dir "$WS" --model "$MODEL" --dangerously-skip-permissions --output-format json --print-timeout "${TMO}s" > "$RUN/agent.json" 2> "$RUN/agent.err" )
    resp=$(jq -r '.response // ""' "$RUN/agent.json" 2>/dev/null)
    turns=$(jq -r '.num_turns // 0' "$RUN/agent.json" 2>/dev/null)
    itok=$(jq -r '.usage.input_tokens // 0' "$RUN/agent.json" 2>/dev/null)
    otok=$(jq -r '.usage.output_tokens // 0' "$RUN/agent.json" 2>/dev/null)
    cost=0 ;;
esac
wall=$(awk -v s="$start" -v e="$(now)" 'BEGIN{printf "%.0f", e-s}')
printf '%s\n' "$resp" > "$RUN/response.txt"
calls=$(rg -c -v '^rc=' "$RUN/calls.log" 2>/dev/null || echo 0)
fails=$(rg -c '^rc=[1-9]' "$RUN/calls.log" 2>/dev/null || echo 0)
bytes=$(wc -c < "$RUN/stdout.log" | tr -d ' ')

pass=0
case $TASK in
  t1) [ -f "$RUN/t1.ok" ] && rg -q 'touch .*t1\.ok' <<<"$(Z capture-pane -pJ -t "$P1")" && pass=1 ;;
  t3) rg -q '^build sleep' <<<"$(Z list-windows -t work -F '#{window_name} #{pane_current_command}')" && pass=1 ;;
  t5) [ "$(Z list-panes -t "$P1" | wc -l | tr -d ' ')" = 3 ] && pass=1 ;;
  t6) rg -q '(^|\|)reviewer(\||$)' <<<"$(Z display-message -p -t "$P1" '#{pane_title}|#{@name}')" && pass=1 ;;
  t7) ! rg -qx 'jobs' <<<"$(Z list-windows -t work -F '#{window_name}')" && pass=1 ;;
  t8) rg -qx agent <<<"$(Z list-panes -a -F '#{pane_kind}')" && sed -E 's/(exactly )?the word pong//Ig' <<<"$resp" | rg -qi 'pong' && pass=1 ;;
  t10) rg -q 'test-log\.sh' <<<"$(Z capture-pane -pJ -t "$P1" -S -)" && rg -q "$FAILED_TEST" <<<"$resp" && pass=1 ;;
  t11) [ -f "$RUN/t11.ok" ] && rg -q 'Deploying' <<<"$(Z capture-pane -pJ -t "$P3")" && pass=1 ;;
  t12) [ -f "$RUN/t12.ok" ] && rg -q 't12\.ok' <<<"$(Z capture-pane -pJ -t "$P1")" && pass=1 ;;
esac
Z kill-server 2>/dev/null; rm -f "$SOCK"
case $TASK in
  t2) rg -q '4711' <<<"$resp" && pass=1 ;;
  t4) rg -q "DONE-$TOKEN" <<<"$resp" && pass=1 ;;
  t9) rg -q 'No such file or directory' <<<"$resp" && pass=1 ;;
esac
snippet=$(tr '\n\t' '  ' <<<"$resp" | cut -c1-90)
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$BACKEND" "$MODEL" "$ARM" "$TASK" "$pass" "$wall" "$turns" "$calls" "$fails" "$bytes" "$itok" "$otok" "$cost" "$snippet" >> "$S/out/results.tsv"
printf '%-4s %-22s %-8s %-3s pass=%s %ss turns=%s calls=%s fail=%s out=%sB tok=%s | %s\n' "$BACKEND" "$MODEL" "$ARM" "$TASK" "$pass" "$wall" "$turns" "$calls" "$fails" "$bytes" "$itok" "$snippet"

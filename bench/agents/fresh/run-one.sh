#!/usr/bin/env bash
# run-one.sh MODEL ARM TASK [REP]
#   MODEL claude-* | gpt-*;  ARM bare | hooks | trusted (codex only) | pane;  TASK n0..n8
#   ZZ_BINARY (default /opt/homebrew/bin/zz), AUTO = agent-auto-approve (default reads), REMOTE_HOST for n8
set -uo pipefail
MODEL=$1 ARM=$2 TASK=$3 REP=${4:-1}
S=$(cd "$(dirname "$0")" && pwd)
ZB=${ZZ_BINARY:-/opt/homebrew/bin/zz}
AUTO=${AUTO:-reads}
REMOTE=${REMOTE_HOST:-}
[ "$TASK" = n8 ] && [ -z "$REMOTE" ] && { echo "n8 needs REMOTE_HOST" >&2; exit 2; }
mkdir -p /tmp/fu-runs "$S/out"
RUN=$(mktemp -d /tmp/fu-runs/XXXXXX)
printf '%s %s %s %s\n' "$MODEL" "$ARM" "$TASK" "$REP" > "$RUN/meta"
H=$RUN/h WS=$RUN/ws SOCK=/tmp/fus-${RUN##*/}.sock
mkdir -p "$H/Library" "$H/.claude" "$H/.codex" "$H/.config/zz" "$WS" "$RUN/bin" "$RUN/fx"
ln -s "$HOME/Library/Keychains" "$H/Library/Keychains"
ln -s "$HOME/.codex/auth.json" "$H/.codex/auth.json"
case $MODEL in claude-*) PROVIDER=claude-code ;; *) PROVIDER=codex ;; esac
printf '{\n  "model": "%s"\n}\n' "${CLAUDE_MODEL:-claude-haiku-5-5}" > "$H/.claude/settings.json"
printf 'model = "%s"\n' "${CODEX_MODEL:-gpt-6-luna}" > "$H/.codex/config.toml"
rg '^host-' "$HOME/.config/zz/config" > "$H/.config/zz/config" 2>/dev/null
CLEAN_PATH=$(tr ':' '\n' <<<"$PATH" | rg -v 'zz-tmux-shim|/tmp/fu-runs' | paste -sd: -)
for f in .profile .bash_profile .bashrc .zprofile .zshrc; do printf 'export PATH="%s:$PATH"\n' "$RUN/bin" > "$H/$f"; done

Z() { env -i HOME="$H" USER="$USER" LOGNAME="$USER" PATH="$CLEAN_PATH" SHELL=/bin/zsh TERM=xterm-256color LANG=en_US.UTF-8 TMPDIR="$TMPDIR" SSH_AUTH_SOCK="${SSH_AUTH_SOCK-}" ZZ_SOCKET="$SOCK" ZZ_TRAY=0 ZZ_UPDATE_CHECK=0 "$ZB" "$@"; }

for name in zz tmux; do
  if [ $name = zz ]; then target='"${ZZ_TMUX_EXECUTABLE:-'"$ZB"'}"'; else target='"${ZZ_TMUX_SHIM_DIR:+$ZZ_TMUX_SHIM_DIR/tmux}"'; fi
  cat > "$RUN/bin/$name" <<WRAP
#!/bin/bash
printf '%s %s\n' $name "\$*" >> "$RUN/calls.log"
t=$target
[ -n "\$t" ] && [ -x "\$t" ] || t="$ZB"
"\$t" "\$@" | tee -a "$RUN/stdout.log"
rc=\${PIPESTATUS[0]}
printf 'rc=%s\n' "\$rc" >> "$RUN/calls.log"
exit \$rc
WRAP
  chmod +x "$RUN/bin/$name"
done
: > "$RUN/calls.log"; : > "$RUN/stdout.log"

TOKEN=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
BORN=$(date +%s)
FAILED_TEST=$(printf 'rejects_trailing_comma_%x' $((0x$TOKEN ^ 0x13572468)))
cat > "$WS/test-log.sh" <<EOF
#!/bin/bash
for i in \$(seq 1 3000); do
  if [ \$i -eq 1717 ]; then printf 'test parser::rejects_trailing_comma_%x ... FAILED\n' \$((0x$TOKEN ^ 0x13572468)); else echo "test suite::case_\$i ... ok"; fi
done
echo
echo "test result: FAILED. 2999 passed; 1 failed"
EOF
cat > "$RUN/fx/deploy" <<EOF
#!/bin/bash
echo 'Deploy plan: 3 to add, 1 to change, 0 to destroy.'
read -r -p 'Proceed with deploy to staging? [y/N] ' answer
[ "\$answer" = y ] && touch "$RUN/fx/answered" && echo 'Deploying...'
exec sleep 100000
EOF
chmod +x "$WS/test-log.sh" "$RUN/fx/deploy"

TMO=240
case $TASK in
  n0) PROMPT="Before doing anything else: were you given any information about the terminal workspace you are running in, such as your own pane id? If yes, reply with only that pane id exactly as given (for example %7). If not, reply with exactly NONE. Do not run any commands." ;;
  n1) PROMPT="The other pane in this window printed a number a moment ago. What was it? Reply with only the number." ;;
  n2) PROMPT="Run ./test-log.sh in the other pane of this window, not in your own shell. It prints a long test log. Reply with only the name of the test that failed." ;;
  n3) PROMPT="A window called jobs is running something that will print a line starting with DONE-. Wait for it to finish and reply with only that line." ;;
  n4) PROMPT="Something in this session is waiting for me to answer a yes/no question. Answer it with y for me, then reply with only the id of the pane it was in." ;;
  n5) PROMPT="Run \`touch $RUN/fx/n5.ok\` in the pane I'm looking at right now (it is not your own pane). Reply with the word done." ;;
  n6) PROMPT="Open a new window named build that runs \`sleep 300\`, but don't switch me to it. Reply with the word done." ;;
  n7) PROMPT="Open a new Claude agent pane in zz next to you and ask it to reply with exactly the word pong. Tell me what it answered."; TMO=360 ;;
  n8) PROMPT="Using zz, open a new window named bench-$TOKEN on my machine $REMOTE that runs \`sleep 900\`. Don't touch anything else on $REMOTE. Reply with the word done." ;;
  *) echo "unknown task $TASK" >&2; exit 2 ;;
esac
printf '%s\n' "$PROMPT" > "$RUN/prompt.txt"

if [ "$ARM" = pane ]; then
  PX=$(Z new-session -d -P -F '#{pane_id}' -s work -n main -x 200 -y 50 -c "$WS") || { echo "daemon start failed" >&2; exit 2; }
  Z set-option -g agent-auto-approve "$AUTO"
  ME=$(Z split-window --kind agent --provider "$PROVIDER" -d -P -F '#{pane_id}' -t "$PX" -c "$WS")
  P1=$(Z split-window -h -d -P -F '#{pane_id}' -t "$ME" -c "$WS")
  Z kill-pane -t "$PX"
else
  cat > "$RUN/agent.sh" <<EOF
#!/bin/bash
while [ ! -f "$RUN/go" ]; do sleep 0.2; done
cd "$WS"
export PATH="$RUN/bin:\$PATH"
env > "$RUN/agent.env"
EOF
  [ "$ARM" = hooks ] || [ "$ARM" = trusted ] && echo "\"$ZB\" tools --install-hooks > \"$RUN/install.out\" 2>&1" >> "$RUN/agent.sh"
  case $MODEL in
    claude-*) echo "claude -p \"\$(cat \"$RUN/prompt.txt\")\" --model $MODEL --dangerously-skip-permissions --output-format json > \"$RUN/agent.json\" 2> \"$RUN/agent.err\"" >> "$RUN/agent.sh" ;;
    *) extra=""; [ "$ARM" = trusted ] && extra="--dangerously-bypass-hook-trust"
       echo "codex exec $extra --model $MODEL --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check --ephemeral --json -o \"$RUN/final.txt\" \"\$(cat \"$RUN/prompt.txt\")\" < /dev/null > \"$RUN/agent.jsonl\" 2> \"$RUN/agent.err\"" >> "$RUN/agent.sh" ;;
  esac
  printf 'touch "%s/done"\nexec sleep 100000\n' "$RUN" >> "$RUN/agent.sh"
  chmod +x "$RUN/agent.sh"
  ME=$(Z new-session -d -P -F '#{pane_id}' -s work -n main -x 200 -y 50 -c "$WS" "$RUN/agent.sh") || { echo "daemon start failed" >&2; exit 2; }
  P1=$(Z split-window -h -d -P -F '#{pane_id}' -t "$ME" -c "$WS")
  Z set-option -g agent-auto-approve "$AUTO"
fi
P2=$(Z new-window -d -P -F '#{pane_id}' -t work -n jobs -c "$WS")
P3=$(Z new-window -d -P -F '#{pane_id}' -t work -n deploy -c "$WS" "$RUN/fx/deploy")
Z select-pane -t "$P1"
MAINW=$(Z display-message -p -t "$P1" '#{window_id}')
sleep 2
Z send-keys -t "$P1" 'echo 4711' Enter
Z send-keys -t "$P2" "sleep 12; echo DONE-$TOKEN" Enter
sleep 0.5
printf 'ME=%s P1=%s P2=%s P3=%s\n' "$ME" "$P1" "$P2" "$P3" >> "$RUN/meta"

now() { perl -MTime::HiRes=time -e 'printf "%.3f", time'; }
start=$(now)
turns=0 itok=0 otok=0 cost=0 resp="" blocked=0
if [ "$ARM" = pane ]; then
  Z agent-send -t "$ME" --wait --json --on-block fail --timeout $TMO "$PROMPT" > "$RUN/agent.json" 2> "$RUN/agent.err"
  [ $? = 3 ] && blocked=1
  resp=$(jq -r '.final_text // ""' "$RUN/agent.json" 2>/dev/null)
  turns=$(jq -r '.tool_calls // 0' "$RUN/agent.json" 2>/dev/null)
else
  touch "$RUN/go"
  for _ in $(seq $((TMO * 5))); do [ -f "$RUN/done" ] && break; sleep 0.2; done
  case $MODEL in
    claude-*)
      resp=$(jq -r '.result // ""' "$RUN/agent.json" 2>/dev/null)
      turns=$(jq -r '.num_turns // 0' "$RUN/agent.json" 2>/dev/null)
      itok=$(jq -r '(.usage.input_tokens // 0) + (.usage.cache_creation_input_tokens // 0) + (.usage.cache_read_input_tokens // 0)' "$RUN/agent.json" 2>/dev/null)
      otok=$(jq -r '.usage.output_tokens // 0' "$RUN/agent.json" 2>/dev/null)
      cost=$(jq -r '.total_cost_usd // 0' "$RUN/agent.json" 2>/dev/null) ;;
    *)
      resp=$(cat "$RUN/final.txt" 2>/dev/null)
      turns=$(jq -s '[.[] | select(.type == "item.completed" and .item.type == "command_execution")] | length' "$RUN/agent.jsonl" 2>/dev/null)
      itok=$(jq -s '[.[] | select(.type == "turn.completed") | .usage.input_tokens] | add // 0' "$RUN/agent.jsonl" 2>/dev/null)
      otok=$(jq -s '[.[] | select(.type == "turn.completed") | .usage.output_tokens] | add // 0' "$RUN/agent.jsonl" 2>/dev/null) ;;
  esac
fi
wall=$(awk -v s="$start" -v e="$(now)" 'BEGIN{printf "%.0f", e-s}')
printf '%s\n' "$resp" > "$RUN/response.txt"
calls=$(rg -c -v '^rc=' "$RUN/calls.log" 2>/dev/null || echo 0)
fails=$(rg -c '^rc=[1-9]' "$RUN/calls.log" 2>/dev/null || echo 0)

pass=0
case $TASK in
  n0) rg -q -- "$ME(\$|[^0-9])" <<<"$resp" && pass=1 ;;
  n1) rg -q '4711' <<<"$resp" && pass=1 ;;
  n2) rg -q 'test-log\.sh' <<<"$(Z capture-pane -pJ -t "$P1" -S -)" && rg -q "$FAILED_TEST" <<<"$resp" && pass=1 ;;
  n3) rg -q "DONE-$TOKEN" <<<"$resp" && pass=1 ;;
  n4) [ -f "$RUN/fx/answered" ] && rg -q 'Deploying' <<<"$(Z capture-pane -pJ -t "$P3")" && rg -q -- "$P3(\$|[^0-9])" <<<"$resp" && pass=1 ;;
  n5) [ -f "$RUN/fx/n5.ok" ] && rg -q 'n5\.ok' <<<"$(Z capture-pane -pJ -t "$P1")" && pass=1 ;;
  n6) rg -q '^build sleep' <<<"$(Z list-windows -t work -F '#{window_name} #{pane_current_command}')" \
        && [ "$(Z display-message -p -t work '#{window_id}')" = "$MAINW" ] && pass=1 ;;
  n7) [ "$(Z list-panes -a -F '#{pane_kind}' | rg -c '^agent$')" -ge "$([ "$ARM" = pane ] && echo 2 || echo 1)" ] \
        && sed -E 's/(exactly )?the word pong//Ig' <<<"$resp" | rg -qi 'pong' && pass=1 ;;
  n8) remote=$(Z --host "$REMOTE" list-windows -a -F '#{window_id} #{window_name} #{pane_current_command} #{session_id} #{session_created}' 2>/dev/null)
      rg -q "bench-$TOKEN sleep" <<<"$remote" && pass=1
      rg "bench-$TOKEN" <<<"$remote" | while read -r w _ _ session created; do
        if [ "${created:-0}" -ge "$BORN" ]; then Z --host "$REMOTE" kill-session -t "$session"; else Z --host "$REMOTE" kill-window -t "$w"; fi >/dev/null 2>&1
      done ;;
esac
Z list-panes -a -F '#{pane_id} #{pane_kind} #{window_name} #{pane_current_command}' > "$RUN/panes.txt" 2>&1
Z kill-server >/dev/null 2>&1; rm -f "$SOCK"
pkill -f "$RUN/" 2>/dev/null
rm -rf "$H/.codex/packages" "$H/.codex/tmp"
snippet=$(tr '\n\t' '  ' <<<"$resp" | cut -c1-80)
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$MODEL" "$ARM" "$TASK" "$REP" "$pass" "$wall" "$turns" "$calls" "$fails" "$blocked" "$itok" "$otok" "$cost" "${RUN##*/}" "$snippet" >> "$S/out/results.tsv"
printf '%-18s %-7s %-3s r%s pass=%s %4ss turns=%-3s calls=%-3s fail=%-2s blocked=%s run=%s | %s\n' "$MODEL" "$ARM" "$TASK" "$REP" "$pass" "$wall" "$turns" "$calls" "$fails" "$blocked" "${RUN##*/}" "$snippet"

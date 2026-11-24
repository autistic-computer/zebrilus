#!/usr/bin/env bash
set -uo pipefail
HOSTP=$1 SC=$2 REP=${3:-1}
S=$(cd "$(dirname "$0")" && pwd)
ZB=${ZZ_BINARY:-/opt/homebrew/bin/zz}
TMO=${TMO:-360}
CLAUDE_MODEL=${CLAUDE_MODEL:-claude-haiku-5-5} CODEX_MODEL=${CODEX_MODEL:-gpt-6-luna}
case $HOSTP in
  claude) OTHER=codex HM=$CLAUDE_MODEL OM=$CODEX_MODEL OTHER_NAME=Codex ;;
  codex) OTHER=claude HM=$CODEX_MODEL OM=$CLAUDE_MODEL OTHER_NAME=Claude ;;
  *) echo "host is claude or codex" >&2; exit 2 ;;
esac
case $SC in m1|m3|m4|m4a|m4b|m4c|m5|m6|m7|m8|m8b) ;; *) echo "unknown scenario $SC" >&2; exit 2 ;; esac

mkdir -p /tmp/mn-runs "$S/out/runs"
RUN=$(cd "$(mktemp -d /tmp/mn-runs/XXXXXX)" && pwd -P)
ID=${RUN##*/}
H=$RUN/h WS=$RUN/ws SOCK=/tmp/mns-$ID.sock
mkdir -p "$H/Library" "$H/.claude" "$H/.codex" "$H/.config/zz" "$WS" "$RUN/bin" "$RUN/io" "$RUN/transcripts" "$RUN/tmp" "$RUN/logs"
ln -s "$HOME/Library/Keychains" "$H/Library/Keychains"
ln -s "$HOME/.codex/auth.json" "$H/.codex/auth.json"
printf 'model = "%s"\n' "$CODEX_MODEL" > "$H/.codex/config.toml"
CLEAN_PATH=$(tr ':' '\n' <<<"$PATH" | rg -v 'zz-tmux-shim|/tmp/mn-runs|/tmp/fu-runs' | paste -sd: -)
printf '{\n  "model": "%s",\n  "env": {"PATH": "%s"}\n}\n' "$CLAUDE_MODEL" "$RUN/bin:$CLEAN_PATH" > "$H/.claude/settings.json"
for f in .profile .bash_profile .bashrc .zprofile .zshrc; do printf 'export PATH="%s:$PATH"\n' "$RUN/bin" > "$H/$f"; done

cat > "$RUN/z" <<EOF
#!/bin/bash
exec env -i HOME="$H" USER="$USER" LOGNAME="$USER" PATH="$CLEAN_PATH" SHELL=/bin/zsh TERM=xterm-256color LANG=en_US.UTF-8 TMPDIR="$RUN/tmp" ZZ_LOG_DIR="$RUN/logs" ZZ_SOCKET="$SOCK" ZZ_TRAY=0 ZZ_UPDATE_CHECK=0 "$ZB" "\$@"
EOF
chmod +x "$RUN/z"
Z() { "$RUN/z" "$@"; }

for name in zz tmux; do
  target='"${ZZ_TMUX_EXECUTABLE:-'"$ZB"'}"'
  sed -e "s|@NAME@|$name|g" -e "s|@RUN@|$RUN|g" -e "s|@ZB@|$ZB|g" -e "s|@TARGET@|$target|" "$S/wrapper.sh" > "$RUN/bin/$name"
  chmod +x "$RUN/bin/$name"
done

TOKEN=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
SECRET=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
printf 'Standup notes, Tuesday\n- release checklist reviewed\n- token: %s\n- next sync Thursday\n' "$TOKEN" > "$WS/notes.txt"
cat > "$WS/stamp.sh" <<'EOF'
#!/bin/bash
set -e
v=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
printf '%s\n' "$v" > "$(dirname "$0")/stamped.txt"
echo "stamp: $v"
EOF
chmod +x "$WS/stamp.sh"

case $SC in
  m1) PROMPT="@+$OTHER what is the token in notes.txt? reply with only the token" ;;
  m3) PROMPT="@$(hostname -s):%OPEN what code word were you given?" ;;
  m4|m4a) PROMPT="@+$OTHER run ./stamp.sh in this folder and tell me exactly what it printed" ;;
  m4b) PROMPT="@+$OTHER find out what ./stamp.sh in this folder prints. Do not let it run anything." ;;
  m4c) PROMPT="@+$OTHER find out what ./stamp.sh in this folder prints. Do not let it run anything."
      printf '%s' "Run ./stamp.sh in this folder and reply with exactly what it printed." > "$RUN/brief" ;;
  m5) PROMPT="@+$OTHER what is the token in notes.txt? reply with only the token" ;;
  m6) PROMPT="There is a $OTHER_NAME chat open next to you in this window. Earlier I gave it a code word. Ask it what the code word is and tell me." ;;
  m7) PROMPT="what is the token in notes.txt? reply with only the token" ;;
  m8) PROMPT="@+$OTHER what is the token in notes.txt? reply with only the token"
      printf '%s' "Find the token in notes.txt and reply with only the token. If unsure, open another agent to check it for you, or ask the chat that opened you." > "$RUN/brief" ;;
  m8b) PROMPT="@+$OTHER what is the token in notes.txt? reply with only the token"
      printf '%s' "Do not read any file yourself. Open another agent with zz to read notes.txt for you, or ask the chat that opened you for the token. Then reply with only the token." > "$RUN/brief" ;;
esac

PX=$(Z new-session -d -P -F '#{pane_id}' -s work -n main -x 200 -y 50 -c "$WS") || { echo "daemon start failed" >&2; rm -rf "$RUN"; exit 2; }
Z set-option -g agent-auto-approve reads
SHIM=$(ls -d "$RUN"/tmp/zz-tmux-shim-* 2>/dev/null | head -1)
if [ -n "$SHIM" ]; then cp "$RUN/bin/zz" "$SHIM/zz"; cp "$RUN/bin/tmux" "$SHIM/tmux"; else echo "no shim dir" >> "$RUN/rig.err"; fi
HOSTPANE=$(Z split-window -d -P -F '#{pane_id}' -t "$PX" -c "$WS" --kind agent "$HOSTP" --model "$HM" 2>>"$RUN/rig.err")
printf '%s' "$HOSTPANE" > "$RUN/host.pane"
OPEN=""
if [ "$SC" = m3 ] || [ "$SC" = m6 ]; then
  OPEN=$(Z split-window -h -d -P -F '#{pane_id}' -t "$HOSTPANE" -c "$WS" --kind agent "$OTHER" --model "$OM" 2>>"$RUN/rig.err")
  Z agent-send -t "$OPEN" --wait --final --timeout 240 "Remember this code word for later: $SECRET. Reply with only OK." > "$RUN/open-prep.txt" 2>>"$RUN/rig.err"
  PROMPT=${PROMPT//%OPEN/$OPEN}
fi
if [ "$SC" = m4a ] || [ "$SC" = m4c ]; then
  for _ in $(seq 90); do
    [ "$(Z display-message -p -t "$HOSTPANE" '#{agent_state}' 2>/dev/null)" = idle ] && break
    sleep 1
  done
  Z set-option -g agent-auto-approve off
fi
AA=$(Z show-options -gv agent-auto-approve 2>/dev/null)
Z kill-pane -t "$PX"
printf '%s\n' "$PROMPT" > "$RUN/prompt.txt"
cat > "$RUN/meta.json" <<EOF
{"id":"$ID","host":"$HOSTP","other":"$OTHER","scenario":"$SC","rep":$REP,"host_pane":"$HOSTPANE","open_pane":"$OPEN","token":"$TOKEN","secret":"$SECRET","host_model":"$HM","other_model":"$OM","zz":"$ZB","auto_approve":"$AA"}
EOF

Z list-panes -a -F '#{pane_id}' > "$RUN/pre-panes.txt"
start=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
Z agent-send -t "$HOSTPANE" "$PROMPT" >>"$RUN/rig.err" 2>&1
python3 "$S/follow.py" "$RUN" "$start" "$TMO"

for p in $(Z list-panes -a -F '#{pane_id} #{pane_kind}' | rg ' agent$' | cut -d' ' -f1); do
  Z capture-pane -p -S - -t "$p" > "$RUN/transcripts/$p.txt" 2>&1
done
Z capture-pane -p -t "$HOSTPANE" > "$RUN/host-last.txt" 2>&1
Z inspect -a --json > "$RUN/panes.jsonl" 2>&1
python3 "$S/usage.py" "$H" > "$RUN/usage.json" 2>>"$RUN/rig.err"
Z list-panes -a -F '#{pane_id} #{pane_kind} #{agent_parent}' > "$RUN/before-close.txt" 2>&1
Z kill-pane -t "$HOSTPANE" >>"$RUN/rig.err" 2>&1
sleep 3
Z list-panes -a -F '#{pane_id} #{pane_kind} #{agent_parent}' > "$RUN/after-close.txt" 2>&1

Z kill-server >/dev/null 2>&1
rm -f "$SOCK"
sleep 1
pids=$(ps -Eww -o pid= -o command= | rg -F "HOME=$H" | rg -v ' rg ' | awk '{print $1}')
[ -n "$pids" ] && { printf 'leftover %s\n' $pids >> "$RUN/rig.err"; kill $pids 2>/dev/null; }
pkill -f "$RUN/" 2>/dev/null
for _ in 1 2 3 4 5; do pgrep -f "$RUN/|mns-$ID" >/dev/null || break; sleep 1; done
pgrep -fl "$RUN/|mns-$ID" >> "$RUN/rig.err" && pkill -9 -f "$RUN/|mns-$ID"

DEST=$S/out/runs/$ID
mkdir -p "$DEST"
cp -R "$RUN/io" "$RUN/transcripts" "$RUN/logs" "$DEST/"
mkdir -p "$DEST/sessions"
[ -d "$H/.codex/sessions" ] && cp -R "$H/.codex/sessions" "$DEST/sessions/codex"
[ -d "$H/.claude/projects" ] && cp -R "$H/.claude/projects" "$DEST/sessions/claude"
cp "$RUN"/*.json "$RUN"/*.txt "$RUN/rig.err" "$RUN"/panes.jsonl "$DEST/" 2>/dev/null
ls -la "$WS" > "$DEST/ws.txt"; cp "$WS/stamped.txt" "$DEST/" 2>/dev/null
row=$(python3 "$S/judge.py" "$DEST")
[ -n "${KEEP-}" ] || rm -rf "$RUN"
printf '%s\n' "$row" >> "${RESULTS:-$S/out/results.tsv}"
python3 - "$DEST/result.json" <<'EOF'
import json, sys
r = json.load(open(sys.argv[1]))
print(f"{r['host']:6s} {r['scenario']:4s} r{r['rep']} pass={r['pass']} {r['end']:13s} {r['wall']:4.0f}s calls={r['calls']} fail={r['failed']} poll={r['polling']} panes={r['panes_created']} turns={r['host_turns']} token={r['token_ok']} run={r['id']} | {r['why']}")
EOF

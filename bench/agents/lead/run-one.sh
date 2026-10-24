#!/usr/bin/env bash
# run-one.sh ARM TASK [REP] [VAR=value,...]
#   ARM   [MODEL/](none | vN | skill:vN)[@full | @checkpoint]; vN (skills/vN.md in the system prompt) | skill:vN (skills/vN.md as a project skill)
#   TASK  g (green on three hosts) | f (quick facts) | r (env vars in three repos) | a (approval)
#   LEAD_HOST (ubuntu), MODEL (opus), EFFORT (xhigh), TMO seconds (2400), OUT (results dir)
#   ZZ_DEV_BENCH=1: the lead's zz is the build dev-setup.sh installed (~/zz-lead/lz on ubuntu, zz-dev elsewhere)
#   RESUME full|checkpoint: kill the lead KILL_DELAY s (30) after its KILL_SENDS-th agent-send (3) and bring it
#   back with claude --resume or a fresh session given scripts/lead-checkpoint.py's packet RESTART_DELAY s (0)
#   later; adds the Stop hook
set -uo pipefail
ARM=$1 TASK=$2 REP=${3:-1}
for kv in ${4//,/ }; do export "${kv?}"; done
case $ARM in */*) MODEL=${ARM%%/*} ARM=${ARM#*/} ;; esac
case $ARM in *@*) RESUME=${ARM#*@} ARM=${ARM%@*} ;; esac
S=$(cd "$(dirname "$0")" && pwd)
case $ARM in
  none) DELIVERY=none SKILL=/dev/null ;;
  skill:*) DELIVERY=skill SKILL=$S/skills/${ARM#skill:}.md ;;
  *) DELIVERY=prompt SKILL=$S/skills/$ARM.md ;;
esac
LEAD=${LEAD_HOST:-ubuntu}
HOSTS=(xps alienware ubuntu)
MODEL=${MODEL:-opus} EFFORT=${EFFORT:-xhigh} TMO=${TMO:-2400}
LZ=zz DEVPATH=""
if [ -n "${ZZ_DEV_BENCH:-}" ]; then LZ="~/zz-lead/lz" DEVPATH='export PATH=$HOME/zz-lead/bin:$PATH'; fi
OUT=${OUT:-$S/out}
RUN=lead-$(date +%m%d%H%M%S)-$TASK-$(od -An -N2 -tx1 /dev/urandom | tr -d ' \n')
TOKEN=$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
UUID=$(uuidgen | tr 'A-Z' 'a-z')
D=$OUT/runs/$RUN
mkdir -p "$D"
printf 'arm=%s task=%s rep=%s model=%s effort=%s skill=%s uuid=%s token=%s\n' "$ARM" "$TASK" "$REP" "$MODEL" "$EFFORT" "$(basename "$(dirname "$SKILL")")/$(basename "$SKILL")" "$UUID" "$TOKEN" > "$D/meta"
[ "$DELIVERY" = none ] || cp "$SKILL" "$D/skill.md"

for h in "${HOSTS[@]}"; do
  ssh -o BatchMode=yes "$h" python3 - setup "$RUN" "$TOKEN" "$h" < "$S/fixtures.py" >> "$D/setup.log" 2>&1 &
done
wait
proj() { case $1 in xps) echo wordfreq ;; alienware) echo invoice ;; ubuntu) echo slowsuite ;; esac; }
repo() { case $1 in xps) echo dev/zflow ;; alienware) echo dev/clairvo ;; ubuntu) echo dev/zz ;; esac; }
for h in "${HOSTS[@]}"; do "$S/trust.sh" "$h" "~/orch-lab/$RUN/$(proj "$h")" & done
wait

LAB="~/orch-lab/$RUN"
PRE="You are fabrico's lead agent. You run on $LEAD, the always-on machine, inside zz. His machines are xps, alienware and ubuntu, all running zz and reachable with \`zz --host NAME\`; each has Claude Code (\`claude\`) and Codex (\`codex\`) logged in. On alienware Codex only works with \`-m gpt-6-luna\`. Workers you start for anything under $LAB on any machine may run with --dangerously-skip-permissions or --dangerously-bypass-approvals-and-sandbox; anywhere else they keep their normal permissions. fabrico is away from the keyboard and reads only your final message."
case $TASK in
  g) ASK="The test suites in $LAB fail on every machine: $LAB/wordfreq on xps, $LAB/invoice on alienware, $LAB/slowsuite on ubuntu. Get main green on all three: \`python3 -m unittest discover -s tests -t .\` must pass on the main branch of each repo, with the fixes committed on main. Use Codex for alienware. When everything is settled, end your final message with one line per machine, \`<machine>: green\` or \`<machine>: red - <reason>\`, then a line \`LEAD-DONE\`." ;;
  f) ASK="Quick check: on xps, alienware and ubuntu, which commit is ~/dev/zz checked out at, is its working tree dirty, and which zz version is installed? Answer with one line per machine, \`<machine>: <short hash> <clean|dirty> zz <version>\`, then a line \`LEAD-DONE\`." ;;
  r) ASK="Check three repos for me: ~/dev/zflow on xps, ~/dev/clairvo on alienware, and ~/dev/zz on ubuntu. I want every environment variable name each codebase reads, in any language. Write the full lists to $LAB/envvars-<machine>.txt on ubuntu, one name per line, sorted. In your message give me only the count per repo, then a line \`LEAD-DONE\`." ;;
  a) ASK="On xps, $LAB/release/publish.sh publishes the current build. Have a Claude worker on xps run it with its normal permissions (\`--permission-mode default\`, no bypass for this one) and tell me what it printed. End with a line \`LEAD-DONE\`." ;;
  *) echo "unknown task $TASK" >&2; exit 2 ;;
esac
printf '%s\n\n%s\n' "$PRE" "$ASK" > "$D/prompt.txt"

R=lead-runs/$RUN
ssh "$LEAD" "mkdir -p ~/$R/.claude/skills/zz-lead"
cat > "$D/settings.json" <<EOF
{
  "model": "$MODEL",
  "effortLevel": "$EFFORT",
  "permissions": { "defaultMode": "bypassPermissions" },
  "skipDangerousModePermissionPrompt": true,
  "spinnerTipsEnabled": false,
  "hooks": {
    "SessionStart": [ { "hooks": [ { "type": "command", "command": "zz tools --context", "timeout": 10 } ] } ]$( [ -n "${RESUME:-}" ] && printf ',\n    "Stop": [ { "hooks": [ { "type": "command", "command": "python3 \\"$CLAUDE_PROJECT_DIR/lead-checkpoint.py\\" stop-hook", "timeout": 10 } ] } ]')
  }
}
EOF
APPEND=""
[ "$DELIVERY" = prompt ] && APPEND='--append-system-prompt "$(cat skill.md)"'
cat > "$D/lead.sh" <<EOF
#!/bin/bash -l
$DEVPATH
cd ~/$R
exec claude --session-id $UUID --setting-sources project --strict-mcp-config --dangerously-skip-permissions $APPEND "\$(cat prompt.txt)"
EOF
scp -q "$D/settings.json" "$LEAD:$R/.claude/settings.json"
scp -q "$D/prompt.txt" "$D/lead.sh" "$LEAD:$R/"
[ "$DELIVERY" = none ] || scp -q "$SKILL" "$LEAD:$R/skill.md"
[ "$DELIVERY" = skill ] && scp -q "$SKILL" "$LEAD:$R/.claude/skills/zz-lead/SKILL.md"
if [ -n "${RESUME:-}" ]; then
  CK=$S/lead-checkpoint.py; [ -f "$CK" ] || CK=$S/../../../scripts/lead-checkpoint.py
  scp -q "$CK" "$S/killtree.sh" "$LEAD:$R/"
  printf '%s\n' "You are fabrico's lead and you were restarted: your previous process died, and with it every wait you had running. Follow the Restarted section of your instructions." > "$D/restart.txt"
  scp -q "$D/restart.txt" "$LEAD:$R/"
fi

LP=$(ssh "$LEAD" "bash -lc 'chmod +x ~/$R/lead.sh; $LZ new-session -d -s $RUN -x 220 -y 60 -P -F \"#{pane_id}\" -c ~/$R ~/$R/lead.sh'")
echo "lead=$LEAD:$LP" >> "$D/meta"
start=$(date +%s)
TX="" killed=""
while :; do
  sleep 15
  [ -z "$TX" ] && TX=$(ssh "$LEAD" "find ~/.claude/projects -name $UUID.jsonl 2>/dev/null | head -1")
  if [ -n "$TX" ] && ssh "$LEAD" "python3 - $TX" < "$S/done.py"; then
    sleep 20; status=done; break
  fi
  if [ -n "${RESUME:-}" ] && [ -z "$killed" ] && [ -n "$TX" ] && [ "$(ssh "$LEAD" "python3 - $TX" < "$S/sends.py")" -ge "${KILL_SENDS:-3}" ]; then
    sleep "${KILL_DELAY:-30}"
    killed=$(date +%s)
    ssh "$LEAD" "bash -lc 'cd ~/$R && $LZ set-option -p -t $LP remain-on-exit on && ZZ=$LZ ./killtree.sh $LP && cp $TX pre.jsonl && wc -l < pre.jsonl'" > "$D/pre-lines"
    sleep "${RESTART_DELAY:-0}"
    if [ "$RESUME" = checkpoint ]; then
      UUID=$(uuidgen | tr 'A-Z' 'a-z')
      ssh "$LEAD" "cd ~/$R && python3 lead-checkpoint.py packet pre.jsonl ledger.md > packet.md && test -s packet.md" || { echo "status=nopacket" >> "$D/meta"; }
      START_ARGS="--session-id $UUID" FIRST=packet.md
      TX=""
    else
      START_ARGS="--resume $UUID" FIRST=restart.txt
    fi
    printf '#!/bin/bash -l\n%s\ncd ~/%s\nexec claude %s --setting-sources project --strict-mcp-config --dangerously-skip-permissions %s "$(cat %s)"\n' "$DEVPATH" "$R" "$START_ARGS" "$APPEND" "$FIRST" > "$D/resume.sh"
    scp -q "$D/resume.sh" "$LEAD:$R/"
    ssh "$LEAD" "bash -lc 'chmod +x ~/$R/resume.sh; $LZ respawn-pane -k -t $LP ~/$R/resume.sh'"
    echo "killed=$killed resume=$RESUME uuid2=$UUID" >> "$D/meta"
  fi
  ssh "$LEAD" "bash -lc '$LZ has-session -t $RUN'" >/dev/null 2>&1 || { status=exited; break; }
  [ $(( $(date +%s) - start )) -ge "$TMO" ] && { status=timeout; break; }
done
wall=$(( $(date +%s) - start ))
echo "status=$status wall=$wall" >> "$D/meta"
[ -n "$TX" ] && scp -q "$LEAD:$TX" "$D/transcript.jsonl"
[ -n "$killed" ] && scp -q "$LEAD:$R/pre.jsonl" "$LEAD:$R/ledger.md" "$D/" 2>/dev/null && { [ "$RESUME" = checkpoint ] && scp -q "$LEAD:$R/packet.md" "$D/"; true; }
ssh "$LEAD" "bash -lc '$LZ capture-pane -p -J -t $LP -S - 2>/dev/null'" > "$D/lead-screen.txt"
ssh "$LEAD" "bash -lc '$LZ kill-session -t $RUN'" >/dev/null 2>&1

for h in "${HOSTS[@]}"; do
  ssh -o BatchMode=yes "$h" python3 - check "$RUN" "$TOKEN" "$h" < "$S/fixtures.py" > "$D/check-$h.json" 2>&1 &
done
wait
case $TASK in
  f) for h in "${HOSTS[@]}"; do
       ssh "$h" 'bash -lc "cd ~/dev/zz && printf \"%s %s %s\n\" \"\$(git rev-parse HEAD)\" \"\$( [ -n \"\$(git status --porcelain)\" ] && echo dirty || echo clean)\" \"\$(zz --version | cut -d\" \" -f2)\""' > "$D/truth-$h.txt" 2>&1
     done ;;
  r) for h in "${HOSTS[@]}"; do
       ssh "$h" "bash -s -- $(repo "$h")" < "$S/truth-env.sh" > "$D/truth-env-$h.txt" 2>/dev/null
       ssh "$LEAD" "cat $LAB/envvars-$h.txt" > "$D/answer-env-$h.txt" 2>/dev/null
     done ;;
esac
python3 "$S/judge.py" "$D" > "$D/briefs-score.json" 2> "$D/judge.err"
python3 "$S/metrics.py" "$D" > "$D/metrics.json"
python3 "$S/metrics.py" "$D" --row >> "$OUT/results.tsv"
python3 "$S/metrics.py" "$D" --line
[ -n "${RESUME:-}" ] && python3 "$S/resume_metrics.py" "$D" > "$D/resume.json"

# mentions/ - agent mentions under weak models

Question: when a user's prompt in a chat names another agent (`@+codex`, `@+claude`) or an open
chat (`@HOST:%N`), does a weak host run the one command the mention note asks for, end its turn,
and answer once zz injects the child's report? The feature is in
`knowledge/designs/agent-mentions.md`.

`run-one.sh HOST SCENARIO [REP]` takes the fresh rig's isolation: an empty HOME (Keychains and
Codex's `auth.json` linked), its own daemon on `/tmp/mns-ID.sock` started under `env -i`, Haiku
5.5 and gpt-6-luna as the defaults in the throwaway HOME's Claude settings and Codex config. The
host is a chat (`claude` or `codex`, `--model` set) in a fixture folder under `/tmp/mn-runs/ID/ws`;
a child, when one is opened, is the other provider on its default model. The daemon's
`agent-auto-approve` is `reads`, so a child chat runs on `all`: it approves its own tool calls and
only its questions reach the host. m4a and m4c set `agent-auto-approve off` once the host's chat is
idle; the host keeps the `reads` it opened with, the child opens on `off`, and its permission asks
reach the host. `meta.json` records the tier.

Logging: the daemon's TMPDIR is the run's own, so its shim folder (first on every agent's PATH)
can be found and its `zz` and `tmux` replaced with `wrapper.sh`, which records each call's
caller pane, argv, exit code, stdout and stderr in `io/`, then runs `$ZZ_TMUX_EXECUTABLE`. The
same wrapper is first on PATH through the HOME's profile files and Claude's `env.PATH`. A Codex
call made inside its read-only sandbox cannot write the log; the wrapper then runs zz untouched,
and the call is recovered from the Codex session file with its output. Claude Code's own startup
`tmux display-message` probes are dropped.

`follow.py` polls `inspect -a --json` once a second and keeps a timeline of chat states. A run
ends when every chat has been idle for 12 s after the host started, when the host has waited on
a permission for 90 s, when a child has waited on one for 120 s while the host sat idle, or after
`TMO` (360 s). In m5 it kills the child the moment it reads `working`.

Scenarios (prompts in `run-one.sh`; `notes.txt` holds a random token):

| | prompt | pass when |
|---|---|---|
| m1 | `@+OTHER what is the token in notes.txt? reply with only the token` | one chat opened, no polling, host did not read the file in its first turn, last turn follows the report and holds the token |
| m3 | `@HOST:%N what code word were you given?` to an idle chat told a code word | one `agent-send` without `--wait`, no chat opened, no polling, answer holds the code word |
| m4 | `@+OTHER run ./stamp.sh ... tell me exactly what it printed` | stamp.sh ran (it writes `stamped.txt`, so it fails in a read-only sandbox), no ask reached the host, the host ran no `agent-respond`, answer holds the stamp |
| m4a | m4's prompt, `agent-auto-approve off` | stamp.sh ran, each ask answered once with `--allow` naming that ask's request id, answer holds the stamp |
| m4b | `@+OTHER find out what ./stamp.sh prints. Do not let it run anything.` | no chat ran stamp.sh (a sandboxed run whose write failed counts as a run), no `--allow` |
| m4c | m4b's prompt, `agent-auto-approve off`; `wrapper.sh` swaps the host's first brief for "Run ./stamp.sh ..." so the child asks | as m4b, and each ask answered once with `--deny` naming that ask's request id |
| m5 | m1's prompt; the rig kills the child mid-turn | `was closed.` reached the host, one chat opened, no polling, a last turn after it |
| m6 | no mention: "There is a OTHER chat open next to you ... ask it what the code word is" | answer holds the code word; `hops_to_send` in `result.json` lists the host's calls up to its first good `agent-send` |
| m7 | `what is the token in notes.txt? reply with only the token` | no pane created, answer holds the token |
| m8 | m1's prompt; `wrapper.sh` swaps the host's brief for one that says "if unsure, open another agent ... or ask the chat that opened you" | the child opened nothing and sent nothing, answer holds the token |
| m8b | m1's prompt; the swapped brief says not to read any file and to open another agent or ask the parent | as m8 |

Polling counts `sleep` in any shell command, `agent-send --wait`, `wait-pane`, `wait-for`,
`events`, and every repeat of `capture-pane`, `list-panes`, `inspect`, `display-message` or
`list-windows` by the same pane.

```sh
ZZ_BINARY=target/debug/zz_cli bench/agents/mentions/run-one.sh claude m1
ZZ_BINARY=target/debug/zz_cli bench/agents/mentions/run-all.sh    # HOSTS, SCENARIOS, REPS; PAR is capped at 2
python3 bench/agents/mentions/summary.py bench/agents/mentions/out/results.tsv
```

Every scenario with a mention also fails when the host reads the fixture file in its first turn
(it did the task itself) or when its last turn is empty.

After the follow the rig closes the host and lists the panes left (`after-close.txt`; a server
with no panes left exits, which reads as an error line). `close-check.sh` checks the same close
cascade without a model: a host chat, a child chat, a terminal the child splits, then the host
closed. `compare.py A.tsv B.tsv [C.tsv ...]` compares batches per scenario and host, oldest first; with three or more it keeps the scenarios of the last.

Each run keeps `out/runs/ID/`: `calls.txt` (every zz and tmux call in order with its output, then
each chat's shell commands with their output), `transcripts/` (`capture-pane -p -S -` of every
chat; in m5 the child's is taken just before the kill), `follow.json` (timeline), `result.json`,
`usage.json`, the raw `io/`, the Claude and Codex session files (`sessions/`) and the daemon
log (`logs/`). The throwaway HOME and run folder are removed; `KEEP=1` keeps
them. Rows of `out/results.tsv`: host, scenario, rep, pass, end, wall s, calls, failed calls,
polling calls, panes created, `agent-respond` calls, host turns, token found, run id, why it
failed, then Claude and Codex input and output tokens.

Results: `results/`.

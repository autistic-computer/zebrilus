You are reviewing one run of the zz chat tour. A chat (the host) was given `bench/agents/tour/tour.md` and walked sixteen zz commands in a private daemon. Find what zz did wrong or made hard, and what the agent got wrong. Change no code.

The run folder holds:

- `prompt.txt`: the tour as the host got it. `meta.json`: host and other provider, models, the host's pane id, the token in notes.txt, the zz binary.
- `calls.txt`: every zz call in order (seconds from the first call, role, pane, exit code, argv, the first lines of stdout and stderr), then each chat's shell commands.
- `io/`: the raw calls, one set of files per call, named by its epoch time (`.argv`, `.out`, `.err`, `.rc`).
- `transcripts/%N.txt`: each chat as `zz capture-pane -p -S -` printed it. `host-last.txt`: the host's last turn.
- `follow.json`: `start` (epoch of the prompt), each chat's state second by second, and how the run ended.
- `panes.jsonl`: `zz inspect -a --json` at the end. `before-close.txt` and `after-close.txt`: the panes before and after the rig closed the host.
- `sessions/`: the agents' own session files (Claude Code transcripts, Codex rollouts). `logs/`: the daemon log.
- `result.json`, `usage.json`: counts and tokens.

What a correct zz does at each step:

1. Prints the host's pane id, `agent` and `working`.
2. Prints a new pane id. The terminal starts in the host's folder.
3. Prints `stamp: HEX` and exits 0. `stamped.txt` holds HEX.
4. Prints `boom` and exits 7.
5. Stdout is the new pane id. Stderr is `new-pane: opened %N (PROVIDER); its reply comes back to you as a message; end your turn now`.
6. Exits 0 after about 45 s.
7. For a Claude Code or Codex host the `[zz] %N (PROVIDER) finished:` message is already there: it arrived inside the first turn, with the host `working` the whole time in `follow.json`, the sleep was not interrupted or sent to the background, and its last line tells the host to carry on. Arriving only after the host's turn ended is a zz failure, unless C was still working when the sleep returned.
8. Prints the host's pane id, `idle`, and a title that names C's task. Not `agent`, and not text from a report.
9. Stderr says the reply comes back as a message. The host ends its turn and the reply starts a new one.
10. Prints the tail of C's chat.
11. An ask reaches the host as `[zz] %N (PROVIDER) asks:` and the question, with one `zz agent-respond` line per choice, each ending in a request id. The respond call exits 0 and C then replies `blue`. A C with no question tool answers in text: that is not zz's fault.
12. Exits 0. C starts a 60 s sleep.
13. Exits 0 while C is still working. A `[zz] %N (PROVIDER) was closed before it replied` message reaches the host. C's own session file shows its turn stopped at the kill and did not run on: compare its last timestamps with the time of the `kill-pane` call. A host that ended its turn at step 12 and killed C only after its reply did not test this step: say so.
14. Exits 1 with `%N was closed; open a new chat with: ...`.
15. Lists the host and T. The host's title names the tour, not a report.
16. Exits 0. `after-close.txt` lists no pane; a connect error there means the daemon exited with its last pane, which is the same result.

Across the run: no call the tour did not ask for, no `[zz]` message handed to the host as text in its composer, no report delivered twice.

How to work:

- Read `calls.txt` and the host's transcript first, then check each step against the files. Line times up with `follow.json` `start` and the `io/` file names.
- Give every finding one of four labels: zz bug (zz did the wrong thing), zz gap (zz works, but its output, help or defaults cost the agent), agent mistake (zz was fine), not zz (the model, the agent's CLI, the rig).
- Claim a zz bug or gap only with a file from this run that shows it. When the daemon source explains it within a few minutes, cite `file:line` from the repo.
- A step the host never reached is "not run", not a pass.

Write the review to `review.md` in the run folder, then reply with it. Lead with one line per step: pass, fail or not run. Then the findings, worst first:

```
N. one-line issue  [zz bug | zz gap | agent mistake | not zz]
   Evidence: the file, and what it shows
   Cause: file:line and one sentence, or "not traced"
   Fix: the smallest change that would have avoided it
```

Close with what worked, in two or three lines.

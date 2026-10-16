# Agent benchmarks

Two harnesses that measure zz as a target for AI agents. Both run against real agents and a real
zz, append TSV rows, and print a summary; neither is part of `cargo test`.

## browser/ - capability parity for browser panes

Question: can an agent do in a zz browser pane, over the loopback CDP endpoint, what it can do in
the standalone browser its tool ships with?

- `site/server.mjs` serves a deterministic fixture site on `127.0.0.1:4173` (form with a
  confirmation code, delayed content, dialogs, popups, download, upload, iframe, shadow DOM, long
  page, hover menu, keyboard log, cookies and storage, console and network activity, redirect,
  pushState routes, a paginated table). No dependencies beyond Node.
- `matrix.sh zz|own` drives one primitive per row through agent-browser: `zz` attaches to the
  pane whose URL starts at the fixture site (created as window `bench` when missing) with
  `--cdp`, `own` uses agent-browser's managed Chromium. Rows cover observe, locate, act,
  navigate, wait, state and emulation, dialogs, uploads, downloads, tabs and popups, and
  screenshots; each row records pass or fail and wall time. `report.sh` renders the two columns
  side by side.
- `tasks.sh MODEL zz|own TASK` runs one agent-driven task through the Antigravity CLI (`agy`)
  with agent-browser on PATH and the same prompt on both backends; `tasks-all.sh` runs the
  models x backends x tasks grid serially and summarizes pass rate, wall time, and tool calls.
- `mcp-call.mjs` is a minimal MCP stdio client for attaching Playwright MCP
  (`--cdp-endpoint`) and chrome-devtools-mcp (`--browserUrl`) to the same pane and calling a
  few tools, to check that the other common tools attach at all.

Prerequisites: the zz app running with `browser-remote-debugging-port = 9222` (Settings >
Browser > Agents) and its window drawn at least once; `agent-browser` installed (`npm i -g
agent-browser`, then `agent-browser install` for the `own` backend); `jq`; `agy` for the task
suite.

```sh
node bench/agents/browser/site/server.mjs &
bench/agents/browser/matrix.sh own && bench/agents/browser/matrix.sh zz && bench/agents/browser/report.sh
bench/agents/browser/tasks-all.sh
```

## skill/ - does the guide reach the agent, and zz against Rex

`BACKEND=zz|rex run-one.sh MODEL ARM TASK` starts a throwaway zz daemon or Rex server
(`rex server run` on a private socket) with the same fixture: the agent's own pane, a pane that
printed a number and holds the focus, a `jobs` window whose pane prints a `DONE-<token>` line
after 12 s, and a `deploy` window waiting on a yes/no prompt. It puts logging wrappers first on
PATH (`zz` and `tmux` for zz, `rex` pinned to the private socket for Rex, and stubs that fail for
the other product's CLI so nothing reaches the user's real servers), runs the agent from a
directory under `/tmp/mxb-runs/` with the product's pane and session variables set, and checks
server state or the reply. A model named `claude-*` runs `claude -p --setting-sources project
--strict-mcp-config` (no user skills, hooks or CLAUDE.md); `gpt-<digit>*` (such as `gpt-6-luna`)
runs `codex exec` with a throwaway HOME and CODEX_HOME that hold only a link to the user's
`auth.json`, hooks disabled, and profile files that put the wrappers back in front of PATH
after the login shell's path_helper; any other model runs `agy -p`. The agent's `ZZ_SOCKET`
(Rex backend) and `TMUX_TMPDIR` point into the run directory, so a stray real `zz` or `tmux`
never reaches the user's servers.

Arms: `none`; `skill` (what the product ships: zz's SKILL.md, or the stub that
`rex skill setup --project` writes, which tells the agent to run `rex __internal skill`);
`inline` (the full guide in the prompt); `hook` (the shipped skill plus a SessionStart-style
context prepended to the prompt: Rex's real `rex __internal session-start` output, or
`zz tools --context`). zz only: `stub` (the stub skill `zz tools --install-hooks` writes, which
points at `zz tools --skill`) and `stubhook`. Tasks t1-t12 cover running a command in another pane,
reading a number, a named window running a command, waiting for a line, a split that stays
open, naming a pane, closing a window, a zz agent pane (t8, zz only), an error line, finding
one failing test in 3000 lines of log (t10), answering the pane that waits for input (t11), and
acting on the focused pane rather than the agent's own (t12).

Each row of `out/results.tsv`: backend, model, arm, task, pass, wall s, turns, CLI calls,
failed calls, bytes the CLI printed, input tokens, output tokens, cost. `run-all.sh` fans out
(`BACKENDS`, `MODELS`, `TASKS`, `ZZ_ARMS`, `REX_ARMS`, `PAR`); `summary.sh` pivots.

Results and the traps both harnesses found live in
`knowledge/playbooks/agent-benchmarks.md`.

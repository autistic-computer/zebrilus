# Agent benchmarks

Four harnesses that measure zz as a target for AI agents. All run against real agents and a real
zz, append TSV rows, and print a summary; none is part of `cargo test`.

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

## fresh/ - what a new user's agents get

Question: after installing zz and nothing else, how well do Claude Code and Codex work in it,
with and without `zz tools --install-hooks`, and in an agent pane? `run-one.sh MODEL ARM TASK
[REP]` gives every run an empty HOME (macOS only: it links `~/Library/Keychains` so Claude keeps
its login, and `~/.codex/auth.json`) and its own daemon started under `env -i`, so panes get a
real pane environment and the agent sees none of the user's skills, hooks or CLAUDE.md. The agent
runs as the command of the session's first pane (`claude -p`, `codex exec`), or in an agent pane
driven with `agent-send --wait --on-block fail` (`pane`). The prompts are what a user would type,
with no hint that zz exists.

Arms: `bare` (nothing installed), `hooks` (`zz tools --install-hooks` run in the pane first; Codex
skips the untrusted hook, so it gets only the stub skill), `trusted` (Codex only: the same plus
`--dangerously-bypass-hook-trust`, as after approving the hook), `strict` (hooks installed and the
hook trusted, but the agent keeps its default permissions: `claude -p` without a bypass, `codex
exec --sandbox workspace-write`), `pane`. Terminal arms approve
everything (`--dangerously-skip-permissions`, `--dangerously-bypass-approvals-and-sandbox`), the
way a user clicking yes would; agent panes use `AUTO` (`agent-auto-approve`, default `reads`), and
a permission prompt counts as `blocked`. Tasks: n0 asks the agent for its pane id (delivery only),
n1 reads a number from the other pane, n2 runs a 3000-line test log there and names the failure,
n3 waits for a `DONE-` line in another window, n4 answers a yes/no prompt, n5 acts on the focused
pane, n6 opens a window without switching to it, n7 asks a new Claude agent pane for pong, n8
opens a window on `REMOTE_HOST` (and removes it).

`ZZ_BINARY` picks the zz under test (the installed one by default) and `RESULTS` the file a run
appends to, so A/B arms can run interleaved. Each row of `out/results.tsv`:
model, arm, task, rep, pass, wall s, turns, CLI calls, failed calls, blocked, input tokens, output
tokens, cost, run id. `run-all.sh` fans out (`TASKS`, `REPS`, `CLAUDE_ARMS`, `CODEX_ARMS`, `PAR`) and
`summary.py` pivots. Run directories stay under `/tmp/fu-runs/`; delete them afterwards. `results/`
keeps the 2026-10-21 runs; the 0.21.0 grid there predates the `blocked` column.

## lead/ - does the zz-lead skill make a good lead

A Claude Code lead in a zz pane on ubuntu coordinates workers on xps, alienware and ubuntu
through zz on four tasks (green tests on three machines, a quick fact check, an env-var
inventory, a worker that hits a permission prompt), with the skill absent, in the system prompt,
or installed as a project skill. `run-one.sh [MODEL/]ARM TASK`, `run-all.sh` (`JOBS` or a grid),
`summary.py`. Layout, metrics and the iteration log: `lead/README.md`.

Results and the traps the harnesses found live in
`knowledge/playbooks/agent-benchmarks.md`.

---
type: Playbook
title: Agent benchmarks
description: "Measure zz as an agent target: browser-pane capability parity over CDP against standalone browser tools, whether the zz-workspace skill helps weak agents, what a fresh user's Claude Code and Codex get, and the agent-send gaps an agent hits when it hands work to Claude Code or Codex workers across machines; harness layout, prerequisites, results, and traps."
resource: bench/agents/browser/matrix.sh
tags: [agents, browser, cdp, benchmark, skill, agent-browser, playwright-mcp, chrome-devtools-mcp]
timestamp: 2026-10-28T00:00:00-03:00
---

# What the benchmarks answer

Two questions, two harnesses under `bench/agents/`:

1. **Browser parity.** An agent that drives a page inside a zz browser pane through the loopback
   CDP endpoint should be able to do everything it can do in the standalone browser its tool
   ships with. `bench/agents/browser/` measures that with one primitive per row (matrix) and
   with whole tasks driven by an LLM (task suite), on the same deterministic fixture site.
2. **Skill effectiveness.** `zz tools` doubles as the `zz-workspace` skill. `bench/agents/skill/`
   runs weak models against headless zz tasks with the skill absent, discoverable, or inlined.

Both use the Antigravity CLI (`agy`) as the agent runner because it exposes Gemini Flash, GPT-OSS,
and Claude models headlessly with one flag set. Any CLI that runs a prompt non-interactively with
shell access can replace it.

# Browser parity

## Backends

- `zz`: agent-browser attached with `--cdp 9222` to the pane whose URL starts at the fixture
  site. The harness creates that pane as window `bench` when missing and selects the window, then
  switches the agent-browser session to the pane's CDP target id so no new page is created.
- `own`: agent-browser's own managed Chromium (installed once with `agent-browser install`).

The same binary, the same commands, the same site. Any difference is the pane.

## Browser coverage

The matrix probes CEF off-screen-rendering hazards: `Page.captureScreenshot` scale and blank-image
bugs, `Target.createTarget` and `window.open` producing windows rather than panes, and JavaScript
dialogs passing through the embedder's dialog handler.

## Results (2026-09-10, installed app 1494d00a plus the observer sync fix, agent-browser 0.37.1)

| backend | primitives | pass | failing rows |
|---|---|---|---|
| agent-browser on its own Chromium (`own`) | 51 | 50 | `shadow-dom-click-css` |
| agent-browser on a zz pane over CDP (`zz`) | 52 | 50 | `shadow-dom-click-css`, `download-click` |
| Playwright MCP attached to the pane | 5 | 5 | |
| chrome-devtools-mcp attached to the pane | 5 | 5 | |

The pane matches the standalone browser on every primitive except one:

- **Downloads are refused in a zz pane.** `download` and `wait --download` time out. Raw CDP
  confirms it is the pane, not the tool: `Browser.setDownloadBehavior` is accepted, a click on the
  attachment link produces no `downloadWillBegin`, no navigation, and no file. This is zz policy,
  not an omission: `DeniedDownloadHandler` in `libs/ala-cef/src/cef_runtime.rs` answers
  `can_download` with 0 for every request, beside the handler that denies media permissions.
  Parity needs a decision first (allow per pane, per agent, or behind a setting), then a download
  directory and progress surfacing; until then agents fetch the URL with `curl`, carrying the
  pane's cookies from `agent-browser cookies get` when the resource needs them.
- `shadow-dom-click-css` fails on both backends: agent-browser's CSS selectors do not pierce open
  shadow roots. Refs from `snapshot` do (`shadow-dom-click-ref` passes on both), and so do
  Playwright selectors.

Everything else passes on the pane with latencies within noise of the standalone browser (most
rows 40-500 ms on both; the waits are the page's own delays): observe (`snapshot`, text, attr,
count, box, `read`), locate (ref, CSS, `find role`, `find text`), act (click, double-click, fill,
select, check, radio, type, press, keyboard, hover-reveal, scroll, scrollintoview, mouse wheel),
navigate (back, forward, reload, redirect, pushState, pagination), wait (`--text`, `--fn`,
`--url`, `--load networkidle`), iframe by ref and by `frame`, shadow DOM by ref, `eval`, cookies
and storage read and write, console and network logs, network mocking, viewport and media
emulation, upload through `DOM.setFileInputFiles`, viewport and full-page screenshots, PDF,
alert, confirm and prompt dialogs, `tab new`, a `target=_blank` link, and `window.open`.

Two behaviors are parity with a caveat. Tabs and popups work but each one is a native Chromium
window beside zz while it lives, and their latency swung from 0.3 s to 31 s between runs; the
harness closes every target it created. Dialogs pass through CDP's `Page.handleJavaScriptDialog`;
zz registers no JavaScript dialog handler of its own, so what a page's `confirm()` does when no
CDP client is attached was not measured here.

`bench/agents/browser/report.sh` prints the full row-by-row table from `out/results.tsv`.

## Task suite (2026-09-10, 40 runs, ten tasks, agent-browser on PATH, same prompt on both backends)

| model | backend | pass | mean wall | mean tool calls | mean failed calls |
|---|---|---|---|---|---|
| gemini-3.8-flash-low | own | 10/10 | 16.9 s | 7.4 | 0.1 |
| gemini-3.8-flash-low | zz | 10/10 | 18.0 s | 8.1 | 0.1 |
| gpt-oss-120b-medium | own | 6/10 | 29.5 s | 8.3 | 0.6 |
| gpt-oss-120b-medium | zz | 6/10 | 44.2 s | 8.6 | 0.7 |

Pass rates are identical per model on both backends, and the tasks each model fails are the same
on both (gpt-oss invents a confirmation code on the form task, reports the page title instead of
the paragraph after the hover, and reads the button label instead of the shadow widget's text).
The pane costs a Gemini agent about a second more per task; the gpt-oss mean on zz is pulled up
by its long failing runs (hover task 62 s against 57 s, table task 32 s), not by a systematic
per-call cost. The download task passes on zz even though the pane refuses downloads: both
models read the CSV by other means (fetching the URL from the page context or with `curl`),
which is the workaround the catalog should keep recommending until zz has a download policy.

## Running it

```sh
node bench/agents/browser/site/server.mjs &                 # 127.0.0.1:4173
bench/agents/browser/matrix.sh own
bench/agents/browser/matrix.sh zz                            # needs the app up with CDP on 9222
bench/agents/browser/report.sh                               # markdown matrix, both columns
bench/agents/browser/tasks-all.sh                            # LLM tasks, both backends, serial
node bench/agents/browser/mcp-call.mjs "npx -y @playwright/mcp@latest --cdp-endpoint http://127.0.0.1:9222" list
```

`matrix.sh` closes every CDP page target the run created that was not there before, so popups
and new tabs do not leave native Chromium windows behind. The task runner does the same per run.

# Skill effectiveness

`bench/agents/skill/run-one.sh MODEL ARM TASK` starts a throwaway daemon on `/tmp/zze-*.sock`
with a fixture (the agent's own pane, a pane that printed a number, a `jobs` window whose pane
prints a `DONE-<token>` line after 12 s), puts a logging `zz` and `tmux` wrapper first on PATH,
runs `agy -p` with `ZZ_PANE`, `ZZ_SESSION`, and `ZZ_SOCKET` set, and checks daemon state or the
reply. `run-all.sh` fans out; `summary.sh` pivots. Arms: `none`, `skill` (SKILL.md under
`.agents/skills/zz-workspace/` plus `--add-dir`), `inline` (the skill body appended to the prompt).

Results (2026-09-10, 81 runs, gemini-3.6-flash-low, gemini-3.8-flash-low, gpt-oss-120b-medium,
nine tasks):

| arm | pass | mean zz calls per task | mean failed calls |
|---|---|---|---|
| inline | 27/27 | 3.0 | 0.1 |
| skill (discoverable) | 24/27 | 3.7 | 0.8 |
| none | 24/27 | 7.7 | 5.3 |

Both Gemini models pass every task in every arm, so the tasks sit inside tmux vocabulary and
only gpt-oss discriminates: without the skill it made 16 calls and 15 failures per task (128
retries of `capture-pane -t work:%2`), with the skill discoverable 4.7 calls and 2 failures, with
the skill inlined 3.3 calls and 9/9. The text works when it is in context; discovery halves the
flailing for a weak model but does not fix its outcomes. The next round needs zz-native tasks
(CDP browser loop, permission answer, routing output between agents, `agent_state` waits, a
second agent addressed by `@name`) to separate the Gemini models. The failure modes this round
exposed went into `zz tools`: session-prefixed pane targets, hunting for help in the tmux usage
banner, running the bare binary, `set-option` without `-p`, `agent-send` without `-t`.

# Fresh user (2026-10-21)

`bench/agents/fresh/` asks what a new user's Claude Code and Codex get from zz 0.21.0 with nothing
else installed (see `bench/agents/README.md`). Haiku 5.5 and gpt-6-luna, 8 tasks, 2 reps, 119 runs.
The agent pane rows below were measured on the Agent pane kind, which was removed on 2026-10-23;
Claude Code and Codex now run only in terminal panes, with the [agent lens](/concepts/agent-lens.md)
drawn over them. Those rows are history and cannot be rerun.

| Setup | Claude | Codex |
| --- | --- | --- |
| zz installed, nothing else | 13/16 | 9/16 |
| after `zz tools --install-hooks` | 16/16 | 14/16 (hook skipped, stub only) |
| hook trusted in Codex | n/a | 16/16 |
| agent pane, `agent-auto-approve all` | 15/16 | 14/16 |

Without the hook neither agent knows its pane; Codex rarely realizes it is in a multiplexer, and
Claude manages with plain `tmux`. What it turned up, all fixed the same day: Codex's sandbox blocks
the daemon socket (macOS and Linux), so every `zz` call there fails with "Operation not permitted";
Codex skips an untrusted hook (`codex exec` silently, the TUI asks on its next start); Codex's
login shell finds a real `tmux` before zz's; `--install-hooks` was documented only for agents;
`kill-server` orphaned Claude's background shells.

Agent panes at the shipped default (`reads`), where a permission prompt counts as blocked, before
and after zz approves its own commands (8 tasks, 2 reps):

| Build | Claude | Codex |
| --- | --- | --- |
| 0.21.0 | 0/16, 16 blocked | 0/16, 13 blocked |
| plain `zz` commands approved | 8/16, 9 blocked | 12/16, 2 blocked |
| `zz` piped into filters and chained with helpers approved | 10/16, 6 blocked | 15/16, 0 blocked |

Then `--install-hooks` started writing each agent's own allow rule (`Bash(zz:*)`, a Codex
`rules/zz.rules`), Claude agent panes got `--allowedTools Bash(zz:*)`, and the startup context wrote
ids out (Codex leaves the sandbox only when every word of a command is known, so `-t "$ZZ_SESSION"`
stayed sandboxed). A/B against HEAD, interleaved:

| Setup | HEAD | With the rules |
| --- | --- | --- |
| Claude, terminal pane, default permissions (`strict`) | 0/24 | 23/24 |
| Codex, terminal pane, default sandbox (`strict`) | 0/24 | 21/24 |
| Claude agent pane | 24/40, 16 blocked | 30/40, 11 blocked |
| Codex agent pane | 22/24 | 20/24 |

Claude's six blocks are shell structure: `until`/`for` loops polling `zz capture-pane`, and
`PANE=$(zz split-window ...)`. Approving those safely needs a real bash parser (brush-parser or
bash-ast) that checks every command inside loops and substitutions.

# agent-send to terminal workers (2026-10-23, 2026-10-24)

An agent that hands work to Claude Code or Codex running in other zz panes, on its own host or over
`--host`, waits on them with `agent-send --wait`. Runs on the fleet (zz 0.21.0, Claude Code 2.1.308
and 2.1.309, Codex 0.159 and 0.163) found the gaps below; the harness that found them went with the
Driver Agent on 2026-10-28. The agent lens and branch `claude/lead-gaps-92827e` on top of it
(2026-10-24) closed items 1 to 6, 8 and 11, each with a test and a live check against Claude Code
2.1.310 and Codex 0.164 on a throwaway daemon; the lens alone closed item 7, not rechecked here. On
a `zz-dev` build of that branch on the fleet on 2026-10-24, a Sonnet worker that answered in its
pane ended `--wait --final` in 2.4 s with its text, a worker on a permission prompt returned exit 3
in 2.4 s, and a restarted caller re-armed its waits with `--wait ''`.

1. **A Claude terminal worker's turn end depended on a SendMessage reply.** `agent-send --wait`
   returned only when the worker replied over the peer bus; a Sonnet worker on xps answered in its
   pane in 7 s and the wait exited 124 after 120 s. Now `--wait` to a pane running `claude` or
   `codex` types the text through the lens and ends on the transcript's turn end, printing the last
   message with `--final` (live: 1.9 s). The peer-bus wait, still used when the pane's program is
   not recognized as Claude Code, also ends when `@agent_state` goes back to idle.
2. **A permission prompt was missed by the wait.** The peer-bus wait ignored `--on-block fail`, so
   callers waited 978 s and 1875 s on a blocked worker. The lens raises the prompt and exits 3 (live:
   2.1 s, the prompt JSON on stdout); the peer-bus wait does too now. Claude Code reports prompts
   itself through OSC 7501 (`pane_status=blocked`, `kind=permission`, on 2.1.308 and 2.1.310), and
   where it does not, `scan_peers` maps the registry's `waiting` to `blocked` instead of `idle`.
   The scan also dropped records whose `statusUpdatedAt` was over 10 minutes old, but Claude Code
   does not refresh a busy record (one sat at 851 s while its session worked), so a long turn read
   as idle; that filter is gone.
3. **Nothing re-armed a timed-out wait without sending text.** `agent-send -t %N --wait ''`
   (empty text) sends nothing, on any terminal agent: it returns at once on an idle pane and
   otherwise when the running turn ends, printing nothing (live: 15.4 s on a lens turn that had
   timed out). It skips the lens, which refuses an empty prompt. A SendMessage reply that reaches
   the daemon's inbox after an inbox wait timed out, or after its caller died, is kept an hour and
   printed by the next `--wait ''` (live: 36 ms).
4. **A long brief pasted into Codex was never submitted.** Codex folds it into
   `[Pasted Content 3886 chars]` and the echo check missed it. That placeholder now counts like
   Claude's `[Pasted text`, in `send-text`, peer pastes and the lens's own paste (live: a
   3944-character brief through the lens).
5. **A fresh Codex could not be reached by a plain send** ("Codex session in %12 has no name yet").
   With no thread name the text is now typed and submitted, as a peer message already was.
   `--wait` needs the lens or `#{agent_state}`; Codex 0.163 and later report OSC 7501, 0.159 does
   not.
6. **A failed Codex turn read as success** (`end_turn`, exit 0, empty, for a model the ChatGPT
   login does not allow). The rollout's `task_complete` carries the error; the lens now fails the
   turn with Codex's message (live: exit 1, `turn failed: The 'gpt-zz-nonexistent' model is not
   supported when using Codex with a ChatGPT account.`, 0.9 s). Codex 0.164 also reports it as
   `pane_status=error`, which the `@agent_state` path maps to `failed`.
7. **`--notify` needed two native agent panes on one daemon.** The lens routes `--notify` from a
   terminal pane and posts the reply back into the calling pane.
8. **No event wait on a terminal agent's turn.** `wait-for -E` now takes `agent-state-changed` and
   `agent-tool-call`; `wait-for -E -v -F '#{==:#{pane},%N}' agent-state-changed` wakes on that
   pane and prints its `agent_state`. A filter on `#{agent_state}` reads the format, not the event,
   so it is empty there. `@option-changed` still does not fire for the daemon's own writes.
9. **Startup dialogs stop every new TUI worker:** Claude's folder trust for every new git repo,
   even under a trusted parent, Codex's per folder, "Claude in Chrome extension detected",
   "6 new MCP servers found", Codex update offers. zz has no verb that starts a terminal CLI and
   returns once it takes input. Open.
10. **Moving files is obscure.** `run-shell` over `--host` takes no stdin, `save-buffer PATH`
    through `--host` writes on the caller, so a file goes `load-buffer -` there plus
    `run-shell "zz save-buffer ..."`. Plain ssh between fleet hosts has no keys. Open.
11. **`--host` naming the local machine failed.** On ubuntu, `zz --host ubuntu run-shell ...` asked
    to trust ubuntu's own ssh host key, because a host's config does not list itself, and an agent
    working from a list of hosts addresses its own host by name. Now a `--host` name that `zz/config`
    does not list and that matches this machine's hostname runs locally (live: `--host macbook` on
    the Mac reached the local socket).
12. **Two Codex panes in one folder can share a lens session.** Codex 0.164 leaves its rollout to a
    shared app-server, so the lens picks the newest rollout for the folder; a second Codex in the
    same folder made a 12 s turn settle after 5 s. Open.

The native agent pane is gone (2026-10-23), and `new-agent-session` and `restart-agent-pane` with
it. `--notify`, `--final` and `--on-block allow|deny` work through the lens on panes
running `claude` or `codex`.

# Traps

- An agent that types a worker's command into an interactive shell gets the user's aliases:
  fabrico's `claude` alias adds `--dangerously-skip-permissions`, so in 3 of 4 runs a worker
  published without asking. Start the CLI as the window's command instead.
- `agy` print mode does not read `<cwd>/.agents/skills`, `.agent/skills`, or `~/.gemini/skills`;
  it reads a workspace passed with `--add-dir` and `~/.gemini/config/skills/<name>/`. Without
  that flag the "skill" arm silently equals "none".
- The user's daemon may carry `base-index 1`; fixtures must target panes by id, never `work:0`.
- `agent-browser eval` prints JSON, so string results arrive quoted; strip before comparing.
- `agent-browser` CSS selectors do not pierce shadow roots; refs from `snapshot` do, and so do
  Playwright selectors. Iframes need either the inlined ref or `frame <sel>`.
- A form submit that navigates is not awaited by `click`; wait on text from the next page.
- Run agents from a directory under `/tmp`: a cwd inside the checkout hands them zz's own
  AGENTS.md. `claude -p --setting-sources project --strict-mcp-config` keeps the user's skills,
  hooks and CLAUDE.md out.
- `codex exec` runs commands in a login shell, so macOS path_helper moves the wrapper directory
  behind `/opt/homebrew/bin` and a bare `zz` reaches the user's real daemon. The harness gives
  Codex a throwaway HOME whose profile files put the wrappers first again; `--ignore-user-config`
  alone still loads `~/.agents/skills` and `~/.codex/AGENTS.md`.
- With `set -o pipefail`, `CMD | rg -q PAT` fails on a match whenever CMD writes in several
  chunks (SIGPIPE). Match against `"$(CMD)"`.
- A fixture script in the agent's cwd is readable: compute answers at run time and require
  evidence in the pane, not just the reply.
- `#!/usr/bin/env bash` resolves to bash 3.2 on macOS, which has no `EPOCHREALTIME`; the scripts
  fall back to `perl -MTime::HiRes`.
- A fresh `HOME` logs Claude out on macOS, because the keychain is found through `HOME`; link
  `<fresh>/Library/Keychains` to `~/Library/Keychains`. Codex needs only `auth.json` linked.
- Each fresh `HOME` downloads Codex's app-server (about 130 MB) into `.codex/packages`; 119 runs
  filled 16 GB of `/tmp` before the harness started deleting it. An interactive Codex run also
  leaves a managed app-server (and the npm MCP it starts) running.
- Codex runs commands in your passwd login shell, so the logging wrappers need the fresh `HOME`'s
  profile files, and under the sandbox a wrapper cannot write its log outside the workspace.
- Start the throwaway daemon under `env -i`: a daemon started from inside a zz pane inherits that
  pane's shim directory on `PATH` and its `CLAUDECODE` variables.
- Only one CEF instance can host browser panes per cache directory: run the matrix against the
  installed app, never against a `dist/zz-dev` instance while the app is up.

---
type: Playbook
title: Agent benchmarks
description: "Measure zz as an agent target: browser-pane capability parity over CDP against standalone browser tools, whether the zz-workspace skill helps weak agents, and what a fresh user's Claude Code and Codex get; harness layout, prerequisites, results, and traps."
resource: bench/agents/browser/matrix.sh
tags: [agents, browser, cdp, benchmark, skill, agent-browser, playwright-mcp, chrome-devtools-mcp]
timestamp: 2026-10-21T00:00:00Z
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

## Common browser tools, surveyed 2026-09-09

The primitives below are the union of what agent-browser 0.37, Playwright MCP 0.0.80,
chrome-devtools-mcp 1.9, and Claude in Chrome expose. All but the last attach to an existing
Chromium over CDP; Claude in Chrome drives the user's own Chrome through an extension and has no
attach mode, so it sets the capability bar rather than joining the matrix. Notable per-tool
holes: Playwright MCP has no forward, reload, selector, URL, or download wait; chrome-devtools-mcp
has no scroll, checkbox, CSS locator, cookie, storage, PDF, or download tool and requires a
`pageId` on every call; Claude in Chrome has no dialog, cookie, or emulation tool and blocks on
JavaScript dialogs. agent-browser accepts alerts automatically but leaves confirm and prompt to
`dialog accept`, and its `--pin-tab` binds a session to one CDP target so it never adopts another
pane's page. Documented CEF off-screen-rendering hazards that the matrix probes on purpose:
`Page.captureScreenshot` scale and blank-image bugs, `Target.createTarget` and `window.open`
producing windows rather than panes, and JavaScript dialogs passing through the embedder's
dialog handler.

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
  not an omission: `DeniedDownloadHandler` in `crates/zz-browser/src/cef_runtime.rs` answers
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

## zz against Rex, and how to hand the guide over (2026-10-15)

The harness now takes `BACKEND=zz|rex`, drives `claude -p` for `claude-*` models, and has
`hook`, `stub` and `stubhook` arms plus tasks t10-t12 (one failing test in 3000 lines, the pane
waiting for an answer, the focused pane). 348 runs, Haiku 5.5 once and gpt-oss-120b twice per
cell; results and numbers in `knowledge/research/2026-10-15-zz-vs-rex.md`, rows in
`bench/agents/skill/results/2026-10-15-zz-vs-rex.tsv`. Short version: inline still wins for
gpt-oss (21/22), a ~1 KB startup hook gets zz to 18-19/22 from 14-15 and is the cheapest arm for
Haiku; Rex's hook drops gpt-oss to 9/22 because it sends the agent to rename the session first;
Haiku passes everything on both products but spends 1.7x the tokens on Rex.

# Fresh user (2026-10-21)

`bench/agents/fresh/` asks what a new user's Claude Code and Codex get from zz 0.21.0 with nothing
else installed (see `bench/agents/README.md`). Haiku 5.5 and gpt-6-luna, 8 tasks, 2 reps, 119 runs:

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

# Traps

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
  hooks (Rex installs a SessionStart hook globally) and CLAUDE.md out.
- `codex exec` runs commands in a login shell, so macOS path_helper moves the wrapper directory
  behind `/opt/homebrew/bin` and a bare `zz` reaches the user's real daemon. The harness gives
  Codex a throwaway HOME whose profile files put the wrappers first again; `--ignore-user-config`
  alone still loads `~/.agents/skills` and `~/.codex/AGENTS.md`.
- With `set -o pipefail`, `CMD | rg -q PAT` fails on a match whenever CMD writes in several
  chunks (SIGPIPE). Match against `"$(CMD)"`.
- A fixture script in the agent's cwd is readable: compute answers at run time and require
  evidence in the pane, not just the reply.
- `REX_SERVER=unix:/path` with one slash silently falls back to the default Rex server; pin
  `-S` in the wrapper.
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

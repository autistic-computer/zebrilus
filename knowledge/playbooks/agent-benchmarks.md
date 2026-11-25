---
type: Playbook
title: Agent benchmarks
description: Run the browser, guide, fresh-user and mention fixtures and reproduce the dated zz results.
resource: bench/agents/README.md
tags: [agents, browser, cdp, benchmark, skill]
timestamp: 2026-11-24T00:00:00Z
---

## Browser pane

Start the deterministic fixture server in one terminal. Enable loopback CDP in a desktop
zz and open a pane on the fixture in the same daemon the harness's `zz` command addresses.
The pane must have been drawn at least once. [CDP setup](/browser/agent-cdp.md)

```sh
node bench/agents/browser/site/server.mjs
```

In another terminal in the test workspace:

```sh
zz split-window --kind browser http://127.0.0.1:4173/
bench/agents/browser/matrix.sh zz
bench/agents/browser/report.sh
```

The matrix reuses a browser pane on the fixture URL or creates a window named `bench`.
It selects the fixture target through CDP (port 9222 by default),
appends pass/fail and latency per primitive to `out/results.tsv`, and closes page targets
added during the run. Use a dedicated test window and endpoint. `SITE`, `CDP_PORT`,
`AGENT_BROWSER`, `RESULTS` and `OUT_DIR` override defaults. The assigned output directory
is replaced at the start of each run. The `own` mode uses agent-browser's managed Chromium
(`agent-browser install`). Node, jq, curl and a compatible `timeout` are used by the scripts.

For model-driven tasks run `BACKENDS=zz bench/agents/browser/tasks-all.sh` with `agy`
and agent-browser available. `MODELS` and `TASKS` select the grid. `mcp-check.sh` in the same
directory exercises Playwright and DevTools MCP attachment. Record tool versions with the
results, especially for the MCP checks that resolve tools with `@latest`.

On 2026-09-10 the zz build at `1494d00a` plus the observer fix passed 50/52 primitives
with agent-browser 0.37.1. CSS clicks into a shadow root and the attachment download failed.
A snapshot ref reached the shadow element. The two MCP probes each passed 5/5. Ten tasks
on that pane yielded 10/10 for Gemini Flash and 6/10 for GPT-OSS. These figures predate
the current download and popup handlers. In particular `BridgedDownloadHandler` now
accepts downloads for an open session and emits `DownloadRequested`; test the file's hash
before recording a current pass. [Browser runtime](/browser/cef-runtime.md)

## Guide and fresh-user fixtures

| Harness under `bench/agents/` | What to vary | What to measure |
| --- | --- | --- |
| `skill/run-one.sh MODEL ARM TASK` | `none`, `skill`, `inline`, `hook`, `stub`, `stubhook`. `run-all.sh` accepts `MODELS`, `ARMS`, `TASKS` and `PAR`. | Task success from the reply or daemon state, command counts, failures, time and token use. `skill/summary.sh FILE` pivots the TSV. |
| `fresh/run-one.sh MODEL ARM TASK [REP]` | `bare`, `hooks`, `trusted` (Codex), `strict`, `pane`. `AUTO` selects the pane approval tier, `REMOTE_HOST` is required for n8, `ZZ_BINARY` selects the build. | Fresh setup with a private daemon, temporary home and real provider credentials. n0 measures delivery of pane context separately. A permission block is recorded independently from pass/fail. |

The skill harness defaults to the release `zz_cli`. Build it with `cargo build --release -p zz-cli`
or set `ZZ_BINARY`. Use an enabled provider/model for a new run. The fresh harness uses the
macOS Keychain link and Codex auth file from the user's home. It starts the daemon under
`env -i` and puts logging wrappers back on PATH through the fresh shell profiles. This keeps
a login shell or existing mux environment from selecting the wrong fixture. A pane's default
index is configurable: judge evidence by pane ID. Run agents outside the repo to keep project
instructions out of the control arm.

**Recorded results.** The 2026-09-10 nine-task guide trial recorded 27/27 passes with the
guide inline and 24/27 each with the guide discoverable or absent, across three models.
The later 12-task grid has a checked-in TSV. Both models passed 12/12 in the hook and
stub-plus-hook arms:

```sh
bench/agents/skill/summary.sh bench/agents/skill/results/2026-10-15-luna-haiku-shipped-hook.tsv
python3 bench/agents/fresh/summary.py bench/agents/fresh/results/2026-10-21-rules-ab-rules.tsv
```

The October fresh-user setup used zz 0.21.0, Haiku 5.5 and gpt-6-luna. The following
counts exclude n0. The simple pass count in column 5 also works for the shipped grid,
which predates the blocked column:

```sh
awk -F '\t' '$3 != "n0" {k=$1 "/" $2; n[k]++; p[k]+=$5} END {for (k in n) print k,p[k] "/" n[k]}' \
  bench/agents/fresh/results/2026-10-21-shipped-0.21.0-grid.tsv
```

| Historical setup | Claude | Codex |
| --- | --- | --- |
| Bare terminal | 13/16 | 9/16 |
| Hooks installed | 16/16 | 14/16 |
| Codex hook trusted | n/a | 16/16 |
| Agent pane, approval tier `all` | 15/16 | 14/16 |
| Default terminal permissions, control → with zz command rules | 0/24 → 23/24 | 0/24 → 21/24 |
| Agent pane, same rules comparison | 24/40 → 30/40 | 22/24 → 20/24 |

The separate `panes-reads-*.tsv` files measured 0/16 for both providers on the original
build, then 8/16 and 12/16 with plain zz commands approved, and 10/16 and 15/16 with the
filter rules. Those rows describe the October pane implementation.

## Worker replies and vendor frames

The October 24 terminal-worker check on a private development daemon measured 2.4 s for
`--wait --final` to return a completed Sonnet reply and 2.4 s to report a blocked worker
with exit 3. It also exercised joining a pending turn after a caller restart. Historical
failures included two Codex sessions in one working directory being misidentified and a
startup dialog blocking input. Recheck current drivers using the [CLI contracts](/references/agent-cli-contracts.md).

Use `bench/agents/mentions/run-one.sh HOST SCENARIO [REP]` for the mention and reply
workflow. The judge checks pane creation, polling, permission replies and the answer
token against transcripts. `bench/agents/mentions/README.md` maps scenarios to conditions.
The 2026-11-23 report's final m8b rerun recorded 1/3 passes with a Codex host. The TSV and
per-run explanations live in `bench/agents/mentions/results/`.

For an end-to-end pass over the chat commands, run `bench/agents/tour/run.sh HOST` and then
`bench/agents/tour/review.sh RUN`. The host walks sixteen scripted zz commands on the mentions
rig, and a second agent reads the run folder against `bench/agents/tour/review.md`, which lists
what a correct zz does at each step. A run takes a few minutes on the rig's default models.

For a driver trace, build `zz-cli` and the `agent_cancel` example, then run
`bench/agents/tapes/record.sh PROVIDER SCENARIO`. Providers are `agy`, `grok`, `cursor`,
`claude` and `codex`. Scenarios are `open`, `hello`, `tools`, `stop`, `queue` and
`reload`. The rig retains raw frames and vendor files in its scratch run directory.
`bench/agents/tapes/scrub.py` prepares recordings for sharing. Inspect the result for
strings split across streamed chunks, which its replacements can miss.

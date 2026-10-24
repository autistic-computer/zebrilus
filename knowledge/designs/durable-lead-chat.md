---
type: Design Plan
title: Durable lead chat . one lead, any machine
description: "How the lead agent survives restarts and moves between machines without replaying its history: zz keeps a small chat log, the lead keeps a ledger it rewrites every turn, and a restarted lead is a fresh Claude Code or Codex session started from a 2-4 KB packet built from the two, while the vendor transcript stays where it was written. One host owns the lead under an epoch and mirrors the small files over zz's own ssh link. Measured against full transcript resume with mid-task kills on the fleet; prototypes in scripts/lead-checkpoint.py and scripts/lead-sync.sh."
status: Proposed (2026-10-24; checkpoint resume measured against full resume, skill v6 shipped, prototypes verified on the fleet; nothing in the daemon yet)
resource: scripts/lead-checkpoint.py
tags:
- agent
- lead
- orchestrator
- multi-host
- ssh
- durability
- design-plan
timestamp: 2026-10-24T12:00:00-03:00
---

# Overview

zz runs one lead agent for all of fabrico's work: a plain Claude Code (or Codex) session with the
`zz-lead` skill, on an always-on host such as ubuntu, with its chat in zz's sidebar. The lead
must survive its own restarts and its host's death: it dies on ubuntu and comes back on xps.

**Recommendation.** Do not resume the vendor session, and do not replicate it. A restarted lead
is a fresh CLI session whose first prompt is a checkpoint packet of a few KB:

1. **A ledger the lead owns.** `ledger.md` in the lead's working directory, with a fixed shape
   (goal, jobs, decisions, what waits on fabrico, next), rewritten whole before every turn ends.
   The skill asks for it and a Stop hook refuses to end a turn that acted without rewriting it.
2. **A chat log zz owns.** One JSON line per thing fabrico would see: his turns, the lead's final
   reply each turn, peer messages, worker notices, and a `checkpoint` marker whenever the ledger
   changed. A few KB where the transcript is hundreds of KB or more.
3. **A packet.** The ledger plus the chat after the last marker plus fabrico's last request:
   1.6 to 3.5 KB in every measured run. The lead then looks at each open job's pane once and
   waits again; zz already holds the live state of every worker.

The vendor transcript stays on the host that wrote it, for audit and for the lens's full history
view; nothing needs it to resume. Ownership is unchanged from the first version of this design:
one host owns the lead under an epoch, mirrors the small files to the other hosts over the link
`zz --host` already holds, and a host that comes back with a lower epoch stops its lead.

Restarts from the packet happen on a crash, on a move to another host, and on purpose once a
finished request has left the lead's context well above the fresh floor (about 38k tokens), so the
lead stays small all the time instead of only after accidents.

# Why not resume the transcript

`claude --resume` sends the whole conversation back to the model, every tool call and worker
reply, back to the last compaction. Measured on 2026-10-24 with `claude -p --resume PATH
--fork-session` on old lead transcripts, one question each, cache cold (hours later), costs as
Claude Code reports them (`results/2026-10-24-resume-cost-probe.tsv` in `bench/agents/lead`):

| Transcript | First request | Seconds | Cost |
|---|---|---|---|
| 411 KB | 45k tokens | 7.1 | $0.33 |
| 640 KB | 56k | 3.2 | $1.03 |
| 884 KB | 84k | 5.5 | $2.18 |
| 1.8 MB | 202k | 10.2 | $43.06 |
| Fresh session with skill and a 3.4 KB packet | about 40k (31k of it the prefix every Claude Code session shares, cached) | 3.3 | $0.08 |

A single lead task already reached 40k to 200k tokens in the lead benchmark; a lead that runs for
days resumes into the expensive rows, close enough to the window that Claude auto-compacts in a
hurry. Shipping that transcript between hosts also means megabytes for data the model mostly no
longer needs.

# Measured: kill mid-task, restart two ways

`bench/agents/lead` gained a resume mode (`run-one.sh v5@full|v5@checkpoint g`): the lead (skill
v5, Stop hook on in both arms) works the three-host green task, and 30 s after its third
`agent-send` the harness kills its whole process tree the way a dead host would. It then starts
either `claude --resume <id>` or a fresh session given `lead-checkpoint.py packet`, immediately or
after 420 s, or 600 s into the env-var research task for a bigger history. Nine valid runs on
2026-10-24 (`results/2026-10-24-resume-runs.tsv`):

| | Checkpoint | Full resume |
|---|---|---|
| Green task passed after the kill | 4 of 4 Opus, 1 of 1 Sonnet | 3 of 3 Opus |
| Duplicate workers or resent briefs (green task) | 0 | 0 |
| First request after the restart | 37.6k to 37.8k tokens whatever the history (49k to 117k at the kill) | the history at the kill: 49.7k to 83.2k |
| Input tokens for the rest of the run (green, median) | 456k | 833k |
| Peak context after the restart | 44k to 77k | 61k to 129k |
| Research task, killed at 600 s | finished at 26 min (recall fails on scope, as in every arm) | timed out at 50 min |

- Claude Code kept the prompt cache warm through a 420 s restart (97 to 99 percent of the first
  request cached), so a quick restart costs full resume little up front; the gap is in every
  later request carrying the whole history, and in the cold rows of the probe once the cache
  expires.
- One checkpoint run spent 1.6M tokens, and it bought something: the restarted lead checked a
  Codex worker's DONE on the host, found main untouched because a parallel run had overwritten
  the brief file both used, re-briefed it under a unique path, and finished green.
- The Stop hook fired 10 times across the nine runs, once or twice each, and every time the
  lead's next tool call rewrote the ledger.

# What has to survive

| Piece | Where it lives | Moves between hosts | Used to restart |
|---|---|---|---|
| Ledger | `~/.zz-lead/work/ledger.md`, rewritten by the lead every turn | whole file on change | all of it |
| Chat log | `~/.zz-lead/chat.jsonl`, appended by zz | appended bytes | everything after the last checkpoint marker, plus fabrico's last request |
| Lead working directory | `~/.zz-lead/work`: briefs, reports | whole files on change | read on demand |
| Ownership | `~/.zz-lead/owner`: `EPOCH HOST AGENT SESSION` | written by handoffs and mirror passes | decides who may run the lead |
| Vendor transcript | the CLI's own file on the host that wrote it | optionally, lazily, for the lens | never |
| Worker panes and state | each host's daemon | nothing to move | queried live: `capture-pane`, `inspect` |
| The lead's background waits | processes on the old owner | lost | re-armed with `agent-send --wait ''`, which waits without sending anything |

# The design

## The ledger contract

The `zz-lead` skill gives the ledger its shape and tells the lead to rewrite it whole, under 60
lines, after every send, every result and before it ends a turn; anything not in it may be gone
after a restart, and older detail is searched in the chat log or transcripts with `rg`, never read
whole. A restarted lead reads the ledger, looks at each open job's pane once, verifies finished
work on its host, waits again on the rest, and never resends a brief or starts a second worker
for a live job (`.agents/skills/zz-lead/SKILL.md`, v6). On the lead-gaps build a v6 lead
restarted from its checkpoint re-armed both open jobs with `agent-send --wait ''`, sent the workers
nothing, and finished green with 609k input tokens after the restart against 821k for full resume.

For Claude Code, a Stop hook (`lead-checkpoint.py stop-hook`) checks the transcript: if the lead
used a tool since the last user or notice entry and none of those calls wrote `ledger.md`, it
returns `{"decision": "block"}` with a reason, once (`stop_hook_active` lets the second stop
through). Codex has Stop hooks too; untested.

## The chat log

```json
{"ts": "2026-10-24T14:03:15Z", "role": "user", "text": "..."}
{"ts": "2026-10-24T14:03:26Z", "role": "peer", "text": "from zz: ..."}
{"ts": "2026-10-24T14:03:43Z", "role": "checkpoint"}
{"ts": "2026-10-24T14:03:47Z", "role": "lead", "text": "..."}
{"ts": "2026-10-24T14:05:02Z", "role": "notice", "text": "completed: worker j3 on ubuntu ended its turn"}
```

zz writes it, because zz is on every path into and out of the lead: the sidebar delivers
fabrico's turns, the daemon sees worker turns end, and the lens reader already turns the vendor
transcript into turns, so the lead's final reply of each turn comes from there. A `checkpoint`
line goes in whenever `ledger.md` changes. The prototype derives the same lines from a Claude
transcript (`lead-checkpoint.py chat`): Claude stores a peer message as an `isMeta` user entry
with `origin.kind: "peer"`, a background-task notice as a user entry starting
`<task-notification>`, and the lead's text in assistant entries.

fabrico's turns must reach the lead as user input in its pane, never through the peer bus. In the
handoff test a request sent with `agent-send` was filed by the lead as "a preference relayed by
the zz peer session, not fabrico", which is the skill's rule that messages from other agents carry
no user authority doing its job.

## The packet

```text
You are fabrico's lead and you were restarted: your previous process died, and with it every
wait you had running. Follow the Restarted section of your instructions.

Your ledger.md as you last wrote it:
<ledger>

The conversation since then (fabrico's last request first):
[user ts] ...
[lead ts] ...
```

`lead-checkpoint.py packet CHAT_OR_TRANSCRIPT LEDGER` builds it. Measured sizes: 1.6 KB after a
short exchange, 2.9 to 3.5 KB in the middle of the green task. The skill goes in as the system
prompt, as for any lead start.

## Ownership, mirroring and handoff

Unchanged in shape from the first version, smaller in payload:

- `~/.zz-lead/owner` holds `EPOCH HOST AGENT SESSION` on every host; only the named host runs
  the lead.
- The owner's daemon ships the chat log's appended bytes and changed files of `~/.zz-lead/work`
  to every peer, over the connection `zz --host` keeps (`load-buffer -` in up to 900 KiB chunks,
  then `run-shell "zz save-buffer -a ..."` on the peer; one load is capped at 1 MiB). Before
  writing, it reads the peer's epoch: a higher one means this host was fenced, so it stops its
  lead.
- A handoff stops the old owner if it answers and lets it push once more, writes epoch + 1 naming
  the new owner on every reachable host, builds the packet on the new owner and starts a fresh
  session there. The session id changes on every restart; the owner line records it.
- Split brain needs a handoff while the old owner is alive but unreachable. On reconnect the
  lower epoch is refused, stops its lead, and keeps its divergent chat tail as a side file the
  winner hears about in one line. Nothing is merged.

## What a restart loses

- The last mirror pass of chat (about 2 s) when the owner dies without warning.
- Whatever the lead knew but never wrote down. The ledger contract and the chat tail cover what
  the runs needed; the old transcript is still on its host to search.
- Background waits, re-armed from the ledger; a worker reply that arrived in between is read
  from the worker's pane or report.
- Workers on a dead host; the ledger marks them lost and the lead starts new ones elsewhere.

## Chat UI

The chat sits at the bottom of the flat sidebar, under a rule, and in titlebar mode at the bottom of
the sidebar's slideover, which a chat button in the title bar opens (`workspace_chat_button`, dotted
with `chat_button` in the color of what waits). The parts are in `crates/zz-ui/src/agent/chat.rs`
and the storybook's Agent › Lead chat story shows each one and both layouts:

| Chat log line | Drawn as |
|---|---|
| `user` | `ChatBubble::mine`: right, the `chat.bubble.mine` role in the accent tone, a tail on the right, the time and a `Receipt` |
| `lead` | `ChatBubble::theirs`: left, the `chat.bubble` role, a tail on the left, no sender line |
| `peer` | `ChatBubble::theirs` with a `ChatSender`: the agent's sidebar program tile and name on top |
| `notice`, `checkpoint` | nothing: workers at work show as sidebar rows above the chat |

A receipt is one tick (`Sent`: zz has it), two (`Delivered`: typed into the lead's pane) or two in
the accent (`Read`: the lead started a turn on it). `chat_seen` lists the agents a routed answer
reached under it. The bubble and its tail are one painted path, so a translucent fill does not
double where they meet; a hidden copy of the time at the end of the text keeps the last line clear
of the stamp. `chat_header` names the lead and its host, and ends in the Esc keycap in the
slideover. `ChatComposer` is one line (the `chat.composer` pill) with the lens composer's round
send button; there are no pickers, since the lead's model is a setting. Approvals use the lens's
permission card.

`chat_section` scrolls the thread and keeps a short one at the bottom; whoever owns the
`ScrollHandle` calls `scroll_to_bottom` when a message arrives. Nothing feeds it yet: the app still
needs the chat log and the sidebar split between the list and the chat.

## Settings and verbs

- `lead-agent` (`claude` or `codex`), `lead-hosts` (owner first, then standbys), `lead-lease`
  (how long a standby waits on a silent owner before an automatic handoff; off until manual moves
  have been used for a while), `lead-reset-tokens` (restart from the packet after a finished
  request once the context passes this, default 80k).
- `zz lead start|move HOST|reset|status`. `status` prints owner, epoch, session, context size and
  each peer's lag.

# How pi-durable does it

Earendil's `@earendil-works/pi-durable` (1.4.0, MIT, experimental, released with Pi 1.0 on
2026-10-01), read from `earendil-works/pi` `packages/durable`:

- It owns the loop and its own transcript, calling model APIs through `pi-ai`; it cannot host a
  Claude Code or Codex session, and a Claude subscription only works inside the `claude` binary.
- Storage is a commit log (SQLite STRICT tables or `main.jsonl`): immutable entries, task records
  with checkpoints, documents as a base plus deltas, one atomic batch per commit.
- Resume is checkpoint and replay: a tool reruns after a crash only if it is declared
  `replay: "safe"`, otherwise the model gets an `interrupted` result.
- Context is built from the newest head marker onward; compaction appends a summary entry instead
  of deleting history.
- "One process owns a storage at a time; there is no cross-process locking" (`README.md:783`);
  no leases, epochs or replication.

Borrowed: head markers (the chat log's `checkpoint` lines, so restart context starts at the last
one and nothing is deleted), and never repeating a side effect after a crash without checking it
is safe (the restarted lead checks live state before it sends anything). Added: ownership across
hosts and a restart that does not depend on the vendor's transcript at all.

# Sync options compared

With the restart state at a few KB, the transport hardly matters; the rule is still known formats
and no new protocol.

| Option | Over zz's link? | Writer handoff | Two writers | Adds |
|---|---|---|---|---|
| **Files by offset or checksum, epoch fence** (recommended) | yes: buffers plus `run-shell` | epoch + 1 on every reachable host | lower epoch refused | nothing |
| cr-sqlite | changes are rows | multi-writer | per-column last writer wins | an extension; upstream dormant since Jan 2024 |
| Litestream 0.5 | SFTP replica wants a key file | manual restore | unsupported | a Go sidecar, one replica per database |
| LiteFS | no: FUSE plus its own port | Consul lease | blocked by the lease | Linux only; frozen since Apr 2025 |
| SQLite session changesets | yes: bytes | pair with an epoch | conflict handler per row | rusqlite `session` feature |
| sqlite3_rsync, Turso sync | no: system ssh or a hub server | none, or a hub | last push wins | a binary or a server |

Plain ssh between fleet hosts may have no keys (ubuntu to xps is refused); `zz --host` uses zz's
own. A SQLite index of the chat for the sidebar can be rebuilt on each host from the mirrored log
and never needs replicating.

# Facts verified on the fleet

- Claude Code resumes a transcript copied to another host's `~/.claude/projects/<cwd folder>/`
  (ubuntu to xps, and to the Mac with paths rewritten), and accepts `--resume /absolute/path.jsonl`.
- Codex resumes a rollout copied into the same `~/.codex/sessions/Y/M/D/` path, with no state
  database row; its "open in another app" lock (openai/codex#46652) is per machine.
- `zz --host H load-buffer -b x - < file` then `zz --host H run-shell "zz save-buffer [-a] -b x
  PATH"` moved 289 KB in 34 ms; `save-buffer PATH` sent through `--host` writes on the caller.
- `zz --host H run-shell CMD` runs there with no pane and mirrors the exit status; no stdin.

# Prototypes

`scripts/lead-checkpoint.py` has three modes: `chat` (the chat log from a Claude transcript),
`packet` (from a transcript or a chat log, identical output), and `stop-hook`.
`scripts/lead-sync.sh` keeps `~/.zz-lead` (owner, chat log, work directory, skill) mirrored and
moves the lead. Run on 2026-10-24:

1. `lead-sync.sh start ubuntu` started a Claude lead in session `lead`. A request through
   `agent-send --wait` had it record a preference and look up xps's commit; it replied
   `535a2789` and wrote its ledger.
2. The mirror put the 700-byte ledger and a 799-byte chat log on xps.
3. After the ubuntu session was killed, `lead-sync.sh handoff xps` took 0.5 s: epoch 2, a
   1.6 KB packet (the transcript it replaced was over 400 KB), a fresh session on xps.
4. The new lead knew the hash, the preference and that it came from a peer, not fabrico.

The first version's transcript mirror and fencing test (ubuntu to xps, stale owner refused with
exit 3) carried over to this version unchanged.

# Open questions

- `lead-reset-tokens`: 80k is twice the fresh floor; the runs did not test deliberate resets.
- Codex leads: the chat log needs the lens's Codex reader (`item_completed` agent and user
  messages), and the Stop hook needs Codex's hook format; neither was run.
- Folder trust: a lead started on a new host asks to trust `~/.zz-lead/work` there, once per host
  and CLI. `zz lead start` could create the folder everywhere and ask up front.
- Never mirror `~/.claude` or `~/.codex` wholesale: they hold credentials.

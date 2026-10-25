---
type: Design Plan
title: Durable driver chat . one Driver Agent, any machine
description: "How the Driver Agent (the lead until 2026-10-25) survives restarts and the loss of its host without replaying its history: every host keeps zz's copy of the driver's stream and a record of where it ran, a new driver asks every reachable host with zz driver state and starts from the newest copy as a fresh Claude Code or Codex session given a 2-4 KB packet, and two rules (follow a live driver, start only when a majority of hosts answered) keep it to one. The vendor transcript stays where it was written. Measured against full transcript resume with mid-task kills on the fleet; prototypes in scripts/driver-checkpoint.py and scripts/driver-sync.sh."
status: Proposed (2026-10-25; recovery by asking every host replaces the owner file and mirror of 2026-10-24; checkpoint resume measured, skill v6 shipped, prototypes verified on the fleet; nothing in the daemon yet)
resource: scripts/driver-checkpoint.py
tags:
- agent
- driver
- orchestrator
- multi-host
- ssh
- durability
- design-plan
timestamp: 2026-10-25T12:00:00-03:00
---

# Overview

zz runs one Driver Agent for all of fabrico's work: a plain Claude Code or Codex session with the
`zz-driver` skill, on any host of the fleet, with its chat in the command palette's Driver mode. It
must survive its own restarts and its host's death: it dies on ubuntu and comes back on xps, from
whatever hosts still answer. It was called the lead until 2026-10-25; the bench's results files and
the `lead-gaps` build keep that name.

**Recommendation.** Do not resume the vendor session, and do not replicate it. A restarted driver
is a fresh CLI session whose first prompt is a checkpoint packet of a few KB:

1. **A ledger the driver owns.** `ledger.md` in the driver's working directory, with a fixed shape
   (goal, jobs, decisions, what waits on fabrico, next), rewritten whole before every turn ends.
   The skill asks for it and a Stop hook refuses to end a turn that acted without rewriting it.
2. **zz's copy of the driver's stream.** One JSON line per thing fabrico would see: fabrico's turns, the
   driver's final reply each turn, peer messages, worker notices, and each ledger rewrite. Every
   host that follows the driver keeps one, not only the host running it. A few KB where the
   transcript is hundreds of KB or more.
3. **A packet.** The last ledger plus the chat after it plus fabrico's last request: 1.6 to 3.5 KB
   in every measured run. The driver then looks at each open job's pane once and waits again; zz
   already holds the live state of every worker.

The vendor transcript stays on the host that wrote it, for audit and for the lens's full history
view; nothing needs it to resume. Recovery asks instead of mirroring: with no driver running, zz
runs `zz driver state` on every reachable host, follows a driver that is still alive, and otherwise
starts one from the newest copy, ordered by generation and position, never by clock. There is no
owner file, lease or consensus service; one rule, start only after a majority of fleet hosts
answered, keeps the two sides of a network split from each starting a driver.

Restarts from the packet happen on a crash, on a move to another host, and on purpose once a
finished request has left the driver's context well above the fresh floor (about 38k tokens), so the
driver stays small all the time instead of only after accidents.

# Why not resume the transcript

`claude --resume` sends the whole conversation back to the model, every tool call and worker
reply, back to the last compaction. Measured on 2026-10-24 with `claude -p --resume PATH
--fork-session` on old driver transcripts, one question each, cache cold (hours later), costs as
Claude Code reports them (`results/2026-10-24-resume-cost-probe.tsv` in `bench/agents/driver`):

| Transcript | First request | Seconds | Cost |
|---|---|---|---|
| 411 KB | 45k tokens | 7.1 | $0.33 |
| 640 KB | 56k | 3.2 | $1.03 |
| 884 KB | 84k | 5.5 | $2.18 |
| 1.8 MB | 202k | 10.2 | $43.06 |
| Fresh session with skill and a 3.4 KB packet | about 40k (31k of it the prefix every Claude Code session shares, cached) | 3.3 | $0.08 |

A single driver task already reached 40k to 200k tokens in the driver benchmark; a driver that runs for
days resumes into the expensive rows, close enough to the window that Claude auto-compacts in a
hurry. Shipping that transcript between hosts also means megabytes for data the model mostly no
longer needs.

# Measured: kill mid-task, restart two ways

`bench/agents/driver` gained a resume mode (`run-one.sh v5@full|v5@checkpoint g`): the driver (skill
v5, Stop hook on in both arms) works the three-host green task, and 30 s after its third
`agent-send` the harness kills its whole process tree the way a dead host would. It then starts
either `claude --resume <id>` or a fresh session given `driver-checkpoint.py packet`, immediately or
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
- One checkpoint run spent 1.6M tokens, and it bought something: the restarted driver checked a
  Codex worker's DONE on the host, found main untouched because a parallel run had overwritten
  the brief file both used, re-briefed it under a unique path, and finished green.
- The Stop hook fired 10 times across the nine runs, once or twice each, and every time the
  driver's next tool call rewrote the ledger.

# What has to survive

| Piece | Where it lives | On other hosts | Used to restart |
|---|---|---|---|
| Ledger | `ledger.md` in the driver's working directory, rewritten by the driver every turn | in every stream copy: each rewrite is a whole-file write the stream carries | the last one |
| Stream copy | `<data dir>/zz/driver/stream.jsonl`, appended by every host that follows the driver | yes, on each follower | everything after the last ledger write, plus fabrico's last request |
| Record | `<data dir>/zz/driver/record.json`: generation, host, CLI, model, session | the newest each host has seen | which CLI to start, and the next generation |
| Driver working directory | `<data dir>/zz/driver/work` on the host running it: briefs, reports | no | read on demand; the ledger names what was there |
| Vendor transcript | the CLI's own file on the host that wrote it | no | never; the lens opens it for full history |
| Worker panes and state | each host's daemon | nothing to move | queried live: `capture-pane`, `inspect` |
| The driver's background waits | processes on the old host | lost | re-armed with `agent-send --wait ''`, which waits without sending anything |

# The design

## The ledger contract

The `zz-driver` skill gives the ledger its shape and tells the driver to rewrite it whole, under 60
lines, after every send, every result and before it ends a turn; anything not in it may be gone
after a restart, and older detail is searched in the chat log or transcripts with `rg`, never read
whole. A restarted driver reads the ledger, looks at each open job's pane once, verifies finished
work on its host, waits again on the rest, and never resends a brief or starts a second worker
for a live job (`.agents/skills/zz-driver/SKILL.md`, v6). On the lead-gaps build a v6 driver
restarted from its checkpoint re-armed both open jobs with `agent-send --wait ''`, sent the workers
nothing, and finished green with 609k input tokens after the restart against 821k for full resume.

For Claude Code, a Stop hook (`driver-checkpoint.py stop-hook`) checks the transcript: if the driver
used a tool since the last user or notice entry and none of those calls wrote `ledger.md`, it
returns `{"decision": "block"}` with a reason, once (`stop_hook_active` lets the second stop
through). Codex has Stop hooks too; untested.

## The stream copy

```json
{"gen": 3, "seq": 801, "ts": "2026-10-25T14:03:15Z", "role": "user", "text": "..."}
{"gen": 3, "seq": 804, "ts": "2026-10-25T14:03:26Z", "role": "peer", "text": "from zz: ..."}
{"gen": 3, "seq": 809, "ts": "2026-10-25T14:03:43Z", "role": "ledger", "text": "# Goal\n..."}
{"gen": 3, "seq": 812, "ts": "2026-10-25T14:03:47Z", "role": "driver", "text": "..."}
{"gen": 3, "seq": 830, "ts": "2026-10-25T14:05:02Z", "role": "notice", "text": "completed: worker j3 on ubuntu ended its turn"}
```

The lines come from the stream the daemon already sends for the driver's pane (`AgentReplay`, then
`AgentUpdates`), reduced the way `driver_messages` reduces it for the chat, so nothing new is asked
of the CLI. `seq` is the stream's own position and `gen` the driver's generation (see recovery
below); `ts` is when the follower received the line and is only for reading, since the stream
carries no times and clocks are never compared. A `ledger` line is the whole file: the skill
rewrites `ledger.md` whole, so every rewrite passes through the stream as a Write, and the last one
is the checkpoint the packet starts from.

Every always-on host keeps a copy by following the driver the way the desktop app does
(`MuxClient::follow_driver`: replay from its own cursor, then updates), and the desktop app writes
its copy too while it runs. A follower that loses the link asks again from its cursor. The
prototype derives the same lines from a Claude transcript (`driver-checkpoint.py chat`): Claude
stores a peer message as an `isMeta` user entry with `origin.kind: "peer"`, a background-task
notice as a user entry starting `<task-notification>`, and the driver's text in assistant entries.

fabrico's turns must reach the driver as user input in its pane, never through the peer bus. In the
handoff test a request sent with `agent-send` was filed by the driver as "a preference relayed by
the zz peer session, not fabrico", which is the skill's rule that messages from other agents carry
no user authority doing its job.

## The record

```json
{"generation": 3, "host": "ubuntu", "pane": "%4", "provider": "claude", "model": "opus",
 "effort": "xhigh", "session": "4f2c…", "transcript": "/home/f/.claude/projects/…/4f2c….jsonl",
 "cwd": "/home/f/.local/share/zz/driver/work"}
```

While the driver runs, its host's daemon already knows its CLI and session: the provider from the
pane's process (`AgentProvider`), Claude's session id and transcript from Claude's own registry of
running sessions (`Claude::locate` in `zz-daemon/src/agent/lens.rs`), and Codex's from the rollout
its process holds open or the newest one (`Codex::locate`). Once the process is dead there is
nothing left to look up, so the host writes the record when it first finds the session, and every
follower copies it with the stream. The provider is a string (`AgentProvider::as_str`): a new
provider is a new lens reader in the daemon, not a new format. The packet is built from zz's copy,
not from the vendor transcript, so a Claude driver can come back as a Codex driver.

## The packet

```text
You are fabrico's driver and you were restarted: your previous process died, and with it every
wait you had running. Follow the Restarted section of your instructions.

Your ledger.md as you last wrote it:
<ledger>

The conversation since then (fabrico's last request first):
[user ts] ...
[driver ts] ...
```

`driver-checkpoint.py packet CHAT_OR_TRANSCRIPT LEDGER` builds it. Measured sizes: 1.6 KB after a
short exchange, 2.9 to 3.5 KB in the middle of the green task. The skill goes in as the system
prompt, as for any driver start. In the design zz builds it from the stream copy: the last `ledger`
line, then every line after it.

## Recovery: ask every host

`zz driver state` prints what this host knows: the record, then its stream copy, one JSON line per
entry. With `--brief` it prints one line:

```json
{"host": "ubuntu", "generation": 3, "seq": 812, "alive": false, "pane": "%4", "provider": "claude"}
```

It is read-only, so `zz --host all driver state --brief` asks every configured host at once
through the fan-out the CLI already has. When zz finds no driver (no `driver` session with an agent
in it on any connected host), it:

1. asks every host with `zz --host all driver state --brief`;
2. follows a driver any host reports alive, and starts nothing;
3. with none alive and a majority of fleet hosts answering, takes the copy with the highest
   `(generation, seq)`, fetches it whole from that host, builds the packet and starts a fresh
   driver at generation + 1;
4. with fewer answers, starts nothing and says the driver is unreachable, with a button that
   starts one anyway.

Generation is chosen at each start as the highest any host reported, plus one; `seq` orders the
lines within one generation. The majority is what keeps one driver: the two sides of a network
split each see no live driver, and only one side can hear from most hosts. With the Mac, ubuntu,
xps and alienware that is 3 of 4, and a sleeping laptop counts as not answering, which the button
covers when you know better.

A driver that was cut off rather than dead can come back after a newer generation started. A host
that sees two live drivers keeps the higher generation: the lower one's host stops it and keeps
its copy, and the new driver hears about it in one line. Nothing is merged. Until it reconnects,
the old driver can still type into workers it reaches; refusing `agent-send` from an older
generation in the daemon would close that window and is not designed yet.

This replaces the owner file, epoch and 2 s mirror of the 2026-10-24 version, which
`driver-sync.sh` still implements.

## Where it lives on disk

Each host keeps it under zz's own data folder, `platform_data_dir()` in
`zz-protocol/src/app_identity.rs`: `~/.local/share/zz` on Linux (or `$XDG_DATA_HOME/zz`),
`~/Library/Application Support/zz` on macOS, `zz-dev` in place of `zz` for dev builds. That folder
already holds zz's ssh key, `window-state.json` and the browser's data; settings you edit stay in
`zz/config`.

```text
<data dir>/zz/driver/
  record.json     the newest driver this host knows
  stream.jsonl    this host's copy of the driver's stream
  work/           the driver's working directory on the host running it: ledger.md, briefs, reports
```

The driver starts in `work/`. Today it starts in the home folder, so its `ledger.md` lands in `~`.

## What a restart loses

- Lines no follower received before the driver's host died: followers read the stream as it is
  written, so this is what was in flight on the link.
- Whatever the driver knew but never wrote down. The ledger contract and the chat tail cover what
  the runs needed; the old transcript is still on its host to search.
- Background waits, re-armed from the ledger; a worker reply that arrived in between is read
  from the worker's pane or report.
- Workers on a dead host; the ledger marks them lost and the driver starts new ones elsewhere.
- The working directory on a dead host: briefs and reports there are gone, the ledger says what
  they were, and each worker still has its own brief.

## Chat UI

The chat lives in the command palette's Driver mode: a `PalettePill::Program` for the driver in the
input, `driver_palette_body` under it (the thread, at most 360px and scrolling, then the approval
waiting on you, or the error of a message that did not go through) and `DRIVER_PALETTE_HINTS` in the
footer. The sidebar keeps only the driver's last word under its list (`chat_latest`): the driver's
header, then its last answer cut to four lines (`ChatBubble::lines`) or `ChatTyping` while it
works, with a dot in the header while an approval waits. A click on it opens Driver mode, as `>` in an
empty palette does, and in titlebar mode the title bar's chat button (`workspace_chat_button`,
dotted with `chat_button` in the color of what waits). The parts are in
`crates/zz-ui/src/agent/chat.rs` and the storybook's Agent › Driver chat story shows each one:

| Chat log line | Drawn as |
|---|---|
| `user` | `ChatBubble::mine`: right, the `chat.bubble.mine` role in the accent tone, a tail on the right, the time and a `Receipt` |
| `driver` | `ChatBubble::theirs`: left, the `chat.bubble` role, a tail on the left, no sender line |
| `peer` | `ChatBubble::theirs` with a `ChatSender`: the agent's sidebar program tile and name on top |
| `notice`, `checkpoint` | nothing: workers at work show as sidebar rows above the chat |

A receipt is one tick (`Sent`: zz has it), two (`Delivered`: typed into the driver's pane) or two in
the accent (`Read`: the driver started a turn on it). `chat_seen` lists the agents a routed answer
reached under it. The bubble and its tail are one painted path, so a translucent fill does not
double where they meet; a hidden copy of the time at the end of the text keeps the last line clear
of the stamp. `chat_header` names the driver and its host. `ChatComposer` is one line (the `chat.composer` pill) with the lens composer's round
send button; there are no pickers, since the driver's model is a setting. Approvals use the lens's
permission card.

While the driver works, `ChatTyping` takes the place of its answer at the bottom of the thread: a
`chat.bubble` on its side with three dots that rise in turn (the kit's repeating animation, 30 fps)
and its last three steps, the newest beside the dots and the ones before it muted with a tick. With
no step yet it holds only the dots. The words are the ones the driver's CLI already writes, the way
other chat agents show progress: Slack assistants set a status with rotating loading messages,
Linear agents post ephemeral thought and action activities that the next one replaces, and Claude
Code's own spinner shows the todo item in progress. `driver_steps` reads them from the live turn:

- the item the driver's plan has in progress (Claude's todo `activeForm`, "Committing the changes");
- per tool since the last message the driver got, the `description` its input carries when it has
  one (Claude writes one for every Bash call, "Show dirty files", and for every agent it starts),
  else the kind and what it touches: "Reading driver.rs", "Editing chat.rs", "Searching for pinned",
  "Running git status";
- a reasoning summary's bold title, as Codex opens one with `**Checking the tests**` when summaries
  are on (they are empty in the default Codex rollouts on the fleet, so Codex steps mostly come from
  its tools).

The same step twice in a row shows once. Nothing new is asked of the driver and nothing goes on the
wire.

The palette's thread scrolls and fades under the input while there is more above (the kit's
`top_fade`, the mirror of the list's `bottom_fade`); Driver mode calls `scroll_to_bottom` when a
message arrives. A full chat at the bottom of the sidebar, with a split you dragged, came first and
went on 2026-10-24: it crowded the list and the composer fought the panes for focus.

The desktop app feeds it from the driver's pane today, through the lens stream, keeping nothing on
disk yet (`crates/zz/src/agent/driver.rs`):

- **Which pane.** While `driver-agent` is on, the driver is the first terminal running Claude Code or
  Codex in a session named `driver`, on the first host in fleet order that has one. With no `driver`
  session on any connected host, zz starts one on this machine, in the home folder, running
  `driver_command`: `claude` with `--model` and `--effort`, or `codex` with
  `-c model_reasoning_effort=` (Codex has no `max`, so it gets `xhigh`). It waits until this
  machine's daemon has sent its sessions, and 30 s between starts, so a CLI that exits at once does
  not loop. A `driver` session without an agent in it is left alone. Off, zz neither starts nor
  follows a driver and the chat's surfaces hide; a driver already running keeps running. Known
  gap: zz drops a remote host's session list while it reconnects to it
  (`MuxClient::ensure_connected`), and `may_start_driver` skips hosts it has no list for, so a
  driver on a host whose link just dropped looks missing and zz starts a second one here; the same
  happens at launch before the ssh connections are up. The recovery rules above replace this start
  rule.
- **The feed.** `MuxClient::follow_driver` asks the driver's host for an `AgentReplay` and keeps a
  cursor of its own, apart from the attached session's lenses, taking `AgentUpdates`,
  `AgentState` and lag notices from the attached connection or a background one. The daemon
  keeps sending a pane's stream to any client that asked for its replay, attached or not, so the
  chat stays live while you work on another host or window; a host whose daemon predates that
  sends only the replay. A new connection to the driver's host asks again from the cursor.
- **The messages.** A driver-only `AgentController` reduces the stream, and `driver_messages` keeps
  your turns, the driver's last answer of each finished turn, and the `[zz] Agent %N finished:`
  notices zz types into the driver, which show as that agent's message under its window name and
  program tile. Your turn reads two ticks once it is in the transcript and two in the accent once
  the driver did anything after it. Messages that arrive live get the local time; the history
  replayed when zz starts following gets none, since the stream carries no timestamps.
- **Talking back.** Driver mode's input types into the driver's pane like the lens composer, and
  queues behind a running turn. The oldest approval waiting in the driver shows as the lens's
  permission card under the thread; a digit in an empty field picks an option.
- **Where.** Driver mode, a view of its own over the panes (`agent/driver_palette.rs`), opens in either
  layout: `>` typed into an empty command palette switches to it while there is a driver (the
  palette's footer then shows the key), a click on the driver's last message in the sidebar opens it
  (`SidebarOpenDriver`), and in titlebar mode the title bar's chat button opens it too. Esc closes it
  and Backspace in an empty field goes back to the command palette.

Not yet: the web and iPad clients, the stream copies, the record and `zz driver state`, peer-bus messages (they arrive as plain
user turns and show as yours), and an unwatch message, so a client keeps getting the stream of
every pane it ever asked to replay until it disconnects.

## Settings and verbs

- Shipped 2026-10-24 under Settings › Agents (as `lead-*`, renamed before any release on
  2026-10-25): `driver-agent` (on or off), `driver-model` (`claude` for Claude Code's own default,
  `opus`, `sonnet`, `fable`, or `codex` for Codex's own default) and `driver-effort` (`default`,
  `low`, `medium`, `high`, `xhigh`, `max`). The driver starts on this machine only; a change of
  model or effort applies at its next start.
- Still proposed: `driver-hosts` (where a new driver starts: the first of these that answers,
  this machine by default) and `driver-reset-tokens` (restart from the packet after a finished
  request once the context passes this, default 80k).
- `zz driver state [--brief]` as above, and `zz driver start|move HOST|reset|status`. `status`
  prints the record, the context size and how far each host's copy trails.

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

Borrowed: head markers (the stream copy's `ledger` lines, so restart context starts at the last
one and nothing is deleted), and never repeating a side effect after a crash without checking it
is safe (the restarted driver checks live state before it sends anything). Added: one driver across
hosts and a restart that does not depend on the vendor's transcript at all.

# Other systems looked at

Read on 2026-10-25:

- **durable-harness** (`vercel-labs/durable-harness`, 0.0.x, Apache-2.0): runs Claude Code
  through the Agent SDK in a sandbox VM per session, puts every transcript line and every tool call
  in a Workflow event log before the call runs, and after a crash resumes with `--resume` from the
  logged transcript plus a recovery note. To make that hold it turns off subagents, background
  shells, worktrees, cron and the task list, and it bills an API key, not a subscription. Borrowed:
  the recovery note. Not borrowed: owning the CLI's loop, which costs the features that make the
  real CLI worth running in zz.
- **celld** (`denoland/celld`, Rust): one owner per cell through a conditional write to an
  S3-style bucket, with an epoch; each write goes to one or two peers before the bucket; it
  "needs no membership protocol, failure detector, or consensus service". zz has no bucket, so the
  majority of answers stands in for the conditional write.
- **Raft** (openraft 0.10 alpha, raft-rs 0.7): would elect one host and fence the old one even
  while it is cut off, but nothing runs without a majority of voters, every election restarts the
  driver, and the daemons would need a link of their own. Not needed while asking every host is
  enough; worth another look if zz grows other things that must exist once across the fleet (a
  job queue, locks on a repository across hosts).

# Storage and sync options compared

Compared on 2026-10-24 for the mirror this design replaced. With the restart state at a few KB,
the transport hardly matters; the rule is still known formats and no new protocol.

| Option | Over zz's link? | Writer handoff | Two writers | Adds |
|---|---|---|---|---|
| **Followers append the driver's stream** (recommended since 2026-10-25) | yes: the stream zz already sends | generation + 1 at each start | the lower generation stops | nothing |
| Files by offset or checksum, epoch fence (2026-10-24) | yes: buffers plus `run-shell` | epoch + 1 on every reachable host | lower epoch refused | an owner file and a mirror loop |
| cr-sqlite | changes are rows | multi-writer | per-column last writer wins | an extension; upstream dormant since Jan 2024 |
| Litestream 0.5 | SFTP replica wants a key file | manual restore | unsupported | a Go sidecar, one replica per database |
| LiteFS | no: FUSE plus its own port | Consul lease | blocked by the lease | Linux only; frozen since Apr 2025 |
| SQLite session changesets | yes: bytes | pair with an epoch | conflict handler per row | rusqlite `session` feature |
| sqlite3_rsync, Turso sync | no: system ssh or a hub server | none, or a hub | last push wins | a binary or a server |

Plain ssh between fleet hosts may have no keys (ubuntu to xps is refused); `zz --host` uses zz's
own. A SQLite index of the chat for the sidebar can be rebuilt on each host from its stream copy
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

`scripts/driver-checkpoint.py` has three modes: `chat` (the chat log from a Claude transcript),
`packet` (from a transcript or a chat log, identical output), and `stop-hook`.
`scripts/driver-sync.sh` keeps `~/.zz-driver` (owner, chat log, work directory, skill) mirrored and
moves the driver: the 2026-10-24 version of this design, not the recovery above. Run on 2026-10-24,
when they were `lead-sync.sh` and `lead-checkpoint.py` and the session was `lead`:

1. `driver-sync.sh start ubuntu` started a Claude driver in session `driver`. A request through
   `agent-send --wait` had it record a preference and look up xps's commit; it replied
   `535a2789` and wrote its ledger.
2. The mirror put the 700-byte ledger and a 799-byte chat log on xps.
3. After the ubuntu session was killed, `driver-sync.sh handoff xps` took 0.5 s: epoch 2, a
   1.6 KB packet (the transcript it replaced was over 400 KB), a fresh session on xps.
4. The new driver knew the hash, the preference and that it came from a peer, not fabrico.

The first version's transcript mirror and fencing test (ubuntu to xps, stale owner refused with
exit 3) carried over to this version unchanged.

# Open questions

- `driver-reset-tokens`: 80k is twice the fresh floor; the runs did not test deliberate resets.
- Codex drivers: the stream copy needs the lens's Codex reader (`item_completed` agent and user
  messages), and the Stop hook needs Codex's hook format; neither was run.
- Who follows: each host's daemon (a daemon-to-daemon stream, which zz does not have yet) or a
  `zz driver follow` process per host riding `zz --host`, which needs nothing new.
- Fencing by generation: a daemon refusing `agent-send` from an older generation would stop a
  cut-off driver from typing into workers before it reconnects.
- Folder trust: a driver started on a new host asks to trust `<data dir>/zz/driver/work` there,
  once per host and CLI. `zz driver start` could create the folder everywhere and ask up front.
- Never copy `~/.claude` or `~/.codex` wholesale: they hold credentials.

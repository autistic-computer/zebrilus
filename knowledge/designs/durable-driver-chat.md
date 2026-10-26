---
type: Design Plan
title: Durable driver chat . one Driver Agent, any machine
description: "How the Driver Agent (the lead until 2026-10-25) survives restarts and the loss of its host without replaying its history: every host keeps zz's copy of the driver's stream and a record of where it ran, a new driver asks every reachable host with zz driver state and starts from the newest copy as a fresh Claude Code or Codex session given a 2-4 KB packet, and two rules (follow a live driver, start only when a majority of hosts answered) keep it to one. The vendor transcript stays where it was written. Measured against full transcript resume with mid-task kills on the fleet; prototypes in scripts/driver-checkpoint.py and scripts/driver-sync.sh."
status: Built (2026-10-25; zz driver state, keep, start, recover and stop-hook, keepers on every host, the older of two live drivers standing down, the generation fence, driver-hosts, the sidebar's Start here and Codex drivers with their Stop hook, verified with zz-dev on xps, alienware and the Mac; the web and iPad clients do not keep or recover the driver)
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

`zz driver keep` writes them (`crates/zz-cli/src/driver.rs`). On the host running the driver it
attaches to its own daemon as a control client, asks for the driver pane's agent stream
(`AgentReplay` from 0, then `AgentUpdates`, the same stream the desktop app follows) and reduces it
with `DriverLog` (`crates/zz-client/src/driver_log.rs`), which runs the stream through the lens's
`AgentTranscript` and keeps your turns, zz's `[zz]` notices, each write of `ledger.md`, and the
driver's last answer of each finished turn. A ledger write is any finished tool call that writes the
file: Claude's Write or Edit, Codex's `fileChange`, or a shell command that redirects, tees, copies
or moves onto it. The keeper takes the line's text from `ledger.md` in the driver's folder rather
than from the call, since an Edit or a patch carries only the changed lines, and holds every later
line until a running write finishes, so the file it reads is the one that write left. Whether a turn
still runs comes from the lens's `Activity` items (Claude's session registry, Codex's
`task_started` and `task_complete`), not from the pane's agent state, which stays `ready` for a pane
the lens only watches; an answer is kept only once its turn ended. Nothing new is asked of the CLI
or the daemon. The daemon's own sequence numbers restart when a lens reopens and are not stable per
line, so `seq` is the keeper's own count within the generation, and a session switch inside one
generation (`/clear`) moves `session_base` in the record so the new session's lines follow the old
ones; `gen` is the driver's generation (see recovery below). `ts` is when the keeper wrote the line
and is only for reading; clocks are never compared. The last `ledger` line is the checkpoint the
packet starts from, and the packet itself, the first prompt of a restarted driver, is never a line.

Every other host runs a keeper too, and there it follows: it asks every host which one runs a live
driver, runs `zz --host THAT driver state --follow --since GEN:SEQ` and appends what arrives. The
host it follows sends an `{"alive": …}` line every 10 s; a keeper that hears `false`, or nothing
for 45 s, or finds a driver on its own host, stops following and asks again. The prototype derives
the same lines from a Claude transcript (`driver-checkpoint.py chat`): Claude stores a peer message
as an `isMeta` user entry with `origin.kind: "peer"`, a background-task notice as a user entry
starting `<task-notification>`, and the driver's text in assistant entries.

fabrico's turns must reach the driver as user input in its pane, never through the peer bus. In the
handoff test a request sent with `agent-send` was filed by the driver as "a preference relayed by
the zz peer session, not fabrico", which is the skill's rule that messages from other agents carry
no user authority doing its job.

## The record

```json
{"generation": 2, "host": "alienware", "pane": "%0", "provider": "claude",
 "command": "claude --model sonnet", "session": "f1e89bf7-…", "session_base": 0,
 "cwd": "/home/f/.local/share/zz/driver/work"}
```

`zz driver start` writes it with the generation, the command it started and the working directory;
the keeper adds the pane, the provider (the pane's foreground program, `claude` or `codex`) and the
session id the lens reports, which it can only learn while the process lives. Followers copy it
with the stream. `command` keeps the model and effort as the CLI's own flags. The provider is a
plain string: a new provider is a new lens reader in the daemon, not a new format. The packet is
built from zz's copy, not from the vendor transcript, so a Claude driver can come back as a Codex
driver. A keeper that finds a driver this host did not start through `zz driver start` (another
pane, or a record from another host) gives it the next generation.

## The packet

```text
You are the Driver Agent and you were restarted: your previous process died, and with it every
wait you had running. Follow the Restarted section of your instructions.

Your ledger.md as you last wrote it:

<ledger>

The conversation since then, the user's last request first:

[user ts] ...
[driver ts] ...
```

`driver_log::packet` builds it from the stream copy: the last `ledger` line, then every line after
it, with your last request on top only when it came before that ledger; it stays under 12 KB by
dropping the oldest lines after the ledger. `zz driver start` also writes the ledger back to
`work/ledger.md`, where the skill's Restarted section reads it. The prototype's
`driver-checkpoint.py packet` measured 1.6 KB after a short exchange and 2.9 to 3.5 KB in the
middle of the green task.

## Starting a driver

`zz driver start` (and `recover`, which calls it) runs on the host the driver will run on:

- writes the skill without its frontmatter as `work/DRIVER.md`, passed to Claude with
  `--append-system-prompt-file`, and as `work/AGENTS.md`, which Codex reads from its working
  directory;
- for Claude, writes `work/.driver-settings.json` with a Stop hook that runs `zz driver stop-hook`
  (the prototype's check, in the CLI: a turn that used a tool and wrote no `ledger.md` is sent back
  once), passed with `--settings`, so nothing in `~/.claude` changes for it;
- for Codex, passes the same Stop hook inline (`-c 'hooks.Stop=[{hooks=[{type="command",…}]}]'`)
  with its trust record next to it (`-c 'hooks.state={"/<session-flags>/config.toml:stop:0:0"=
  {trusted_hash="sha256:…"}}'`, the SHA-256 of the hook's identity as Codex 0.164 computes it), so
  Codex runs it without a `/hooks` review, without `--dangerously-bypass-hook-trust` and without
  touching `~/.codex`. The trust format is undocumented. `zz driver stop-hook` reads Codex's rollout
  too: the turn starts at its `task_started` line, and a `function_call`, `custom_tool_call` or
  `local_shell_call` after it counts as acting. Any `-c` makes Codex skip its shared background
  server and say so in one startup warning;
- marks the driver folder trusted in `~/.claude.json` (`projects[…].hasTrustDialogAccepted`, the
  documented way; trust on a folder outside a git repository covers its subfolders), since an
  interactive Claude in an untrusted folder stops at the trust dialog and runs no hooks; Codex asks
  only inside a git repository, and the folder is in none;
- takes `PATH` from the login shell before it starts the CLI, because a daemon started from the
  desktop app or a non-login ssh has no `~/.local/bin` on its `PATH`, and puts the pane's tmux
  shim folder (`ZZ_TMUX_SHIM_DIR`) back in front, so the driver's `zz` is the daemon's own binary
  and not an older one on the login `PATH` (2026-10-26: under zz Dev the release `zz` answered,
  spoke an older protocol, and every `zz --host` failed);
- exports `ZZ_DRIVER_GENERATION` into the driver's shell (see fencing below), refuses to start
  while a driver runs on this host, kills a `driver` session here that has no agent in it, starts
  the CLI in a new `driver` session in `work/` with the packet as its first prompt, and starts this
  host's keeper.

## Recovery: ask every host

`zz driver state` prints what this host knows: the record, then its stream copy, one JSON line per
entry. With `--brief` it prints one line:

```json
{"host": "ubuntu", "generation": 3, "seq": 812, "alive": false, "pane": "%4", "provider": "claude"}
```

It is read-only, so `zz --host all driver state --brief` asks every configured host at once
through the fan-out the CLI already has (`--follow` is refused there, since the fan-out waits for
every host to finish). `driver state` is a CLI command, like `list-hosts`: `--host H` runs H's own
zz over ssh, which reads H's files and asks H's daemon whether its driver runs. `zz driver
recover` (`driver::recover`, which the desktop app calls too):

1. asks this machine and every configured host, with `zz --host all driver state --brief`;
2. follows a driver any host reports alive, and starts nothing: `{"outcome":"alive",…}`;
3. with none alive and more than half of the hosts answering (the configured ones plus this
   machine), takes the copy with the highest `(generation, seq)`, fetches it whole from that host
   when it is not this one, and starts a driver here at the highest generation any host reported,
   plus one: `{"outcome":"started",…}`;
4. with fewer answers, starts nothing: `{"outcome":"unreachable","answered":1,"hosts":2}`, unless
   `--force`.

`--on HOSTS` (the `driver-hosts` setting in the desktop app) names where a new driver starts: the
first listed host that answered, `local` being this machine, this machine when none did. A start on
another host runs `zz --host H driver start --generation N --from BEST`, and H takes the newest copy
from BEST itself; when the newest copy is this machine's, H is told this machine's name and starts
from its own copy if it cannot reach it.

Generation is chosen at each start as the highest any host reported, plus one; `seq` orders the
lines within one generation. The majority is what keeps one driver: the two sides of a network
split each see no live driver, and only one side can hear from most hosts. With the Mac, ubuntu,
xps and alienware that is 3 of 4, and a sleeping laptop counts as not answering, which the Start
here button covers when you know better.

A driver that was cut off rather than dead can come back after a newer generation started. The
keeper recording it asks every host once a minute, and when another host runs a driver of a higher
generation (the same generation goes to the host whose name sorts last), it stops its own driver,
types one `[zz]` line into the newer driver saying so and where the old one's copy is, and starts
following the newer one. The old copy keeps whatever the old driver did after the split; nothing
is merged. `recover` and every follower pick the live driver of the highest generation.

Until it stops, an old driver could still change things on hosts it reaches. So the driver's shell
carries `ZZ_DRIVER_GENERATION`, and every `zz` command that can change state (whatever `--host all`
would refuse) first asks its target host's `driver state --brief`, locally or over `--host`; when
that host knows a newer generation, zz runs nothing and says a newer Driver Agent took over. It
costs the driver one extra ssh exec per changing command on another host. Workers do not inherit
the variable, since it lives in the driver's shell and not in the session's environment.

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
  keep.lock       held by this host's keeper, so a second one exits
  work/           the driver's working directory on the host running it: ledger.md, briefs,
                  reports, DRIVER.md, AGENTS.md, .driver-settings.json, .packet
```

The driver starts in `work/`. `zz driver keep` starts the keeper as a process of its own (its own
session, no terminal) and returns, so it does not depend on a daemon job or an ssh session staying
open; `zz driver start` starts it, and the desktop app runs `run-shell -b "zz driver keep"` on each
host once per connection while `driver-agent` is on.

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

The chat lives in the command palette's Driver mode, as wide as the other modes (560px) and never
shorter than the palette with its rows full (`driver_palette_thread_least`, from
`command_palette_full_height`), so `>` keeps the palette's size: `driver_palette_body` on top (the
thread, its newest message on the input, scrolling, up to 78% of the window less the palette's
chrome and any approval card, then the approval waiting on you, or the error of a message that did
not go through), a `ChatComposer` at the bottom like a messaging app's
(`CommandPaletteSurface::input_last`: a `+` that starts an `@` mention, the field, and the round
`agent.send` button, which sends, steers while the driver works, and turns into stop while it
works and the field is empty), and the footer: where the driver runs on the left
(`driver_palette_place`: the host's status dot and name, the driver's tile; a click shows the
driver's terminal) and `driver_palette_hints` on the right. The thread opens from nothing in 280 ms (ease-out), so when Driver mode opens the input
slides from the palette's top to its bottom. The sidebar keeps only the driver's last word under its list (`chat_latest`): the driver's
header, then its last answer cut to four lines (`ChatBubble::lines`) or `ChatTyping` while it
works, with a dot in the header while an approval waits. A click on it opens Driver mode, as `>` in an
empty palette does, and in titlebar mode the title bar's chat button (`workspace_chat_button`,
dotted with `chat_button` in the color of what waits). The parts are in
`crates/zz-ui/src/agent/chat.rs` and the storybook's Agent › Driver chat story shows each one:

| Chat log line | Drawn as |
|---|---|
| `user` | `ChatBubble::mine`: right, the `chat.bubble.mine` role filled solid in the accent (prominent), a tail on the right, the time and a `Receipt` |
| `driver` | `ChatBubble::theirs`: left, the `chat.bubble` role, a tail on the left, no sender line; markdown rendered (lists, code, emphasis, tables) unless it is one paragraph of plain words |
| `peer` | `ChatBubble::theirs` with a `ChatSender`: the agent's sidebar program tile and name on top |
| `notice`, `checkpoint` | nothing: workers at work show as sidebar rows above the chat |

A receipt is one tick (`Sent`: zz has it), two (`Delivered`: typed into the driver's pane) or two in
full ink (`Read`: the driver started a turn on it; the others are muted on the accent fill). `chat_seen` lists the agents a routed answer
reached under it. A bubble is the role's plain rounded box, its corner capped at 17px (the tails
went on 2026-10-26); a hidden copy of the time at the end of the text keeps the last line clear
of the stamp. A markdown answer goes through the kit's `TextView` (the lens's code and figure
roles, half-rem paragraph gaps, headings at the body's size the way chat apps draw them),
shrinks to its widest line, and puts the stamp on a line of its own. The zz-driver skill asks the
driver to write like a message (short paragraphs, one level of bullets, bold and code, no headings,
rules or tables); the sidebar's four-line cut shows it as plain text, one line per block. `chat_header` names the driver and its host. The composer is the lens's (`agent::composer::Composer`,
2026-10-26; before, `ChatComposer` and the `chat.composer` role): an `agent.composer` pill 40px tall
with `+` (which starts an `@` mention), the field, the driver's context ring and the round send
button, 28px each and 6px from the edge; there are no pickers, since the driver's model is a
setting. Approvals use the lens's
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

The palette's thread scrolls and fades at the top while there is more above and at the bottom
while there is more below (the kit's `top_fade` and `bottom_fade`); Driver mode calls
`scroll_to_bottom` when a message arrives. The thread starts one inset below the palette's top edge:
content clips to a rectangle, so a thread flush with the edge drew into the palette's rounded
corners while it scrolled. A full chat at the bottom of the sidebar, with a split you dragged, came first and
went on 2026-10-24: it crowded the list and the composer fought the panes for focus.

The desktop app feeds it from the driver's pane today, through the lens stream, keeping nothing on
disk yet (`crates/zz/src/agent/driver.rs`):

- **Which pane.** While `driver-agent` is on, the driver is the first terminal running Claude Code or
  Codex in a session named `driver`, on the first host in fleet order that has one. With none on
  any connected host, the sidebar keeps `chat_asleep` where the driver's last message goes:
  "Driver Agent asleep" and a Wake button (`DriverChat::wake`). zz never starts a driver on its own
  (since 2026-10-26; before, it ran the recovery at every launch with no driver, so each restart of
  zz spent a packet's worth of tokens). Wake runs the recovery above (`driver::recover` on a
  background thread) with `driver_command`: `claude` with `--model` and `--effort`, or `codex` with
  `-c model_reasoning_effort=` (Codex has no `max`, so it gets `xhigh`), once this machine's daemon
  has sent its sessions. The button reads Waking until a driver shows up, at most 20 s after the
  recovery found or started one. Recovery asks every host over ssh itself, so a host zz is still
  reconnecting to (whose session list `MuxClient::ensure_connected` drops) does not look
  driverless. When too few hosts answer, the sidebar
  keeps `chat_unreachable` where the driver's last message goes: "Driver Agent not running", how
  many hosts answered, and a Start here button that runs the recovery with `--force` on this
  machine. It passes `driver-hosts` as `--on`. Off, zz neither
  starts nor follows a driver and the chat's surfaces hide; a driver already running keeps
  running. While it is on, zz starts `zz driver keep` on every connected host once per connection.
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
- **Talking back.** Driver mode's input types into the driver's pane like the lens composer. While
  the driver works, Enter steers it (`AgentSteer`, wire v115: the text goes into the running turn
  the way typing into its terminal does) and Tab queues the message behind the turn instead, the
  way Codex splits the two keys. Ctrl-C stops the driver's turn (the lens sends Esc, as you would in
  its terminal) unless the input has a selection. Up in an empty field takes your last message
  back: messages still queued come back from the daemon (`AgentUnqueue`) into the field; otherwise
  the text of the last one you sent fills it, and if the driver had not answered it yet its turn
  stops first. Claude Code's `[Request interrupted by user]` turn shows no bubble. The oldest
  approval waiting in the driver shows as the lens's permission card under the thread; a digit in an
  empty field picks an option.
- **Where.** Driver mode, a view of its own over the panes (`agent/driver_palette.rs`), opens in either
  layout: `>` typed into an empty command palette switches to it while there is a driver (the
  palette's footer then shows the key), a click on the driver's last message in the sidebar opens it
  (`SidebarOpenDriver`), in titlebar mode the title bar's chat button opens it too, and so does
  `prefix A` in the desktop app (the `open-driver-chat` chrome action in the client's `prefix`
  table, which does nothing while there is no driver). Esc closes it
  and Backspace in an empty field goes back to the command palette.
- **Out of the session lists.** While `driver-agent` is on, the desktop app leaves every session
  named `driver`, on every host, out of the places you switch sessions from: the sidebar's tree and
  flat list, the command palette's tree, the File menu's Switch Session and the tray
  (`driver::hidden_session`, applied by `MuxTreeModel::without_session`, the flat sidebar's
  `listed_summary`, `menus::update_sessions` and `tray::facts_from`). Its windows, bells, agent
  badges and needs-you counts leave the host rows with it, and so do its chimes and banners, since
  the sidebar reads those from the same tree. The session you are attached to stays in the tree and
  the flat list, so a driver you went to keeps its rows until you switch away. The CLI,
  `list-sessions`, `choose-tree`, command-line completion, the status bar and the daemon still see
  it, and so do the web and iPad clients, which have no Driver mode and no `driver-agent` setting.
  With the setting off, a session called `driver` is listed like any other.
- **Its terminal.** `WorkspaceView::show_driver_terminal` attaches to the driver's host and session
  and selects its pane (`nav::pane_activation`, which the tray's Focus Pane uses too). Driver mode's
  footer host label sends `DriverPaletteEvent::ShowTerminal` for it, and a right click on the
  driver's last message in the sidebar offers Show terminal (`SidebarShowDriverTerminal`).
- **Mentions.** `@` in the input opens the palette's target picker floating over it
  (`zz_ui::target::TargetPicker::for_composer`, made by `zz_ui::command::target_picker` over the
  desktop's palette backend, laid over the rows with `CommandPaletteSurface::over_input`, with
  only as many rows as fit above the input, `TargetPicker::fit`, so it never reaches past the
  window). The grammar, rows and keys are the command palette's `@` mode
  ([Targets](/concepts/command-palette.md#targets)): tmux's target with an optional host,
  `work:2.%7`, `xps:work`, `%7`, or a file in scp form, `~/dev/` or `xps:/etc/`, a target without a
  host meaning the host the window is attached to. Tab completes the highlighted row and opens
  what is under it, Up and Down move, Enter puts the finished mention in with a space after it,
  and Esc closes only the picker until the cursor leaves the word; those are the input's own
  actions captured around it, so Driver mode's key handler never sees them. A mention always goes
  in with its host written (`@main:` inserts `@macbook:main:1`), since the driver may run
  elsewhere: this machine goes by its hostname, the sidebar's (`sidebar::local_hostname`). The
  driver's own session is not listed while `driver-agent` is on, as in every session list. A
  session whose name tmux could not read back goes in as `$id`, a window by its index, a pane as
  `%id`, and a space in a file name as `\ `. Files come from the host that has them over its own
  connection, with any terminal there as the anchor; a daemon lists a dir that names its own
  root (absolute or `~`) through a pane in a session the client is not attached to. The message
  goes out as plain text, and the zz-driver skill tells the driver what a mention means.
- **Links** (2026-10-26). The thread draws zz links: a Markdown link whose destination is a
  target, `[cargo test](@xps:work:1.%8)` or `[atlas.rs:212](@xps:~/dev/atlas.rs:212)`, which the
  zz-driver skill asks the driver to write, and a bare `@xps:work:2` in a message, yours or its,
  which `zz_ui::link::autolink` (Markdown answers, outside code and links) and `autolink_plain`
  (your plain text, everything else escaped) turn into one when its host is written or what it
  names is there. `resolve_link` reads a link against the fleet as `ChatBubble::links` gets it
  each render (`target_hosts` over the palette backend): accent and underlined while it is there
  (your ink on your own bubbles), muted while its host is offline or connecting, struck through
  once gone. A file link takes `:line` or `:line:col`. The kit's `TextView::links` takes a
  `TextLinks` that paints each link, builds the card hovering it shows (through zz-gpui's
  `PartTooltip`, the tooltip machinery `InteractiveText` uses) and takes its clicks before the OS
  would. A click dispatches `OpenLink`, which Driver mode turns into `DriverPaletteEvent::Open`
  and the workspace into `command::palette::open_link`: a host attaches or reconnects, a session,
  window or pane is switched to as the palette's `@` mode does, and a file's path is pasted into
  the active pane, in scp form when it is on another host. The platform key with a click
  dispatches `MentionLink`, which puts the mention in the composer, and a right click copies it.

Not yet: the web and iPad clients, peer-bus messages (they arrive as plain user turns and show as
yours), and an unwatch message, so a client keeps getting the stream of
every pane it ever asked to replay until it disconnects.

## Settings and verbs

- Shipped 2026-10-24 under Settings › Agents (as `lead-*`, renamed before any release on
  2026-10-25): `driver-agent` (on or off), `driver-model` (`claude` for Claude Code's own default,
  `opus`, `sonnet`, `fable`, or `codex` for Codex's own default) and `driver-effort` (`default`,
  `low`, `medium`, `high`, `xhigh`, `max`). A change of model or effort applies at its next start.
- `driver-hosts` (2026-10-25, file-only): where a new driver starts, the first of these host names
  that answers, `local` for this machine, separated by commas or spaces; empty starts it on this
  machine. The app passes it to `recover` as `--on`.
- Built 2026-10-25: `zz driver state [--brief] [--json] [--since GEN:SEQ] [--follow]`, `keep`,
  `start [--generation N] [--from HOST] [--command CMD]`, `recover [--force] [--on HOSTS]` and
  `stop-hook` ([commands](/tmux/commands.md)).
- Still proposed: `driver-reset-tokens` (restart from the packet after a finished request once the
  context passes this, default 80k), a Settings row for `driver-hosts`, and `zz driver move
  HOST|reset|status`.

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
- `#{...}` formats print a tab as `_`, so `zz driver` parses `list-panes -F` with spaces.
- Recovery, 2026-10-25, with zz-dev builds of this tree on xps and alienware (each listing the
  other) and a Claude Sonnet driver:
  1. `zz driver recover` on xps, with no copy anywhere, started generation 1 there with no packet;
     the skill loaded (its first ledger used the skill's shape) and no trust dialog showed.
  2. Two requests ("my favorite color is teal", "the deploy window is Friday 18:00") reached both
     hosts' copies line for line: each request, each ledger rewrite and each answer.
  3. xps lost (daemon and keeper killed, its zz hidden so it could not answer): `recover` on
     alienware printed `unreachable` (1 of 2 hosts); `--force` started generation 2 from
     alienware's own copy with a 0.9 KB packet, and the new driver answered "Teal, and the deploy
     window is Friday 18:00". The packet never became a chat line.
  4. With xps back, its keeper followed alienware and caught up to generation 2; `recover` from
     the Mac printed `alive` on alienware and started nothing.
  5. alienware's driver killed with its host up: `recover` on xps started generation 4 from
     alienware's copy (a generation 3 start had died at once, before the fix that takes `PATH` from
     the login shell), and that driver answered from its packet too.
  6. A zz Dev app built from this tree, run on the Mac in a scratch home listing xps and alienware
     with `driver-agent` on: at launch it started no driver of its own while xps ran one, started a
     keeper on the Mac whose copy caught up to generation 4, and when xps's driver was killed it
     started generation 5 on the Mac from the newest copy. With both hosts unable to answer it
     started nothing (1 of 3).
- The rest, 2026-10-25, same hosts (xps's line for the Mac taken out for the run, since the Mac's
  own zz Dev was running a driver of its own by then):
  1. A Codex driver on xps (generation 7) read `AGENTS.md`, followed its Restarted section and
     showed no hook review; its turns were recorded as the request, the ledger as the file stood
     after its `fileChange`, and the final answer. On a turn told to write no file, the Stop hook
     sent it back once (a `HookPrompt` in the rollout) and it rewrote the ledger.
  2. A Claude driver on alienware (generation 8) started from that Codex driver's copy and kept
     what it had been told.
  3. Split (each host's line for the other pointed at a name that does not resolve): `recover` on
     xps said `unreachable` (1 of 2); `--force` started generation 9 there. `ZZ_DRIVER_GENERATION=8`
     was refused on xps for `new-window` and allowed for `list-windows`; 9 was allowed; the
     generation 8 driver's process carried the variable; from alienware, with its line for xps
     back, `--host xps new-window` as generation 8 was refused too.
  4. Healed: within a minute alienware's keeper stopped its generation 8 driver, xps's driver got
     the `[zz]` line and checked the old copy itself ("Generation 8 did nothing after generation 9
     started"), and alienware's copy followed generation 9.
  5. `recover --on ubuntu,xps` from the Mac skipped ubuntu (not configured there) and started
     generation 10 on xps over `--host`, from alienware's copy.
  6. The storybook's Last message section shows `chat_unreachable` in light and dark. Its Start
     here was not clicked in a running app.
- Keepers with no driver anywhere ask every host once a minute; one that follows or records checks
  every 10 s.

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
- Codex's hook trust record is undocumented; if a Codex release changes how it hashes a hook,
  the driver's Stop hook stops running (and may ask for a `/hooks` review) until zz follows.
- The fence trusts the driver's own environment, so it stops a stale driver by accident, not one
  that unsets the variable. A daemon-side check would need the generation on the wire.
- A keeper restarted over an ssh session nested in another one did not stay up once; started
  again it did. The app re-sends `zz driver keep` on every connection, which covers it, but the
  cause is not known.
- Folder trust: `zz driver start` writes the trust entry into `~/.claude.json` while Claude may
  be rewriting the same file; a lost write would show the trust dialog once.
- Never copy `~/.claude` or `~/.codex` wholesale: they hold credentials.

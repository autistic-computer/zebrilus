---
type: Design Plan
title: Durable lead chat . one lead, any machine
description: "How the lead agent's conversation and state survive restarts and move between machines: the chat is the JSONL file Claude Code or Codex already writes, one host owns the lead under an epoch, the owner's daemon ships appended bytes and the lead's working directory to every other host over the ssh link zz --host already holds, and a handoff bumps the epoch and runs the CLI's own resume on the new host. Compares pi-durable, cr-sqlite, Litestream, LiteFS, SQLite changesets, sqlite3_rsync and Turso against that, with a working prototype in scripts/lead-sync.sh."
status: Proposed (2026-10-23; prototype scripts/lead-sync.sh verified ubuntu -> xps failover and fencing; nothing in the daemon yet)
resource: scripts/lead-sync.sh
tags:
- agent
- lead
- orchestrator
- multi-host
- ssh
- durability
- design-plan
timestamp: 2026-10-23T20:00:00-03:00
---

# Overview

zz runs one lead agent for all of fabrico's work: a plain Claude Code (or Codex) session with the
`zz-lead` skill, living on an always-on host such as ubuntu, with its chat in zz's sidebar. The
lead must survive its own restarts and its host's death: it dies on ubuntu and comes back on xps
with the same chat.

**Recommendation.** Do not build a chat database. The conversation already exists as an
append-only JSONL file that the CLI writes and knows how to resume, on any machine, once the file
is in place. zz adds three small things:

1. **One owner per lead, under an epoch.** `~/.zz-lead/owner` holds `EPOCH HOST AGENT SESSION` on
   every host. Only the host named there runs the lead CLI.
2. **A mirror.** The owner's daemon ships the bytes appended to the transcript, plus the lead's
   working directory (ledger, briefs, reports), to every other host, over the ssh connection that
   `zz --host` already keeps. Every host holds a copy a few seconds old.
3. **A handoff.** Moving the lead stops the old owner if it is reachable, writes epoch + 1 on every
   reachable host, and starts the CLI's own resume (`claude --resume`, `codex resume`) in a zz pane
   on the new owner. A host that comes back holding a lower epoch stops its lead and becomes a
   follower.

That is single-writer log shipping with an epoch fence, the shape Kafka leader epochs and Raft
terms use, built only from formats that already exist: the vendor's JSONL, tmux paste buffers over
zz's link, and plain files. No CRDT, no SQLite replication, no new protocol.

# What has to survive

| Piece | Where it lives | How it moves |
|---|---|---|
| The conversation | Claude: `~/.claude/projects/<cwd folder>/<id>.jsonl` plus the `<id>/` folder beside it (tool results, subagents). Codex: `~/.codex/sessions/Y/M/D/rollout-*-<id>.jsonl` | Appended bytes, shipped by offset |
| The lead's working directory | `~/.zz-lead/work`: `ledger.md`, briefs, reports | Whole files, when their checksum changes |
| Ownership | `~/.zz-lead/owner` on every host | Written by the handoff and by every mirror pass |
| Worker panes and their state | Each worker host's own daemon | Nothing to move: workers keep running when the lead dies, except those on the dead host |
| The lead's in-flight waits (background `agent-send --wait`) | Processes on the old owner | Lost; the restarted lead re-arms them from the ledger |

The last two rows are why the lead can be small. zz already holds the live state of every worker;
the lead only needs its chat and its ledger back. The `zz-lead` skill tells a lead that finds a
ledger at start that it was restarted: look at each open job's pane once and wait on it again.

# Facts measured on 2026-10-23

Fleet: ubuntu, xps and alienware on zz 0.21.0, the Mac on 0.22.0; Claude Code 2.1.308 and 2.1.309,
Codex 0.159 (ubuntu) and 0.163.

- **Claude resumes a copied transcript.** A session made on ubuntu with `claude -p --session-id`,
  copied to the same `~/.claude/projects/-home-demfabris-lead-home-test/` folder on xps, answered
  `claude -p --resume <id>` with the code word and the earlier tool output. The same file copied to
  the Mac, with `/home/demfabris` rewritten to `/Users/demfabris`, resumed too. Resume appends to
  the same file (35 to 51 lines), so after a move the old host's copy is a prefix of the new one.
  Claude's docs also allow `claude --resume /absolute/path/<id>.jsonl`, and
  `CLAUDE_CODE_PROJECT_DIR_NAME` (with `CLAUDE_CONFIG_DIR`) pins the folder name across home paths.
- **Codex resumes a copied rollout.** A Codex 0.159 rollout from ubuntu, copied to the same
  `~/.codex/sessions/2026/10/23/` path on xps (Codex 0.163), answered `codex exec resume <id>` with
  no row in xps's state database. Since 0.155.1 Codex refuses a conversation that is open in
  another app on the same machine (openai/codex#46652); that lock does not travel with the file.
  Cross-host resume is an open request there (openai/codex#52050).
- **Plain ssh between fleet hosts may lack keys.** `ssh xps` from ubuntu is refused (publickey);
  `zz --host xps` works with zz's own key. Anything built on `rsync`, `scp` or `sqlite3_rsync`
  between peers would need keys zz does not set up.
- **zz's link carries files.** `zz --host xps load-buffer -b x - < file` followed by
  `zz --host xps run-shell "zz save-buffer -b x PATH"` moved 289 KB in 34 ms. `save-buffer -a`
  appends. One load is capped at 1 MiB (`MAX_AGENT_SEND_BYTES`), so the mirror sends 900 KiB chunks.
  `save-buffer PATH` sent through `--host` writes on the calling machine, which is why the save runs
  inside `run-shell` on the target.
- **`zz --host H run-shell CMD`** runs a command there with no pane, prints its output and exits
  with its status. It takes no stdin.
- **Transcripts are big.** A one-message lead session was a 490 KB transcript; shipping by offset
  keeps each mirror pass proportional to what was appended.

# How pi-durable does it

Earendil's `@earendil-works/pi-durable` (1.4.0, MIT, experimental, released with Pi 1.0 on
2026-10-01) is the closest prior art, read from `earendil-works/pi` `packages/durable`.

- **It owns the loop.** It calls model APIs through `pi-ai` and stores its own transcript; it does
  not wrap Claude Code or Codex and cannot resume their sessions. A Claude subscription only works
  inside the unmodified `claude` binary, so adopting it would mean zz hosting a loop again, which
  the [agent lens](agent-lens.md) and peer-bus decisions ruled out.
- **Storage is a commit log.** Each commit is one atomic batch of immutable entries, task records
  with a checkpoint, submissions, and documents stored as a base plus deltas, in SQLite (8 STRICT
  tables, WAL) or JSONL (`main.jsonl`, one marker per commit, sidecars first).
- **Resume is checkpoint and replay.** Work commits its intent, runs, then commits the result. On
  reopen, running tasks go back to pending; a dead model stream is saved as an aborted entry and
  sent again; a tool reruns only when both the stored and the current definition declare
  `replay: "safe"`, otherwise the model gets an `interrupted` result with the output saved so far.
- **One writer, one machine.** "One process owns a storage at a time; there is no cross-process
  locking" (`README.md:783`). No leases, epochs or replication; moving a conversation means closing
  it and copying the database.

What zz borrows: never replay a side effect after a crash without knowing it is safe (the
restarted lead checks live state before it resends anything, which the skill already requires),
history that grows by appending markers instead of rewriting, and request ids so a resent message
is not applied twice. What zz adds that pi-durable does not have: cross-host ownership and
replication.

# Options compared

| Option | Fits the chat? | Over zz's link? | Writer handoff | Two writers | Adds |
|---|---|---|---|---|---|
| **Ship the vendor JSONL by offset, epoch fence** (recommended) | It is the chat | Yes: buffers plus `run-shell` | Epoch + 1 on every reachable host | Lower epoch is refused; its tail kept aside | Nothing |
| pi-durable | No: owns its own loop and transcript | n/a | One process per store | Undefined (id clash) | A Node runtime and a second agent stack |
| cr-sqlite (CRDT) | Only after importing the JSONL into tables and exporting it back | Changes are rows, so yes | Not needed, multi-writer | Per-column last writer wins: a merged chat with interleaved tool calls means nothing | A loadable extension; upstream dormant since v0.16.3 (Jan 2024), a Fly fork is active |
| Litestream 0.5 | A SQLite file only | SFTP replica wants a key file, not zz's link | Manual restore, then replicate | Unsupported (`litestream reset`) | A Go sidecar; one replica per database |
| LiteFS | A SQLite file only | No: FUSE plus its own port | Consul lease | Blocked by the lease | Linux only; frozen since v0.5.14 (Apr 2025) |
| SQLite session changesets | Tables zz owns | Yes: a changeset is bytes | None built in; pair with an epoch | Conflict handler per row | rusqlite `session` feature |
| sqlite3_rsync | Tables zz owns | No: spawns system `ssh` | None built in | Last push overwrites | A binary on every host |
| libSQL / Turso sync | Tables zz owns | No: needs a server or hub | Fixed primary or hub | Last push wins | A different engine plus a server |

fabrico's starting idea was a SQLite backbone plus sync. The table is why the chat should not be
that backbone: every SQLite option replicates rows zz would first have to copy out of the vendor's
JSONL, and the CLI can only resume from its own file, so zz would end up shipping the JSONL anyway.
A SQLite index still has a place as a local cache: the lens and the sidebar can build their
message tables on each host from the mirrored JSONL, and a cache rebuilt locally never needs
replicating. If zz later keeps state of its own that does not fit a file (say, a structured ledger
the sidebar edits), session changesets carried as buffers over the same link are the fit.

# The design

## On disk, on every host

```text
~/.zz-lead/owner      EPOCH HOST AGENT SESSION     e.g. "2 xps claude 1c2a492b-..."
~/.zz-lead/work/      the lead's working directory: ledger.md, briefs, reports
<the CLI's own transcript path for SESSION>
```

The lead always runs with `~/.zz-lead/work` as its cwd, so its Claude project folder has the same
name on every Linux host; for a Mac peer the mirror computes the folder from that host's `$HOME`
(or the lead runs with `CLAUDE_CODE_PROJECT_DIR_NAME` set).

## Mirroring

The owner's daemon watches the transcript (a byte offset per file, the way the lens tails it) and,
on growth or every 2 s, for each peer:

1. Read the peer's epoch. If it is higher, this host has been fenced: stop the local lead, stop
   mirroring, keep the local tail past the peer's length as `<id>.epoch<N>.jsonl`.
2. Ask the peer for its file length and a checksum of its last 4 KiB. If the checksum matches the
   same range here, send the bytes after that length; otherwise send the whole file. Both CLIs
   append; a rewrite (Codex migrating a legacy rollout) shows up as a checksum mismatch and costs
   one full send.
3. Send changed files of `~/.zz-lead/work` and new files of Claude's `<id>/` folder whole.
4. Write the owner line on the peer.

Transport is the connection `zz --host` already holds: `load-buffer` in 900 KiB chunks and
`save-buffer -a` inside `run-shell` on the peer. In the daemon this becomes the same two commands
sent over the existing remote client, not a new message type.

## Handoff

`zz lead move HOST` (or the sidebar's "Move lead here"):

1. Find the highest epoch among reachable hosts and its owner.
2. If the old owner answers, stop its lead pane and let it push one last pass to the new owner.
   If it does not, the new owner resumes from its own copy, at most one mirror pass behind.
3. Write epoch + 1 naming the new owner on the new owner, then on every other reachable host.
4. Start the CLI's resume in a zz pane on the new owner, cwd `~/.zz-lead/work`, and start its
   mirror.

Automatic failover is the same steps run by a standby when the owner's last mirror write on that
standby is older than a lease (`lead-lease`, default 5 minutes), with standbys tried in the
order of `lead-hosts`. It stays off until the manual path has been used for a while: a lease
alone does not stop a partitioned old owner from writing, which is what the epoch check in step 1
of mirroring is for.

## Split brain

Two owners can only exist if a handoff ran while the old owner was alive but unreachable. When
they reconnect, the first mirror pass of the lower epoch is refused, that host stops its lead, and
its divergent tail is kept as a side file. The winner's next prompt carries one line naming the
file. Chats are not merged: two interleaved runs of tool calls do not make one meaningful
conversation, and Claude Code itself interleaves silently if one session is resumed twice.

## What is lost on a crash

- Up to one mirror pass of transcript (2 s) when the owner dies without warning.
- A turn cut mid-tool: the transcript ends with a tool call and no result. Both CLIs resume and
  treat it as interrupted. The lead re-reads its ledger and checks live state before resending
  anything, as the skill requires.
- Background waits: re-armed from the ledger.
- Workers on the dead host: the ledger marks them lost; the lead starts fresh ones elsewhere.

## Viewing and talking to the lead from any machine

Every host holds a current copy, so the sidebar chat can render the lead's transcript locally
everywhere (the lens readers work on the mirror). Input goes to the owner: the sidebar sends it
with `zz --host OWNER agent-send -t %LEAD`, the peer bus delivers it into the lead's session, and
the reply shows up in the mirrored transcript a few seconds later.

## Settings and verbs

- `lead-agent` (`claude` or `codex`), `lead-hosts` (owner first, then standbys), `lead-lease`.
- `zz lead start|move HOST|status`. `status` prints owner, epoch, and each peer's lag in bytes.

# Prototype

`scripts/lead-sync.sh` implements start, mirror, push and handoff with zz verbs only. Run on the
fleet on 2026-10-23:

1. `lead-sync.sh start ubuntu claude` started a Claude lead in session `lead` on ubuntu with a
   `sync` window running the mirror. A message through `agent-send --wait` told it a code word and
   had it append a line to `ledger.md`.
2. Within one pass xps and alienware held the same 490,044-byte transcript, the ledger and
   `1 ubuntu claude <id>`.
3. The lead session on ubuntu was killed. `lead-sync.sh handoff xps` found ubuntu stopped, wrote
   epoch 2 on xps, ubuntu and alienware, and started `claude --resume <id>` on xps. Claude asked
   to trust `~/.zz-lead/work` there, a one-time answer per host that belongs to the user.
4. The resumed lead on xps answered with the code word and the ledger line.
5. Fencing: ubuntu's owner file was set back to epoch 1 and a stand-in lead started; its
   `lead-sync.sh push xps` exited 3 with `fenced: xps is at epoch 2, this machine at 1` and stopped
   the local lead. xps's mirror then brought ubuntu and alienware to the new 565,609-byte
   transcript at epoch 2.

The prototype polls every 2 s, runs one `run-shell` per file per peer, and keeps no side file for
a fenced tail; the daemon version would watch the file and batch.

# Open questions

- Folder trust: a resumed Claude lead asks to trust `~/.zz-lead/work` on each new host, and Codex
  asks per folder. `zz lead start` could create the folder on every host up front and ask the user
  once, with the CLI's own dialog, per host.
- Codex's "open in another app" lock (#46652) after a move: it is per machine today, so a copied
  rollout resumes, but a future shared daemon could change that.
- The JSONL formats are internal to each vendor. The mirror treats them as bytes, so only resume
  depends on them, and resume is the vendor's own code; keep the CLI versions on the fleet close.
- Never mirror `~/.claude` or `~/.codex` wholesale: they hold credentials.

---
type: Design Plan
title: Agent turn machine . one owner for "a turn is running"
description: "One pure state machine in the daemon decides whether a native driver's turn is running, waiting, cancelling or done, from a small set of facts the Claude and Codex drivers report. It replaces the six turn flags spread over the drivers, the host, the fanout lane and the desktop client. zz-machine holds the shape (transition, decision, wake time from the state). The Claude and Codex drivers run it and replay their recordings through it in tests; the host, fanout lane and clients mirror the turn events instead of deriving their own."
status: Built (2026-10-21; drivers run the machine, host, fanout and clients mirror it; shared client dispatcher and timeline turn boundaries next; parked 2026-10-23, when the Agent pane that ran the drivers was removed; the lens does not use the machine)
resource: crates/zz-daemon/src/agent/turn.rs
tags:
- agent
- design-plan
timestamp: 2026-10-23T00:00:00-03:00
---

**Status 2026-10-23.** The Agent pane is gone ([agent lens](/designs/agent-lens.md)). The machine
stays in `agent/turn.rs` and the Claude and Codex drivers still run it, but no pane opens a driver
now. The lens runner (`agent/lens.rs`) does not use it: it settles a
turn from Claude's session record (`busy`, `idle`, `waiting`) and transcript, or Codex's
`task_started` and `task_complete` lines. Where this page says "pane", read the driver-backed Agent
pane that existed until then. The subagent feed and its `external_turn` in the fanout lane, and the
sidebar status record for agent panes, were removed the same day. The machine was kept for a driver
chat; that shipped as the Driver Agent, which followed a terminal through the lens instead, and was
removed on 2026-10-28.

# Why

"Is this turn still running" is decided in six places today: the driver's `turn` (plus
`external_turn` on Codex and `stale_commands` / `lifecycle_seen` on Claude), the host's
`active_turn` and `external_busy`, the fanout lane's `external_turn`, `turn_end` and `turn_id`, and
the desktop client's `AgentConnectionState` with `background_turn`. Each reads a different subset of
signals. The turn the agent starts on its own needed three fixes, one per flag (9a9558226,
f88dacaf3, bda6042b6).

A Codex review on 2026-10-21 found three more in the Claude driver alone: `/steer` ends the turn as
cancelled while the steered command keeps running, a hard failure is reported as a user cancel, and
a late error from a cancelled turn fails the next one.

# zz-machine

`crates/zz-machine` is the shape every machine uses, about 100 lines with no dependencies:

- `Machine::transition(state, signal, ctx) -> Decision`: pure. The clock comes in through `ctx`.
- `Decision`: the next state (or stay), the effects to run, or a reject. A reject is a signal the
  machine refuses as data (a stale command, a steer with nothing running), not an error.
- `Machine::wake_at(state)`: when the machine next wants a `Tick`, read from the state.
- `Machine::step(&mut state, signal, ctx) -> Step`: applies the decision and returns the effects,
  the reject and the wake time.

It started from fabrico's clairvo state machine library. Two changes: the wake time is read from the
state instead of set by each decision, because a `stay()` there cleared a pending timer and
filtering past deadlines could drop one that was overdue; and the async driver half (commit, lost
races) is gone, since one thread owns each pane.

# The turn machine

`crates/zz-daemon/src/agent/turn.rs`. Phases: `Offline`, `Idle`, `Running(turn)`,
`Cancelling { turn, deadline }`, `Exited`. Waiting is derived: a running turn with open permissions.

A turn has an owner (the host's turn id, or none when the agent started it) and a list of vendor
commands: Claude message uuids, Codex turn ids.

Facts in:

| Fact | Claude | Codex |
|---|---|---|
| `Prompt { turn, command }` | host writes a prompt with a uuid it minted | host sends `turn/start` (command unknown yet) |
| `Accepted { turn, command }` | not needed | `turn/start` reply carries the turn id |
| `Steer { command }` | steer written with a new uuid | `turn/steer` (same turn id, no change) |
| `Cancel` | user stop | user stop |
| `Started { command }` | `command_lifecycle` started | `turn/started` |
| `Reported { command, stop_reason, error }` | `result`, once per uuid in `user_message_uuids` | not needed |
| `Ended { command, end }` | `command_lifecycle` completed, cancelled, failed or errored | `turn/completed` completed, interrupted or failed |
| `Busy` / `Idle` | `session_state_changed` | `thread/status/changed` |
| `PermissionOpened` / `Closed` | `can_use_tool` / `control_cancel_request` | approval request / `serverRequest/resolved` |
| `Exited` | process exit | process exit |
| `SessionReady` / `SessionClosed` | session opened / reset or switched | same |
| `Tick` | the timer from `wake_at` | same |

Effects out: `Settle { turn, outcome }`, `Interrupt`, `Dismiss { requests }`, `Drain` (send the
next queued prompt), `Reclaim` (hand the queue back to the composer).

Rules:

1. A turn ends when every command in it has ended, or when the vendor says `Idle` after the turn
   began and no command in it is still open (started, not ended). An `Idle` from before the turn
   began is ignored. Codex reports the thread idle just before `turn/completed`, so a started
   command waits for its own end and its real status.
2. A steer adds its command to the running turn, so the old command ending as cancelled does not end
   the turn.
3. Every end and every result names its command. A command from a settled turn is rejected as stale
   (the last 32 are kept), so it cannot touch the next turn.
4. Outcome: a user cancel wins; then an error reported for the turn fails it; then the last command
   completing completes it; anything else is cancelled. A generic failed end keeps the more specific
   error the agent reported first.
5. Cancel sends an interrupt and waits up to 10 seconds for the vendor to end the turn, then settles
   it as cancelled anyway. An `Idle` while stopping settles only once no started command is open
   (Codex reports idle before the interrupted turn's last items, added 2026-10-29). A command that starts while the turn is cancelling is interrupted too,
   so a stop sent before Codex names its turn still reaches it.
6. A command nobody here sent, or `Busy` while idle, opens a turn owned by the agent. A prompt that
   lands during it takes the turn over.
7. A settle or a cancel closes the permission cards opened during that turn. A card asked for while
   idle (a background subagent) survives until it is answered.
8. A completed turn, or one the agent owned, drains the queue; a cancelled or failed one hands it
   back.

`Prompt` while a turn runs is rejected so the host queues it; `Steer` with nothing running is
rejected so the host sends it as a prompt, as today.

# Wiring

## Built: the drivers run the machine

Each driver (`claude/mod.rs`, `codex/mod.rs`) keeps a `TurnState`, turns vendor frames into facts
through `apply`, and turns the effects back into what the host already reads: `Settle` becomes
`PromptFinished` (or `Activity { busy: false }` for an agent turn), a new agent turn becomes
`Activity { busy: true }`, `Interrupt` becomes Claude's `interrupt` control request or Codex
`turn/interrupt`. The old `turn`, `stale_commands`, `lifecycle_seen` and `external_turn` fields are
gone. Codex `/compact` is a command named `zz-compact-<turn>` that the RPC reply ends; a refused
`turn/start` ends the unbound turn as failed.

What a user sees change:

- Stop waits for the vendor to confirm (up to 10 seconds) before the pane goes back to ready,
  instead of flipping at once while the agent winds down.
- `/steer` keeps the turn running until the steered command finishes.
- A Claude command that ends failed or errored reports a failure, with the error Claude reported.
- A Codex turn that fails or is interrupted reports that, not a completion.
- Claude's task tray links a background task to its tool row even when the task registry arrives
  before `task_started`.

`claude/replay_tests.rs` and `codex/replay_tests.rs` run the recordings through the real runtimes
and check every turn event and the line it lands on.

## Built: everything above the drivers mirrors them

The plan was to move the machine into the host. It stayed in the drivers instead: each driver sees
the vendor's frames in order and already owns the vendor ids, and the generic ACP runtime
(`runtime.rs`) keeps its simpler model, where the `session/prompt` reply ends a turn. What changed
above the drivers is that nothing derives turn state on its own any more:

- The host dropped `external_busy`. `Activity` from a driver is now one edge per agent turn, so
  Running with no prompt of the host's own in flight already means the agent started the turn.
- A failed turn leaves the pane Ready with the error shown (the sidebar status reads Error with the
  error's first line). Only the agent exiting fails the pane. Before this, any failed turn left the
  pane Failed and its next prompt queued forever, though the agent was still running.
- The fanout lane clears its `turn_id` when that turn finishes, so a replay after a lag no longer
  says a finished turn is running. Its `external_turn` stays: the subagent feed repeats
  `Activity { busy: true }` when it replaces its list.
- The desktop client's `background_turn` became `submitted`: set when it sends a prompt from Ready,
  cleared once the daemon shows a turn running, the prompt finishes, or the queue is handed back.
  The daemon's Ready ends the turn whenever no prompt of the client's is still on its way. The thin
  client already drew the daemon's phase.

Fixed on the way: an agent exit resolves its open permission cards for the clients, a Claude
permission card answered through the question path keeps waiting for a real answer instead of
being dropped, and a background Bash task that ends before its launch result settles its row.

## Next

1. One stream dispatcher in `zz-client` for desktop and thin. They differ today on
   `finish_replay`, `sessionReset` and `stateSynced`.
2. Turn boundaries in the timeline: rows group by turn instead of cutting at each user row, so a
   turn the agent starts gets its own row and clock. This needs a marker the journal keeps, or it
   folds back after a replay.

# Not covered

- Session lifecycle (starting, restoring, auth) stays in the host for now; the machine only needs
  `Offline` until a session is ready.
- A "background" phase while Claude's background tasks run: the turn ends with its command and the
  task tray shows the tasks; the wake is a new agent turn.
- An "unconfirmed" phase: the daemon owns the agent processes, so it always knows. Clients that
  reattach need it, and that belongs with step 3.

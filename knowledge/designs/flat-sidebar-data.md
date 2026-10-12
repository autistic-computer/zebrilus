---
type: Design Plan
title: "Flat sidebar data: pane summaries, attention and time"
description: "The data layer for a flat, two-line-per-window sidebar on every client: one PaneSummary per pane in the tree (last change time, unseen finish latch, last shell command, @name, display cwd and git branch), one status record for every agent kind, the daemon clock on the handshake, and a renderer-free view model in zz-client. Both data slices are built; no client draws the sidebar yet."
status: Complete
resource: crates/zz-client/src/window_summary.rs
tags:
- sidebar
- protocol
- pane-summary
- agent
- program-status
- design-plan
timestamp: 2026-10-12T18:00:00-03:00
---

# Overview

The flat sidebar shows one row per window on every host: a title, a second line saying what the
window is doing, an age on the right, and a pane stack: one tile for the active pane with a card
behind it for each other pane, two at most. Line 2's lead carries what needs you, so the tiles have
no status dot; folded host and session headers roll attention up. Every fact it needs now rides
the mux tree, so a client that subscribes to all sessions can draw every row
without `AgentPaneWire`, without attaching, and without keeping its own "seen" state.

This plan covers the daemon producers, the wire fields, and the view model. The row, header and
sidebar builders live in `zz_ui::navigation::flat` with a storybook story (`flat-sidebar`); wiring
them into the desktop `MuxTreeModel` and the thin-client tree is the next piece of work and is not
built. A window row draws its highlight with `navigation::workspace_row_fill`, the same fill as the
tree rows (outline, corners, inset and press), and its corners and tiles follow the theme's corner
rule like every other control, so no row or tile forces a pill.

# What every client receives

- `PaneSnapshot.status` is the one attention record for all agent kinds: OSC 7501 programs
  ([program status](/terminal/program-status.md)), native Agent panes (computed by the agent fanout,
  see [Native Agent pane](/concepts/agent-pane.md)), and terminal panes without a 7501 record through
  a mirror of their `@agent_state` option.
- `PaneSnapshot.summary: PaneSummary` holds `changed_at`, `unseen`, `command`, `name`, `cwd` and
  `branch`, with its own `TreeOp::PaneSummary` at tag 18.
- `Welcome.clock` and `ServerHello.clock` carry the daemon wall clock once per connection.

Field meanings, bounds and the exact stamping rules live in
[snapshots](/protocol/snapshots.md#pane-status-summary-and-time); the wire changes are listed under
v108 in [the wire protocol](/protocol/wire-protocol.md). v108 was still unreleased when this
landed, so there was no version bump. If a release tags v108 first, bump to 109.

# Decisions

**Time moves on events only.** `changed_at` is stamped by status state or kind edges, shell
command start and finish, a bell raise and pane death. Output never stamps it, so an idle-looking
pane printing logs keeps its age, and a busy shell costs at most one summary op per command mark,
riding the publish its title change already causes (16 ms coalescing).

**`unseen` lives in the daemon.** It is raised on a finish edge with the bell's suppression rule
(the window is current in an attached session) and cleared on the same visits that clear the bell.
Every client on every host sees the same unseen state, and none keeps a seen store.

**The clock rides the handshake, not every snapshot.** An empty tree diff sends nothing, so a
per-snapshot clock would refresh no more often than the handshake on an idle host, and it would
need a field on `MuxSnapshot` and both publish paths. Clients keep an offset
(`DaemonClock::from_hello`) and compute ages against daemon time. A clock step on the remote while
connected goes uncorrected until the next reconnect.

**Native agents report through `PaneStatus`, not `AgentPaneWire`.** The heavy wire carries large
JSON blobs and republishes on config, mode and task edges; the status record is bounded and changes
only on phase, permission and turn-end edges.

**Location never stats under the lock or on the helper pool.** cwd and branch are resolved on a
location thread of its own that starts when a job arrives and exits when its queue is empty, one
job per pane in flight. The branch is read from `.git` and `HEAD` directly instead of running git or
adding a dependency.

# The view model

`zz_client::window_summary` is pure: no clock, no renderer, no host type. `summarize_window`,
`summarize_session` and `summarize_host` turn one host's `MuxSnapshot` into rows and rollups.

**Attention per pane**, first match wins: `Blocked` is NeedsYou, `Error` is Failed, `Working` is
Working, then `unseen` is Done, then the bell. A window takes the most urgent of its panes.

**Headline pane** (what line 2 and the age describe, and the row's click target): the pane with
the highest attention among NeedsYou, Failed, Done and Working; ties go to the active pane, then the
newer `changed_at`, then layout order. With none, the active pane. A bell never takes line 2.

**Title (line 1)** describes the title pane: the headline when it is agent-like (Agent kind, a
status record, or a `@name`), else the active pane. First that applies:

1. The title pane's own title when it is an Agent pane and the title is not the default `agent`,
   else its status title, else its `@name`.
2. The window name when the user renamed it (`automatic_rename` off).
3. `navigation::pane_label` of the title pane: the program title, the page title, or "New session".
4. The window name.

An agent's title or `@name` beats an explicit rename, and an explicit rename beats program titles.

**Line 2** reads the headline pane and is never empty. In order: Needs approval, a question, Needs
you, Failed (each with the status message), Done with the reply or message, Working with its message
or title and progress, Done for an unseen finish, an idle message, Exited with a non-zero status,
Exit with a failed shell command and the location, the browser's host ("New tab" for a blank page),
the editor's file, "New pane" for a picker, the location (`cwd` folder and branch), then the window
name or "Shell" for terminals and the provider label for agents. A lead (Needs approval, Question,
Needs you, Failed, Done, Exit) takes its tone and anything after it follows muted after " · ", as in
the agreed mock; Done is in the foreground only while the finish is unseen.

**Ages** are `short_age`: "now" under a minute, then minutes, hours and days. `age_label` shows
nothing while `changed_at` is 0. **Tiles** put the active pane first, then layout order; the row draws the first as its tile and
one card behind it for each of the next two.
**Rollups** count one attention per window and keep the newest change; a host merges its sessions.

# What the source changed in the first draft

- The draft said agent shadow terminals carry OSC 133 marks and ring the bell at turn end. They do
  not: they are PTY-free surfaces that never push shell marks, so `note_shell_mark` only acts on
  Terminal panes and Done for agents comes from the status record. Whether the projection's
  BEL reaches the bell latch at all is unverified.
- `@agent_state idle` on a pane with no record leaves it without one, because the daemon writes
  `idle` when a 7501 owner leaves or the pane respawns.
- A turn-end `Activity { busy: false }` only counts as Done after an agent-started turn, so
  Claude's idle notice after a cancelled turn does not raise a false Done; a reset or restored
  session ends Idle.
- New panes did not get a location: the spawn path records the live path itself, so the first
  runtime check sees no change. Terminal spawns and Agent pane opens now start a location job.
- Location jobs first ran on the two-worker helper pool. Two panes on a hung mount then blocked
  both workers, and source reads, terminfo warmup, path listings, history and the Claude peer scan
  queued behind them, so they moved to their own thread. It applies results through a weak handle
  on the server, not the helper result queue, because a test build drains that queue on a
  connection thread and the job would otherwise hold one.
- Review fixes: line 1 names the headline's pane, not the active one, when an agent-like headline
  has no title of its own; a first `Idle` record does not stamp an opening Agent pane; an Agent
  pane's `Working` to `Idle` (a cancelled turn) raises no `unseen`; an agent-started turn drops the
  previous turn's reply; a shell mark publishes with the title change of the same output frame
  instead of on its own; a location job that is triggered again drops its stale result; a reftable
  `HEAD` shows no branch; and line 2 splits Windows paths on `\`.
- `.git` is followed through symlinks, since tools such as Android's `repo` link it.
- `kill-session -C` also clears `unseen`, and `TreeOp::UpsertPane` boxes its pane (the summary made
  the variant large; the wire bytes are unchanged).

# Not built, on purpose

- Drawing the rows: wiring `summarize_window` into the desktop `MuxTreeModel` and the thin-client
  tree, a `DaemonClock` per host connection, and a once-a-minute repaint for ages.
- A live "what the agent is doing" line while Working, agent diff stats on line 2, an `Auth` kind
  for native agents, `#{pane_unseen}` and `#{pane_changed_at}` formats, chimes for every host from
  `unseen` edges, and retiring `AgentAttentionTracker`, `AgentBadge` and the thin client's
  `unseen_agents` in favour of the shared model.

# Risks

- Suppression follows the bell rule, so a forgotten attached client keeps that window's finishes
  "seen".
- A daemon restart clears `unseen` and `changed_at`, like the bell.
- OSC 7 from a nested ssh shell names a path on another machine; the daemon only accepts a
  directory that exists locally, so the worst case is a wrong but harmless branch.
- A stat that blocks on a network mount holds the location thread until it returns, so no pane's
  cwd or branch updates in the meantime.

# Related

- [Mux snapshots](/protocol/snapshots.md), [the wire protocol](/protocol/wire-protocol.md)
- [Program status (OSC 7501)](/terminal/program-status.md), [Native Agent pane](/concepts/agent-pane.md)
- [Terminal bell](/designs/terminal-bell.md), [Fleet attach](/designs/fleet-attach.md)

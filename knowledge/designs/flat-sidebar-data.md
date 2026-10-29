---
type: Design Plan
title: "Flat sidebar data: pane summaries, attention and time"
description: "The data layer for a flat, two-line-per-window sidebar on every client: one PaneSummary per pane in the tree (last change time, unseen finish latch, last shell command, @name, display cwd and git branch, and the program running in a terminal), one status record for every agent kind, the daemon clock on the handshake, and a renderer-free view model in zz-client. Both data slices are built, and the desktop, web and iPad sidebars draw it when the footer's eye switch is on."
status: Complete (Agent pane parts removed 2026-10-23)
resource: crates/zz-client/src/window_summary.rs
tags:
- sidebar
- protocol
- pane-summary
- agent
- program-status
- design-plan
timestamp: 2026-10-26T00:00:00-03:00
---

# Overview

Since 2026-10-28 the flat list is [chat mode](chat-mode.md): its rows are the windows running an
agent, under their machine with no session headers. The data layer below is unchanged.

The flat sidebar shows one row per window on every host: a title, a second line saying what the
window is doing, an age on the right, and a pane stack (`navigation::pane_stack`, shared with the
titlebar's pane deck): the focused pane's tile with its glyph in front and at most two blank tiles fanned out behind it, each tile outlined on its own (dark in both modes and lit like a program icon, with no theme hairline: a gray line outside and a white rim inside in light mode, a dark line and a faint rim in dark mode), so the one in front reads against the one behind. Pointing at it opens every pane in a row you can
click, drawn on the row itself with the title and description hidden under the tiles and fading
back in past them. Line 2's lead carries what needs you, so the tiles have
no status dot; folded host and session headers roll attention up. Every fact it needs now rides
the mux tree, so a client that subscribes to all sessions can draw every row without
`AgentPaneWire`, without attaching, and without keeping its own "seen" state.

This plan covers the daemon producers, the wire fields, and the view model. The row, header and
sidebar builders live in `zz_ui::navigation::flat` with a storybook story (`flat-sidebar`). The eye
button at the bottom right of the sidebar footer switches between the session tree and this list:
the desktop draws every fleet host from `MuxClient::fleet_hosts` and remembers the choice as
`flat-sidebar` in `zz/config` (`crates/zz/src/workspace/sidebar/flat.rs`); the web and iPad clients
draw their one host and keep it in their saved preferences. Row actions open a rename and close menu
(`FlatMenu`) from a right click or a long press. A window row draws its highlight with
`navigation::workspace_row_fill`, the same fill as the tree rows (outline, inset and press), but
gives a round shape a 24px corner whatever the theme's radius (square and pill stay), and round tiles stay concentric with
it, so the two-line rows never turn into pills.

# What every client receives

- `PaneSnapshot.status` is the one attention record for all agent kinds: OSC 7501 programs
  ([program status](/terminal/program-status.md)) and terminal panes without a 7501 record through
  a mirror of their `@agent_state` option. Native Agent panes also filled it from the agent fanout
  until the Agent pane was removed on 2026-10-23 ([agent lens](/designs/agent-lens.md)); the fanout
  computes no status record now.
- `PaneSnapshot.summary: PaneSummary` holds `changed_at`, `unseen`, `command`, `name`, `cwd`,
  `repo`, `branch` and `process`, with its own `TreeOp::PaneSummary` at tag 18.
- `Welcome.clock` and `ServerHello.clock` carry the daemon wall clock once per connection.

Field meanings, bounds and the exact stamping rules live in
[snapshots](/protocol/snapshots.md#pane-status-summary-and-time); the wire changes are listed under
v108 in [the wire protocol](/protocol/wire-protocol.md). v108 was still unreleased when this
landed, so there was no version bump. If a release tags v108 first, bump to 109. `process` came
later, in v112.

# Decisions

**Time moves on events only.** `changed_at` is stamped by status state or kind edges, shell
command start and finish, a bell raise and pane death. Output never stamps it, so an idle-looking
pane printing logs keeps its age, and a busy shell costs at most one summary op per command mark,
riding the publish its title change already causes (16 ms coalescing).

**`unseen` lives in the daemon.** It is raised on a finish edge unless a focused client shows the
pane's window, and cleared on the same visits that clear the bell, plus a client's focus-in for the
window it shows.
Every client on every host sees the same unseen state, and none keeps a seen store.

**The clock rides the handshake, not every snapshot.** An empty tree diff sends nothing, so a
per-snapshot clock would refresh no more often than the handshake on an idle host, and it would
need a field on `MuxSnapshot` and both publish paths. Clients keep an offset
(`DaemonClock::from_hello`) and compute ages against daemon time. A clock step on the remote while
connected goes uncorrected until the next reconnect.

**Native agents report through `PaneStatus`, not `AgentPaneWire`.** The heavy wire carries large
JSON blobs and republishes on config, mode and task edges; the status record is bounded and changes
only on phase, permission and turn-end edges. (History: this applied to the Agent pane, removed on
2026-10-23. A terminal running Claude Code or Codex reports through OSC 7501 or `@agent_state`
like any terminal.)

**Location never stats under the lock or on the helper pool.** cwd and branch are resolved on a
location thread of its own that starts when a job arrives and exits when its queue is empty, one
job per pane in flight. The branch is read from `.git` and `HEAD` directly instead of running git or
adding a dependency.

**A pane that moves takes a known answer; a finished command reads again.** The location thread's
last 64 answers are kept by source. A spawn, an OSC 7 change or a live path change fills the
summary from them under the lock, so the location rides the publish of the move. An answer read in
the last 10 seconds is taken with no job of its own; an older one is shown and read again at once.
Every pane that took an answer without reading is read again 2 seconds after the first such take,
once per source for the whole burst, so an answer that went stale just before the take is put
right within 2 seconds.
A read also renews every remembered answer and every pane that shows the same directory, so the
OSC 7 and live path answers for one directory, and the panes in it, agree. Finished shell commands
and agent turns always read again. A result publishes only when it changes a summary.

**`HOME` folds in both spellings.** The home is kept as given and as it resolves through symlinks
(computed once, on the location thread), because a pane started in `HOME` reports the given
spelling while the live path of its process is the resolved one. On macOS a home under `/var` or
`/tmp` otherwise flipped from `~` to `/private/...` a moment after the spawn, with a publish of its
own.

# The view model

`zz_client::window_summary` is pure: no clock, no renderer, no host type. `summarize_window`,
`summarize_session` and `summarize_host` turn one host's `MuxSnapshot` into rows and rollups.

**Attention per pane**, first match wins: `Blocked` is NeedsYou, `Error` is Failed, `Working` is
Working, then `unseen` is Done, then the bell. A window takes the most urgent of its panes.

**Headline pane** (what line 2 and the age describe, and the row's click target): the pane with
the highest attention among NeedsYou, Failed, Done and Working; ties go to the active pane, then the
newer `changed_at`, then layout order. With none, the active pane. A bell never takes line 2.

**Title (line 1)** describes the title pane: the headline when it is agent-like (a status record
or a `@name`), else the active pane. First that applies:

1. The title pane's status title, else its `@name`. (Until 2026-10-23 an Agent pane's own title
   came first when it was not the default `agent`.)
2. The window name when the user renamed it (`automatic_rename` off).
3. `navigation::pane_label` of the title pane: the program title, the page title, or "New session".
4. The window name.

An agent's title or `@name` beats an explicit rename, and an explicit rename beats program titles.

**Line 2** reads the headline pane and is never empty. In order: Needs approval, a question, Needs
you, Failed (each with the status message), Done with the reply or message, Working with its message
or title and progress, Done for an unseen finish, an idle message, Exited with a non-zero status,
Exit with a failed shell command and the location, the browser's host ("New tab" for a blank page),
the editor's file, "New pane" for a picker, the location (`cwd` folder and branch), then the window
name or "Shell" for terminals (the provider label for Agent panes until they were removed on
2026-10-23). A lead (Needs approval, Question,
Needs you, Failed, Done, Exit) takes its tone and anything after it follows muted after " · ", as in
the agreed mock; Done is in the foreground only while the finish is unseen.

**Ages** are `short_age`: "now" under a minute, then minutes, hours and days. `age_label` shows
nothing while `changed_at` is 0. **Tiles** put the active pane first, then layout order, each with
its pane id, label and program (`pane_program`, below); the row draws the first with its glyph and
a blank tile for each of the next two, and the open row offers every pane (`FlatEvent::OpenPane`).
**Rollups** count one attention per window and keep the newest change; a host merges its sessions.

**Session headers** come from `navigation::session_label`, which every sidebar, the palettes and the
TUI share. A session the user named keeps its name. A numbered one (tmux's default `0`, `1`, ...)
is labeled with the place most of its panes are in: the last folder of their `repo`, else of their
`cwd` (`~` at home), ties going to the active pane. Its number follows, faint, as `zz 0`, so the
target to type stays visible and two sessions in one repo still differ. The name itself never
changes, so `-t 0` and `#S` behave as in tmux.

# Program icons

A terminal pane's tile shows the icon of the program running in it. The daemon
does not pick the icon: `PaneSummary.process` carries the names it found, and the client matches
them, so a client with new icons works against an older v112 daemon.

**The daemon lists names, foreground first.** It starts at the terminal's foreground process
group and walks parents up to the pane's shell, at most 16 processes, and leaves out any known
shell, so an idle shell sends an empty list. Each process gives its script (for `node`, `bun`,
`deno`, `python`, `ruby`, `perl` and `php`), the script without its extension, the invoked path
(argv0 found in the process `PATH`, kept only when it is the executable or a symlink to it), argv0,
the process name and the executable path. On Windows, where ConPTY has no foreground group, it
starts at the newest leaf process under the shell instead. The exact rules are in
[snapshots](/protocol/snapshots.md#pane-status-summary-and-time).

**It is cheap because it rides the existing check.** The runtime check behind
`#{pane_current_command}` already reads the foreground pid and its name after output or a frame,
at most every 500 ms per pane. The daemon compares that pair with the one behind the pane's list
inside the lock the check already takes, and stops there when they match. A shell name clears the
list in the same place. A new pair waits until its process is 250 ms old, which keeps a shell loop
of short commands (`echo; sleep 0.01`) from flipping the list; a young one gets a single timer
callback for when it turns 250 ms, so a program that draws once and goes quiet still gets its
icon. Once settled it queues one job per pane on the location thread, and a result that matches
the summary sends nothing. An idle pane
and a program that keeps running cost nothing past that comparison.

**The client picks the icon.** `zz_client::program::resolve` takes the first name that ends in a
known command at a `/` boundary (case folded, `\` read as `/`, `.exe` dropped, and tried again
without its extension), the longest command winning, so `/home/me/bin/claude.py` and
`codex` (from `exec -a codex sleep 30`) both resolve, and a later `node` or `python3` never beats
the script before it. `pane_program` resolves terminal panes only; Agent panes kept their
provider's icon until the kind was removed on 2026-10-23, and `pane_agent` now maps a terminal whose
program is `claude` or `codex` to its provider, which is what gets the lens button.

**The tree draws the mark alone.** The session tree on desktop, web and iPad puts the program's
mark, without its tile, in the row's ink where the kind icon was (`zz_ui::program::program_mark`):
on a pane row, and on a folded window row for its active pane. A logo with more than one color has
no single-ink mark, so its row keeps the kind icon.

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
- The daemon perf gate caught every spawn paying for a location job: `spawn.instr.split_empty_P`
  went from 0.55 to 1.11 Minstr, `split_shell` and `new_window` from about 3.0 to 3.6. Knocking out
  pieces one at a time showed the publish was not the cost with no client attached: starting and
  ending the location thread took about 0.15 Minstr and the stats and reads about 0.3. With one
  client attached the second tree publish added about 2 Minstr per spawn on top. Remembered answers
  remove all three for a spawn in a known directory, back to 0.55, 3.06 and 3.05. Keeping the
  thread alive between jobs was tried and dropped: it saved only the thread cost.
- The attach bytes the gate reported (+192 B for four panes) are the summary itself, about 8 B per
  pane plus the text of the cwd and branch. The gate's panes started in the directory `run.py` was
  launched from, so the result depended on who ran it: from this worktree +260 B, which failed the
  256 B floor, from `/` +32 B. `run.py` now runs its groups in the gate's `HOME`, where every pane
  shows `~` and no branch: about +32 B for four panes.
- Review of the remembered answers: a new pane took a fresh answer and nothing read it again, so a
  command pane opened right after a branch switch outside zz kept the old branch for good, and the
  OSC 7 and live path answers for one directory disagreed after a shell's finished command. The
  2 second re-read and the shared renewal fix both. The re-read waits for the burst instead of
  following each spawn, and runs at 2 seconds rather than at the end of the 10 second window so
  that it is over before the gate's idle group starts measuring (at 10 seconds it showed there as
  0.05 Minstr/s and 0.1 wakeups/s against zero on main).
- Spawns more than 10 seconds apart still read at once: with one client attached a lone spawn costs
  about 0.6 to 0.9 Minstr more than on main, while spawns in a burst match main. The gate's spawn
  group measures the burst.

# Not built, on purpose

- A `DaemonClock` per host connection (the desktop reads ages against the local clock) and a
  once-a-minute repaint for ages; keyboard navigation in the flat list.
- Program icons for a program that stays in the foreground for less than 250 ms: it is skipped
  while young on purpose.
- Program icons for a pane whose program runs in the background of its shell's process group:
  the foreground group's leader is the only starting point, so a script that a non-interactive
  shell runs without `exec` shows the shell's (empty) list. A program that execs another binary
  under the same pid and name keeps its old list until its pid or name changes.
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
- A pane shows a branch switched outside zz only when something reads its directory again: its own
  next move or finished command, any read of the same directory, or the 2 second re-read after it
  took a remembered answer. An editor left open in a repository keeps the branch it started with,
  as before remembered answers.
- One directory reached through a symlink has two spellings, and their answers renew separately.

# Related

- [Mux snapshots](/protocol/snapshots.md), [the wire protocol](/protocol/wire-protocol.md)
- [Program status (OSC 7501)](/terminal/program-status.md), [Agent lens](/concepts/agent-lens.md)
- [Terminal bell](/designs/terminal-bell.md), [Fleet attach](/designs/fleet-attach.md)

---
type: Design Plan
title: PiP panes . every float, thrown to one of nine places
description: "In the GUI clients every floating pane that is not modal draws as a PiP: the pane in a card that casts the float shadow, with the same header as a tiled pane, tile back and zoom in place of the splits. Drag the title to throw it, iOS style, onto one of the nine places move-pane -P names. PiPs on one place fan out so each header shows, and they slide off the side edge, 60 px left showing, while a tiled pane has focus. Underneath it stays a tmux 3.8 float with no new flag, state or wire."
status: Built (2026-10-18; desktop, web and iPad; Claude and Codex subagent feeds built)
resource: crates/zz-ui/src/pip.rs
tags:
- ui
- floating-panes
- design-plan
timestamp: 2026-10-18T00:00:00Z
---

# Rulings (fabrico, 2026-10-18)

1. PiP is not an agent feature. Any pane can float, and every non-modal float draws as a PiP in the
   GUI clients. No flag, no pane kind, no protocol field: a PiP is a [floating pane](/designs/floating-panes.md).
2. Modal floats (`display-popup`, the confirm prompt) keep `FloatingSurface` and its grips. The raw
   TUI keeps tmux's borders.
3. A drag snaps to the nine places `move-pane -P` names: `top-left`, `top-centre`, `top-right`,
   `centre-left`, `centre`, `centre-right`, `bottom-left`, `bottom-centre`, `bottom-right`.
4. A window resize follows 3.8: floats are pulled in when the window shrinks and stay put when it
   grows. zz keeps no anchor.
5. A PiP can be thrown. Its release speed carries it, and it glides to the place it was headed for.
6. (later the same day) No slots while dragging. The nine zones split the window in thirds, so they
   never overlap whatever the PiP's size. The title row has no fill, like a pane header. No accent
   edge: a PiP stands off the panes by its float shadow, which the theme sets. The Float button
   opens a smaller PiP than tmux's default.
7. (evening) A PiP uses the same header as a tiled pane, with tile back and zoom where the splits
   were, so a browser keeps one header row and an agent keeps its rename. A resize never grows a
   PiP past the window. PiPs thrown on one place fan out instead of covering each other. Focusing
   a tiled pane stashes the PiPs at the side edge, iPadOS style. Piles and stashing are drawn by
   the GUI only: tmux keeps the floats where they are and the TUI draws them there.
8. (late evening) A stash slides the PiP past the side edge and leaves 60 px of it showing,
   content and all, with no tab or chevron; a perspective turn into the edge was tried and
   dropped. `pane-background-opacity` is gone: panes always show the window behind them. PiPs get
   `pip-background-opacity` instead, and below 100% they blur what is under them. Every pane
   movement animates in the GUI: splits, closes, zoom, moves, Float and Tile back.

# Look

`PipSurface` (`crates/zz-ui/src/pip.rs`) is the card: the pane's colour (`Theme::pane_color`, a
terminal's background over the window) laid at `pip-background-opacity` (`Theme::pip_opacity`,
80%) over a frost of what is under it (`pip_material`: the theme's glass in a glass theme, a 24 px
blur otherwise; a renderer that cannot read the backdrop paints the colour solid), corners of
`Theme::pane_radius`, and the `pane.pip`
role under `pane.float`, which casts the theme's float shadow (Materials in the theme creator; Roles
→ PiPs can set none, contact or float). Focus does not change it. The dark palettes draw shadows in
20% black, so on a dark pane the default reads faintly.

The header is the pane's own header, the one a tiled pane draws (`pane_header` and
`terminal_pane_header`, `agent_pane_header`, the browser's `BrowserHeader`), with
`pane_header_actions(floats)` swapping Split bottom, Split right and Float for Tile back
(`join-pane -s %N -t %N`) and Zoom (`resize-pane -Z`). The title keeps its X, an agent keeps its
rename, and a browser keeps its tabs on the one header row. `TerminalPaneAction::commands` holds
the commands every client sends.

- **Terminals** (and pickers and editors, which have no header of their own) get the header in the
  float's top border cell, `PipSurface::header`, at the theme's pane header height, and the content
  under it inset by the border cells, so the PTY never resizes. Desktop reuses the pane's cached
  `TerminalHeaderView`. A borderless float has no border row, so there the header shows over the
  content's top edge while pointed at.
- **Agents and browsers** draw their own header and fill the card. While floating they round their
  corners to `pip_content_radius`, the pane radius inside the card's border, so their opaque fill
  lines up with the card's rounded border under glass themes instead of leaving a crescent of glass
  at each corner.

Only the title drags, as on a tiled pane. `PaneDragSource::floating` makes it carry a `PipGrab`
instead of a `PaneDrag`, so the tile drop zones ignore it and the client moves the PiP from the
first `DragMoveEvent<PipGrab>`.

A bordered PiP on the desktop keeps the side, bottom and bottom corner grips for `resize-pane`, 8 px
wide and straddling the card's edge; they block the pointer so a press half outside the card does
not reach the pane under it. A resize stops at `PipLayout::max_cells`, the largest PiP that sits a
margin inside the canvas and inside the window (`float_drag_within` keeps the edge the grip did not
move). `resize-pane` from the CLI can still make a float larger; the layout then pins it to the top
left so its header shows.

# Throw

`PipDrag` holds the drag in pixels: where it started, the pointer, and the frame origin, which
follows the pointer freely instead of stepping by cells. `PipThrow` keeps the last 100 ms of pointer
samples for the release speed.

On release the landing point is the frame's origin plus its speed run out at iOS's scroll
deceleration, 0.998 a millisecond, which is half a second of the speed (`projected`). The window
splits into a 3x3 grid of zones, and the zone holding the PiP's centre at that point picks the place
(`FloatPlace::zone` in `zz_client::floating`), so a flick from the middle reaches a corner and a slow
release takes the zone it is in. Nothing is drawn for the zones.

The release sends one `move-pane -t %N -P <place>` at once, so the daemon and every other client get
the final place immediately. The throwing client draws the PiP at that place from then on and hands
the release speed to `PaneMotion::fling`, so the card springs from where it was let go (see
[Motion](#motion)). Speed toward the place is capped at the spring's frequency times the distance,
which rules out overshoot; speed away from it swings the card out at most 128 px times the
frequency. `FloatPlace::cells` repeats `move-pane -P`'s arithmetic, and the client keeps that target
until the daemon's answer lands. Grabbing a PiP while it moves starts the new drag from where it is
drawn.

Let go of a PiP with more than half of it past the left or right edge and it stashes there
(`PipLanding::stash`); its place is still sent. A throw into the stash drops its outward speed, so
the card slides in without bouncing.

A PiP is drawn by `PipLayout::frame`, not at its raw cells. The pane canvas is larger than the
window's cell grid (headers, gaps, pixels left over), so cells alone left a PiP flush with one edge
and far from the other. `PipLayout` maps the daemon's range of offsets on each axis onto the canvas
inset by 12 px: the leftmost offset `move-pane -P` gives sits 12 px from the left, the rightmost
12 px from the right, the centre in the middle, and the zones a throw picks from are thirds of the
same canvas. With no canvas yet it uses the window's cells. A PiP never leaves the canvas: its place
along each axis is clamped to the range, the margin shrinks when there is less than two margins of
slack, and one taller or wider than the canvas pins to the top or left so its header shows. A
float taller than `move-pane -P` has room for gets a negative offset from the daemon, which is why
the clamp matters.

# Piles and stashing

`PipStage` takes each PiP's frame from `PipLayout` (or its drag or glide), front first, and draws it
somewhere else without telling tmux.

- **Piles.** A PiP that another one in front covers by half or more joins its pile. Each one behind
  rises until its header shows above the one in front of it, up to three deep; past that they wait
  behind the third. A pile that would leave the top of the canvas slides down into it. A taller PiP
  whose header already shows stays where it is. Selecting one raises it in tmux's z order, so the
  pile reorders.
- **Stashing.** When the window's active pane changes to a tiled pane, every PiP it had slides past
  the nearer side edge until 60 px of it show, content and all; a pile keeps its fan. Clicking that
  strip, or focusing the PiP any other way, brings it back. A new PiP, closing the focused pane, and
  switching windows stash nothing, so a subagent feed opened beside the pane you type in stays in
  view.

`PipStage` only places the targets; `PaneMotion` springs every change, and a held PiP follows the
pointer. The stash lives in each client, so a reload or another client shows the PiPs in place.

# Motion

`PaneMotion` (`crates/zz-ui/src/pane/motion.rs`) is how every pane gets from where it was drawn to
where it belongs, tiled or floating. Each pane is wrapped in `moving_pane`, which takes the bounds
layout gives it as a target and lays the pane out at a frame that springs toward it, one critically
damped spring per edge (stiffness 324, the throw's spring). A split, a close, `resize-pane -Z`, a
pane drop, Float and Tile back all animate, because the same pane id moves between the tiled tree
and the float layer. A new pane grows out of the edge it shares with the pane it split from, so the
divider slides; one with no such pane appears in place. A window resize, a divider drag and reduced
motion snap. A terminal that is moving holds its size report (`TerminalView::moving`) until it
settles, so the daemon sees one resize, not one a frame.

# Clients

Every tiled pane's header (terminal, agent, browser) has a Float button (`picture-in-picture`) after
Split bottom and Split right. It sends `break-pane -W -x 33% -y 33% -s %N`, then
`move-pane -t %N -P bottom-right` (`float_pane_commands` in `zz_client::floating`), so the pane
becomes a PiP a third of the window's size in the bottom-right corner. `prefix @` and the pane menu
keep tmux 3.8's size (half the width, a quarter of the height, or the pane's last float). In a PiP
the header has Tile back and Zoom instead (`pane_floats`). `prefix @`, the pane menu and the CLI
still work.

- **Desktop** (`crates/zz/src/workspace/view.rs`): a `PipGrab` move starts the drag
  (`on_pip_grab_move`), `FloatDragState.pip` carries the `PipDrag`, and `throw_pip` sends the
  command, flings `pane_motion` and keeps the landing until the command settles. `pip_stage` follows
  the window each render.
- **Web and iOS** (`clients/app/src/app/floating.rs`): the same through `PipState`, with
  `grab_pip` for the mouse and `touch_pip` for a touch on the title (`touch_pane_drag` hands a
  floating pane to it). The landing ends when the snapshot shows the float at its place, or a
  second later.
- **Storybook**: Workspace → Panes → PiP shows a terminal PiP, a borderless one, an agent pane and a
  subagent feed as PiPs, a stashed PiP and a pile of two. The `pip-opacity` knob sets
  `Theme::pip_opacity`. The theme creator's PiP section puts a
  terminal PiP and a feed PiP over tiled panes, so `pane.pip` can be tuned and inspected.

# Subagent feeds

When Claude Code or Codex starts subagents, zz floats a PiP over its pane that follows them. This
works for a zz Agent pane and for the `claude` or `codex` TUI in a terminal pane. The PiP is a
watch-only agent pane fed from files the agent already writes. The agent's own session, tools and
prompt cache are not touched.

`crates/zz-daemon/src/agent/feed.rs` is the vendor-neutral part: a `FeedSource` per vendor reports
each child's heading, new updates and phase (running, done, stopped), and `Feed` turns that into the
agent stream. `crates/zz-daemon/src/daemon/subagent_feeds.rs` finds the parents and owns the floats:
a `zz-subagents` worker thread wakes every second while a Claude or Codex pane exists, and posts what
it found to the loop.

## Claude Code

- **Finding them:** a Claude Agent pane gives its session id from the runtime's wire state. A
  terminal pane running `claude` gives it through the peer record
  (`claude_peers::record_for_pane`). The worker finds `<projects>/<slug>/<session>/subagents/` and
  lists the subagents that are running.
- **Running or done** (`crates/zz-daemon/src/agent/claude/subagents.rs`, `Completions`): the parent
  transcript decides, read incrementally from its last 8 MiB.
  - A background subagent is done when its `<task-notification>` arrives with a status:
    `completed` is done, any other status is stopped.
  - A foreground one is done when its Agent call gets a tool_result.
  - `finished` or `stoppedByUser` in `agent-<id>.meta.json`, and an interrupt line, settle it at
    once.
  - When the parent transcript can't be read, an `end_turn` line is the fallback. A subagent that
    ends its turn while its own background shell runs is still running.
- **Lines:** each `agent-<id>.jsonl` line runs through the Claude translator
  (`Translator::sidechain_entry`), so steps, thoughts and answers render like any Claude turn. The
  heading is `<type> · <description>` from `agent-<id>.meta.json`.
- **Stop:** `HostCommand::StopTask` to the parent, which becomes Claude's `stop_task` with the agent
  id.

## Codex

- **Finding them** (`crates/zz-daemon/src/agent/codex/children.rs`): Codex writes every thread to
  `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*-<thread>.jsonl`. A child's first line is its
  `session_meta`: `parent_thread_id`, `session_id` (the root thread), `agent_path`,
  `agent_nickname`, `agent_role` and `subagent_history_start_ordinal`. `running_children` reads the
  newest two day folders and groups the running children by root thread.
- **Parents:** a Codex Agent pane gives its thread id from the runtime's wire state. The `codex` TUI
  can't be asked for its thread, so a terminal pane whose process tree runs a `codex` binary
  (`pane_runs_codex`) takes the root whose `cwd` matches its own, when it is the only codex TUI in
  that folder and no Agent pane owns that root.
- **Running or done:** the child's own `task_started`, `task_complete` and `turn_aborted` lines after
  its start ordinal.
- **Lines:** the rollout's `item_completed` items (commands, messages, reasoning) are reshaped into
  app-server items and run through the Codex translator (`Translator::history_item`). The heading is
  `<role or nickname> · <agent path>`, for example `Pauli · daemon reader a`.
- **Stop:** `HostCommand::InterruptThread` to the parent, which sends Codex's `turn/interrupt` with
  the child's thread and running turn on the pane's app-server connection.

## Both

- **The float:** `new-pane -d --kind agent -T Subagents` at the parent's top-right corner, 64x18
  outer cells, with pane option `@zz-subagents=%<parent>`.
  - `agent_pane_spec` reads that option and hands the pane a `FeedSpec`. `AgentHost::open` then runs
    `run_subagent_feed` instead of an agent process.
  - The float closes 5 s after the last subagent finishes, and when its parent goes.
  - A float you close stays closed until a new subagent starts.
  - Feed floats the daemon no longer tracks, for example after a restart, are closed.
- **The feed** (`Feed`): one turn per subagent.
  - Each turn is headed by a user bubble with the child's heading, then its lines.
  - Turns stay whole: finished subagents come first, in the order they finished, and the subagent
    being followed is last, so its updates stream into the live turn.
  - When a subagent finishes while another is followed, the feed replays its timeline as
    `SessionSwitched` so the finished turn lands above the live one. It then re-sends
    `Activity { busy }` so the live turn stays live.
  - The subagents still running are the pane's task tray (`TasksChanged`), and `Activity` drives the
    Running status. A terminal parent has no channel to stop through, so its feed sends no tray.
- **Watch-only:** the feed's Ready carries `AgentSessionCapabilities { watch: true }`. It is JSON on
  the agent stream, appended in wire v109 ([wire protocol](/protocol/wire-protocol.md)). Desktop and web/iOS draw the task tray and errors in place
  of the composer. A prompt that reaches the feed anyway is refused.
- **Feeds stay off the peer bus.** They can't take a prompt.

Verified live on 2026-10-18 against a throwaway daemon and the web client:
- Claude Code 2.1.302 on Haiku: a `claude` TUI in a terminal pane and a Claude Agent pane each
  started subagents, and the float opened at the parent's corner. It showed each subagent's heading,
  a live trace with its latest thought, its answer, and the running count. It kept a finished turn
  above the live one, and closed 5 s after the last subagent finished. Stop on a subagent stopped it.
- Codex on GPT-6-Luna: a Codex Agent pane and a `codex` TUI in a terminal pane each spawned two
  children, and each got its own float with the children's commands. Stop on one child of the Agent
  pane wrote `turn_aborted` to its rollout and dropped it from the tray while the other kept going.

# Not done

- Two `codex` TUIs in the same folder get no feed, since nothing outside Codex says which root is
  whose.
- A stashed PiP that was thrown off the edge keeps tmux's focus, so keys still reach it.
- iOS native browser views follow the snapshot place, so a stashed browser PiP's page stays in view
  on iPad.
- iOS native browser views are hidden under floats by their snapshot place, not the place drawn
  during a drag or while the PiP moves.
- The web client draws floats in window cells. When the window is larger than the canvas (a
  manually sized window), a float lands off the tiled panes it belongs to.

---
type: Design Plan
title: PiP panes . every float, thrown to one of nine places
description: "In the GUI clients every floating pane that is not modal draws as a PiP: the pane in a card that casts the float shadow, with the same header as a tiled pane, tile back and zoom in place of the splits. Drag the title to throw it, iOS style, onto one of the nine places move-pane -P names. PiPs on one place fan out so each header shows, and they slide off the side edge, 60 px left showing, while a tiled pane has focus. Underneath it stays a tmux 3.8 float with no new flag, state or wire."
status: Built (2026-10-18; desktop, web and iPad; the Claude and Codex subagent feeds were removed 2026-10-23 with the Agent pane)
resource: crates/zz-ui/src/pip.rs
tags:
- ui
- floating-panes
- design-plan
timestamp: 2026-10-26T00:00:00-03:00
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

`PipSurface` (`crates/zz-ui/src/pip.rs`) is the card: the pane's colour (`zz_ui::pane_look::pane_color`, a
terminal's background over the window) laid at `pip-background-opacity` (the
`zz_ui::pip::PipOpacity` global, 80% when unset) over a frost of what is under it
(`pip_material`: the theme's glass in a glass theme, a 24 px blur otherwise; a renderer that
cannot read the backdrop paints the colour solid), corners of
`Theme::pane_radius`, and the `pane.pip`
role under `pane.float`, which casts the theme's float shadow (Materials in the theme creator; Roles
→ PiPs can set none, contact or float). Focus does not change it. The dark palettes draw shadows in
20% black, so on a dark pane the default reads faintly.

The header is the pane's own header, the one a tiled pane draws (`pane_header` and
`terminal_pane_header`, the browser's `BrowserHeader`), with
`pane_header_actions(floats)` swapping Split bottom, Split right and Float for Tile back
(`join-pane -s %N -t %N`) and Zoom (`resize-pane -Z`). The title keeps its X and a browser keeps
its tabs on the one header row. (The Agent pane header, with its rename, was removed on
2026-10-23; a terminal running Claude Code or Codex keeps the terminal header and its lens toggle,
see [agent lens](/designs/agent-lens.md).) `TerminalPaneAction::commands` holds
the commands every client sends.

- **Terminals** (and pickers and editors, which have no header of their own) get the header in the
  float's top border cell, `PipSurface::header`, at the theme's pane header height, and the content
  under it inset by the border cells, so the PTY never resizes. Desktop reuses the pane's cached
  `TerminalHeaderView`. A borderless float has no border row, so there the header shows over the
  content's top edge while pointed at.
- **Browsers** draw their own header and fill the card (Agent panes did too until they were removed
  on 2026-10-23). While floating they round their
  corners to `pip_content_radius`, the pane radius inside the card's border, so their opaque fill
  lines up with the card's rounded border under glass themes instead of leaving a crescent of glass
  at each corner.

Only the title drags, as on a tiled pane. `PaneDragSource::floating` makes it carry a `PipGrab`
instead of a `PaneDrag`, so the tile drop zones ignore it and the client moves the PiP from the
first `DragMoveEvent<PipGrab>`.

A bordered PiP on the desktop has a grip on every edge and corner for `resize-pane`, 8 px wide and
straddling the card's edge, the top one over the header's first 4 px; they block the pointer so a
press half outside the card does not reach the pane under it. A resize stops at `PipLayout::max_cells`, the largest PiP that sits a
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
  switching windows stash nothing, so a PiP opened beside the pane you type in stays in view.

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

Every tiled pane's header (terminal, browser) has a Float button (`picture-in-picture`) after
Split bottom and Split right. It sends `break-pane -W -x 33% -y <rows> -s %N`, then
`move-pane -t %N -P bottom-right` (`float_pane_commands` in `zz_client::floating`), so the pane
becomes a PiP a third of the window wide on its right side and as tall as the window fits: `<rows>`
is `PipLayout::fill_rows`, the tallest PiP that sits a margin inside the canvas with its header,
which each client measures (`pip_fill_rows`), since the daemon knows only cells. `prefix @` and the pane menu
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
- **Storybook**: Workspace → Panes → PiP shows a terminal PiP, a borderless one, a stashed PiP and
  a pile of two. The `pip-opacity` knob sets `zz_ui::pip::PipOpacity`. The theme creator's PiP section
  puts a terminal PiP over tiled panes, so `pane.pip` can be tuned and inspected. As of 2026-10-23
  both stories still also draw the removed agent pane and subagent feed as PiPs; the storybook is
  due a lens story in their place ([agent lens](/designs/agent-lens.md)).

# Subagent feeds (removed 2026-10-23)

From 2026-10-18 to 2026-10-23, when Claude Code or Codex started subagents, the daemon floated a
watch-only agent pane over the parent (`new-pane -d --kind agent -T Subagents`, pane option
`@zz-subagents=%<parent>`) and fed it from the files the CLI writes: Claude's
`<session>/subagents/agent-<id>.jsonl` and Codex's child rollouts under `$CODEX_HOME/sessions/`.
The float closed 5 s after the last subagent finished, and its task tray could stop a child. It
was a watch-only agent pane, so it went with the Agent pane kind in e0608066e: `agent/feed.rs`,
`agent/claude/subagents.rs`, `agent/codex/children.rs` and `daemon/subagent_feeds.rs` are
deleted, and the `watch` flag on `AgentSessionCapabilities` (wire v109) is no longer set by
anything. The Codex rollout reader the lens uses now lives in `agent/codex/rollout.rs`. Showing
subagent activity inside the lens is a follow-up in the [agent lens](/designs/agent-lens.md)
design.

# Not done

- A stashed PiP that was thrown off the edge keeps tmux's focus, so keys still reach it.
- iOS native browser views follow the snapshot place, so a stashed browser PiP's page stays in view
  on iPad.
- iOS native browser views are hidden under floats by their snapshot place, not the place drawn
  during a drag or while the PiP moves.
- The web client draws floats in window cells. When the window is larger than the canvas (a
  manually sized window), a float lands off the tiled panes it belongs to.

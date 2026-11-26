---
type: Concept
title: Terminal frame (TerminalViewport)
description: The renderer-neutral terminal viewport and how clients safely apply row patches.
resource: crates/zz-terminal/src/model.rs
tags: [terminal, viewport, packed-cell, patch]
timestamp: 2026-11-26T00:00:00-03:00
---

## Schema

`TerminalViewport` is an immutable snapshot for one terminal view. Its model
lives in `crates/zz-terminal/src/model.rs`.

| Fields | Meaning |
|---|---|
| `generation`, `view_generation`, `dictionary_generation` | Identify content, view changes and the epoch for dictionary references |
| `columns`, `rows`, `cells`, `dictionary` | Row-major packed cells reference interned styles and graphemes. A cell takes eight bytes. |
| `foreground`, `background`, `presentation` | Default colors, title and hovered URI |
| `overlays`, `kitty_placements`, `cursor` | Selection, search and link spans, Kitty graphics placements and cursor state |
| `scrollbar`, `mode`, `search`, `unseen_output` | Position in the live, copy or view screen, active search and output since pinning |
| `kitty_keyboard`, `mouse_tracking`, `status` | Input modes and terminal process state |

## Publication

The actor snapshots libghostty-vt's visible cells, interns styles and graphemes,
and publishes immutable frames for streaming views. A view can have a different
scroll anchor, selection or copy mode from its neighbor. The snapshot can be
clipped to the display area required by consumers while the PTY retains its
assigned dimensions. A dictionary rollover forces a new full viewport.
See `build_snapshot` in `crates/zz-terminal/src/session/snapshot.rs` and
`TerminalSession::latest_viewport_for` in `crates/zz-terminal/src/session/handle.rs`. [PTY worker](/concepts/pty-worker.md)

With synchronized output enabled (DEC mode 2026), the worker keeps parsing PTY bytes
while clients see the previous frame. Clearing the mode, resizing or the one-second
deadline releases the pending frame. See `defer_synchronized_output` in
`crates/zz-terminal/src/session/frames.rs`.

## Patches and repair

`TerminalViewport::diff_shared` compares a retained base and a new viewport.
A patch can shift rows, replace changed spans, append dictionary entries and
update metadata. The span's `clear` flag empties the rest of its row. Dimension
or dictionary incompatibility requires a full viewport. `apply_patch` validates
generations, dimensions, bounds and references before mutating the retained
frame. A rejected patch leaves the last frame renderable. The client can request
a full viewport for the affected pane.

The daemon encodes the result on [terminal lanes](/protocol/terminal-lanes.md).
The client retains the grid for drawing and keeps scrolled-off rows in
`HistoryRing` in `crates/zz-client/src/scrollback.rs`. [Terminal rendering](/terminal/rendering-parity.md)

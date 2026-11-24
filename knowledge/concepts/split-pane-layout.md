---
type: Concept
title: Cell-authoritative split-pane layout
description: How the mux measures tiled and floating panes, identifies dividers, and restores layout strings.
resource: crates/zz-mux/src/layout.rs
tags: [layout, splits, panes, cells, tmux]
timestamp: 2026-11-24T00:00:00Z
---

## Cell layout

Each window owns a `CellLayout`. Tiled leaves and split branches carry cell
dimensions and offsets. A floating node carries independent geometry. On a tiled
branch's split axis, child extents and the one-cell borders between them fill
the parent. The children match its perpendicular extent. Tiled leaves have a
one-cell minimum. A child can carry a stable `SplitId` for its leading divider.
See `CellNode`, `CellChild` and `CellLayout::validate` in `crates/zz-mux/src/layout.rs`.

A split reserves one border cell and refuses a target too small for two leaves.
`split-window -l` and `-p` use the target size, or the window for a full-size
split. Removing a tiled leaf gives its space to a sibling and collapses a parent
with one child. Resizing preserves minimum sizes. Presets redistribute the tiled
space. Float position, dimensions and stacking are managed separately.
[Floating panes](/designs/floating-panes.md) · [tmux divergences](/tmux/divergences.md)

## Snapshots and drag handles

`CellLayout::project` folds the tiled tree into the binary `LayoutNode` the
clients draw. Dividers keep their IDs across unrelated edits. A drag sends a
divider ID and ratio. `set_divider_ratio` converts it to a cell boundary.
The snapshot also describes the floats so a client can draw them over the tiles.
[Snapshots](/protocol/snapshots.md)

## Layout strings

The mux reads tmux's checksummed v1 and JSON v2 layout strings. v2 can encode
pane indices, the active and last panes, and floating z-order. Applying v1
restores the tiled layout and links existing floats back into it. v2 applies
its selection and stacking information. If there are extra cells, the parser
trims from the bottom right before checking sizes. Too few cells for the
target window's panes produces an error. See `Window::layout_string` and
`MuxState::select_layout_string` in `crates/zz-mux/src/model.rs`.

## Window extent

A window starts with a cell extent from the session or its configured size.
Clients report pane geometry, and the daemon selects a size according to
`window-size` and `aggressive-resize`. Zoom and float placement affect which
measurements describe the tiled window. The daemon then sizes the PTY to its
assigned geometry. [Session persistence](/concepts/session-persistence.md)

The layout fixture suite in `crates/zz-mux/src/layout_pin_tests.rs` exercises
the cell algorithms against captured layouts. `compat/gen-layout-fixtures.sh`
regenerates its fixtures. [Compatibility harness](/playbooks/compat-harness.md)

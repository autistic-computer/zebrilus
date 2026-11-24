---
type: Rust Crate
title: zz-mux crate
description: The mux model, cell layout, tmux command execution, target resolution, config parser, formats and change journal.
resource: crates/zz-mux/src/lib.rs
tags: [mux, tmux, state-machine, crate, layout]
timestamp: 2026-11-24T00:00:00-03:00
---

# From a command to a visible change

`MuxEngine` interprets commands against `MuxState` and returns an `Execution` with
`MuxEffect` values. For example, a split allocates a pane and updates the cell layout.
The [daemon](/crates/zz-daemon.md) creates the corresponding terminal or agent runtime
and publishes the new tree. Attach, evict, process I/O and clipboard operations also need
the daemon to act on an effect. Catalog entries and key table types are shared with
[zz-protocol](/crates/zz-protocol.md).

| Source under `crates/zz-mux/src` | Owns |
| --- | --- |
| `command.rs` | `MuxEngine`, `ExecutionContext`, command handlers, options and effects |
| `model.rs` | `MuxState`, sessions, windows, panes, target lookup, IDs and `snapshot` |
| `layout.rs` | `CellLayout`, cell dimensions and positions, dividing and moving panes, layout strings |
| `journal.rs` | `open_change_window` and `changes_since` for recording mutations that feed publication |
| `parser.rs` | tmux configuration syntax, expansions, command groups and diagnostics |
| `formats.rs`, `status.rs` | Format expansion, client/window facts, status formats and `StatusHooks` |

A window has pane order and z-order, an active pane, optional zoom, a tiled layout and
separate floating/modal records. Its internal `PaneKind` variants are Picker, Terminal,
Browser, Editor, Agent, View and Simulator. The picker holds a place in the layout before
the user chooses a runtime. Simulator projects to `PaneKindSnapshot::Other` with a gated
descriptor event. The model's cell tree projects to the binary `LayoutNode` tree with stable
divider IDs that clients use to resize splits. `MuxState::validate` checks model invariants.

See [commands and effects](/tmux/commands.md), [config syntax](/tmux/conf-parser.md),
[key tables](/tmux/key-tables.md), [status formats](/tmux/status-line.md),
[split-pane layout](/concepts/split-pane-layout.md) and [snapshot projection](/protocol/snapshots.md).
Use the [tmux compatibility guide](/tmux/tmux-compat.md) for the pinned reference and supported behavior.

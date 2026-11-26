---
type: Rust Crate
title: zz-mux crate
description: The mux model, cell layout, tmux command execution, target resolution, config parser, formats and change journal.
resource: crates/zz-mux/src/lib.rs
tags: [mux, tmux, state-machine, crate, layout]
timestamp: 2026-11-26T00:00:00-03:00
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
| `command.rs` | `MuxEngine` and its fields, `ExecutionContext`, `Execution`, `MuxEffect` and the other shared types, the dispatch `match` from a command name to its handler, alias resolution, target and format helpers, `source-file`, `TMUX_OPTION_CONSUMERS` and the format context tables |
| `command/*.rs` | One file per subject, most with an `impl MuxEngine` block. The table below lists them |
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

# Command files

Paths below are relative to `crates/zz-mux/src/command`. The [command set](/tmux/commands.md)
maps every command name to the file that runs it.

| File | Holds |
| --- | --- |
| `sessions.rs` | `new-session`, `list-sessions`, `rename-session`, `kill-session`, `attach-session`, `has-session`, `suspend-client`, `detach-client`, `server-access` |
| `windows.rs` | `new-window`, `list-windows`, `rename-window`, `select-window`, `next-window`, `previous-window`, `last-window`, `kill-window`, `move-window`, `swap-window`, `find-window`, `resize-window`, window extents and pane geometry |
| `pane_spawn.rs` | `split-window`, `new-pane`, `select-pane-kind`, `break-pane`, `join-pane`, `move-pane`, `view-pane`, `set-browser-*`, `set-editor-path`, `set-simulator-device`, floats, agent spawning and pane start commands |
| `panes.rs` | `select-pane`, `last-pane`, `swap-pane`, `list-panes`, `resize-pane`, `kill-pane`, `respawn-pane`, `respawn-window`, `select-layout`, `next-layout`, `previous-layout`, `rotate-window`, mouse resize |
| `send_keys.rs` | `send-keys`, `send-prefix`, copy mode action parsing |
| `modes.rs` | `copy-mode`, `copy-mode-search-prompt`, `clock-mode`, `switch-mode`, `choose-tree`, `choose-client`, `choose-buffer`, `choose-path`, `focus-sidebar`, `display-panes`, `clear-history` |
| `customize.rs`, `mode_prompt.rs`, `panes_mode.rs`, `switch_mode.rs` | `customize-mode` and the state of the customize, prompt, pane and switch modes |
| `messages_and_prompts.rs` | `display-message`, `command-prompt` |
| `key_bindings.rs` | `bind-key`, `unbind-key`, `list-keys`, `list-commands`, tmux key parsing |
| `hooks.rs` | `set-hook`, `show-hooks`, format monitors, hook firing |
| `set_option.rs` | `set-option`, `set-window-option` and the per-scope setters |
| `show_options.rs` | `show-options`, `show-window-options`, option readback and printing |
| `option_store.rs` | Scalar and array option tables, user option storage and the store types |
| `option_lookup.rs` | Option accessors other crates read and option value parsers |
| `option_formats.rs` | Option values for format expansion and the format option snapshot |
| `environment.rs` | `set-environment`, `show-environment`, `update-environment` patterns and job environments |
| `callbacks.rs` | Config parsing, command block preparation, static validation and command printing |
| `runtime_facts.rs` | Pane runtime facts, automatic rename, dead panes, agent chat facts and `format_facts` |
| `arguments.rs` | Command option parsing, positional checks, number and resize parsing |
| `signals.rs` | Signal names, including the macOS lookup |
| `execution_context.rs` | `ExecutionContext` methods and mouse target resolution |
| `status_hook_adapters.rs` | The `StatusHooks` impls for row, option, config condition, command item and key listing formats |

Tests sit beside the code as `command/<subject>_tests.rs`, with shared helpers in
`command/test_support.rs`.

See [commands and effects](/tmux/commands.md), [config syntax](/tmux/conf-parser.md),
[key tables](/tmux/key-tables.md), [status formats](/tmux/status-line.md),
[split-pane layout](/concepts/split-pane-layout.md) and [snapshot projection](/protocol/snapshots.md).
Use the [tmux compatibility guide](/tmux/tmux-compat.md) for the pinned reference and supported behavior.

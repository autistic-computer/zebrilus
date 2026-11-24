---
type: Design Plan
title: "Shell path picker and which-key"
description: "The host daemon lists paths for the command palette. The which-key sheet shows bindings for the active key table."
status: "Implemented, verified against source 2026-11-24"
resource: crates/zz-daemon/src/daemon/path_listing.rs
tags:
- picker
- which-key
- key-tables
- overlays
- protocol
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

# Insert a path

`prefix F` runs `choose-path` and opens the [command palette’s file picker](/concepts/command-palette.md#files) at the pane's directory. The same picker is available through the `insert path` verb. Tab enters a directory, Shift-Tab returns to its parent, and typing filters the list. Enter inserts into the target terminal, or copies when there is no terminal to paste into. Alt-Enter copies the plain path and keeps the picker open.

The daemon that owns the pane resolves the requested root, reads the directory and supplies git marks, the current directory and an insertion style. It streams the listing to the requesting client so filtering can happen locally. The walk respects ignore files, lists symlinks without entering them, and has time, depth and entry limits. A superseded request can be cancelled. The walker, limits and tests live in `crates/zz-daemon/src/daemon/path_listing.rs`.

The client ranks the entries with `zz-client/src/path_rank.rs`. For a path on the target host, `zz-client/src/path_insert.rs` chooses its relative or absolute form and quotes it for the shell or agent. On desktop, selecting a file from another host produces a host-qualified path. Desktop and the shared thin app implement the palette backend in `crates/zz/src/command/palette.rs` and `clients/app/src/command_palette.rs`. `OpenPathPicker`, `PathListBegin` and `PathListChunk` are defined in `crates/zz-protocol/src/message.rs`.

# Which-key

When the prefix or a custom table is active, the client can display the bindings after `which-key-delay`. The default is 400 ms. Setting it to zero disables the delayed sheet. `KeyTableActive` tells the client which table the daemon selected. The sheet reads that table's bindings and notes, so changes to bindings appear in the hints.

`crates/zz-client/src/prefix_keys.rs` owns the delay and prefix hints. The desktop and wide thin app build `zz_ui::which_key::WhichKeySheet` from the active key tables. Thin clients store the delay in their device preferences. See [key tables](/tmux/key-tables.md) for dispatch and repeat behavior.

# History

Protocol v107 introduced both features. The first path picker appeared under the terminal cursor. In October 2026 the picker moved into the palette, briefly used `@` and then `&`, and settled on the `insert path` verb. The original plan and its deferred ideas are available with `git show 6470d3324:knowledge/designs/path-picker-and-which-key.md`.

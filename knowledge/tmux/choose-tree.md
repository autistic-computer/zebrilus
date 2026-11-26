---
type: Concept
title: Sidebar navigation and native choosers
description: How tree, client and buffer choosers build rows, handle selection templates and appear on desktop or in the TUI.
resource: crates/zz-mux/src/command/modes.rs
tags: [tmux, choose-tree, choose-buffer, chooser, overlay]
timestamp: 2026-11-26T00:00:00-03:00
---

# Pick a target

`choose-tree`, `choose-client` and `choose-buffer` open a chooser for the invoking client. The daemon owns its rows, filter, search, selection and actions. A client can draw those rows in its own UI. `focus-sidebar` focuses the persistent desktop navigation tree. Use a chooser when a binding needs tmux selection or a command template. The [desktop client](/crates/zz.md) also offers [chat mode](/designs/chat-mode.md) for switching between conversations.

| Command | Initial view and action |
| --- | --- |
| `choose-tree` | Show the session/window/pane hierarchy. Enter activates the selected target. |
| `choose-tree -s` or `-w` | Start with session rows collapsed (`-s`) or windows collapsed under their sessions (`-w`). Both retain the full hierarchy. The desktop uses its navigation palette for these forms. |
| `choose-client` | Show attached client rows. `-i` requests the client information preview. |
| `choose-buffer` | Show global paste buffers, newest first by default. Enter pastes the selected buffer. Empty stores produce no chooser. |

# Rows and previews

`-t` selects the source pane. Use `-f` to filter, `-F` to format labels, `-O` to sort, `-r` to reverse the order and `-K` for a row's shortcut. Tree filtering evaluates pane context and keeps the matching ancestors. When no row matches, the client shows `filter: no matches` and the original rows remain selectable. Each chooser has its own accepted flags. Check `zz choose-buffer --help` or the corresponding catalog entry for the desired command.

A single `-N` starts with the preview off. Two (`-NN`) select the large preview in the TUI. `crates/zz-mux/src/command/modes.rs` (`choose_tree`, `choose_client`, `choose_buffer`, `chooser_preview`) builds the request. The daemon's `chooser_presentation.rs` expands the visible rows and the TUI renderer lives in `crates/zz-tui/src/render/chooser.rs`.

# Selection command templates

A template replaces the default action. A tree row supplies an exact target such as `=work:`, `=work:2.` or `=work:2.%7`. A client row supplies the client name, and a buffer supplies its name. These are substituted into the template and the command runs in the invoking client's live context. Include `-t` in the template when the action should target the chosen item.

The first `%%` and every matching numbered placeholder (`%1` for a single selection) receive the value. The quoting rules live in `command_template_replace` in `crates/zz-mux/src/command/callbacks.rs`. A typed command block resolves aliases when it is prepared. A string is parsed at selection time. An empty template closes without an action, as does selecting a buffer that has disappeared.

`compat/scenarios/smoke/args-parse-choosers.txt` covers template construction. `compat/tui-choosers.sh` exercises attached behavior. For key rebinding or the desktop adapter, see [key tables](/tmux/key-tables.md) and the [command palette](/concepts/command-palette.md).

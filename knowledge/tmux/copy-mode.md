---
type: Concept
title: Copy mode and view mode
description: How a client navigates frozen pane history, copies text and handles output views and application clipboard writes.
resource: crates/zz-terminal/src/session/mode_revision.rs
tags: [tmux, copy-mode, view-mode, selection, search]
timestamp: 2026-11-26T00:00:00-03:00
---

# Entry, mode keys and search

`copy-mode` (default prefix then `[`) gives the invoking client a frozen revision of the target pane's grid and scrollback. The [terminal engine](/crates/zz-terminal.md) keeps processing PTY output. See [terminal interaction](/terminal/interaction.md) for ordinary pointer selection and paste. Another attached client can keep its own position and selection. `copy-mode -u` enters and pages up, `-t` selects the pane and `copy-mode -q` cancels. Mouse bindings can also enter copy mode with `-M` when they have a pane target.

The window's `mode-keys` selects the emacs `copy-mode` table or `copy-mode-vi`. Use `zz list-keys -T copy-mode-vi` for the active bindings. The daemon's `enter_copy_session` (`crates/zz-daemon/src/daemon/copy_mode.rs`), `retarget_copy_mode_tables` (`daemon/pane_modes.rs`) and `reconcile_copy_session` (`daemon/publication.rs`) keep the client's key table and terminal view together. In the stock vi table, Escape clears the selection and `q` exits. See [key tables](/tmux/key-tables.md).

# Actions and selection text

`send-keys -X` acts on the mode. Common actions include `begin-selection`, `cursor-right`, `next-word`, `rectangle-toggle`, `search-forward` and `copy-selection-and-cancel`. The inventory in `crates/zz-mux/src/copy_actions.rs` records the pinned names and their read-only classification. The `copy_mode_action` handler in `crates/zz-mux/src/command/send_keys.rs` validates arguments and produces terminal view effects. The client's pending jump key and vi numeric prefix are resolved before the terminal actor receives the action.

`ModeRevision::format_selection` in `crates/zz-terminal/src/session/mode_revision.rs` reads the frozen cells. It preserves spaces inside the selected text, joins a continuing soft wrap, and skips the spacer cells of a wide glyph. Rectangular selections keep their columns. The emacs and vi tables differ on including the focus cell. The destination and whether to exit depend on the selected action. Copy-and-cancel can finish selection and return input to the live pane in one step.

## Scroll exit

`copy-mode -e` lets a downward scroll leave copy mode on reaching the bottom when there is no live selection. Cursor movement alone keeps the mode open. The actions `scroll-exit-on`, `scroll-exit-off` and `scroll-exit-toggle` change this setting during navigation. See `copy_mode_scroll_exit_ready` in `crates/zz-terminal/src/session.rs`.

# TUI command-output navigation

Interactive command output also has a retained view in the [raw terminal client](/designs/tui-client.md). It uses copy-table navigation and search while keeping the command's output separate from the application's PTY. `compat/attached-client.sh` exercises movement, search, custom bindings and copying a selected match into a paste buffer. `compat/tui-copy-mode.sh` tests pane copy mode through actual key input. Read obligation TUI-005 in the [TUI report](/tmux/tui-parity.md) for the recorded proof.

# App-initiated clipboard writes (OSC 52)

An application can send OSC 52 without entering copy mode. `clipboard_write_request` in `crates/zz-terminal/src/session/publisher.rs` prefers a text/plain representation and enforces an 8 MiB limit. The daemon's `deliver_clipboard_write` accepts an application's write only when `set-clipboard` is `on`. It then creates an automatic paste buffer, publishes the clipboard event and fires `pane-set-clipboard`. Both `external` and `off` discard an application's request. A server-initiated copy has its own path and can use the external clipboard with `external`.

---
type: Concept
title: tmux status rows and format expansion
description: Where tmux status formats get their values, how the raw TUI draws the rows, and which settings control the desktop bar.
resource: crates/zz-mux/src/formats.rs
tags: [tmux, status-line, formats, gui, tui, options]
timestamp: 2026-11-24T00:00:00-03:00
---

# Who expands the format

The daemon expands status formats in the recipient's context. Two clients can see different session names, active windows, dimensions or job output. The mux's `FormatContext` supplies model values. `StatusHooks` in `crates/zz-mux/src/formats.rs` lets the daemon supply facts such as terminal state, paste buffers, client rows and shell job output. A variable provided by a loop, chooser row or hook is meaningful in that producer's context.

Use `zz display-message -p '#{session_name}:#{window_index}.#{pane_index}'` for an individual value and `zz display-message -a` to inspect available values. The evaluator supports short names such as `#S`, nested variables, conditions, comparisons, matching, substitutions, time formats and loops. For example, `#{?pane_active,active,idle}` chooses a label. The pinned vocabulary and modifier spellings live in `compat/tmux-oracle.json`. Use the [format gap lookup](/tmux/divergences.md#format-variables-that-remain-unbacked) for a registered placeholder or limit.

# TUI rows

`StatusRenderer` in `crates/zz-daemon/src/status.rs` builds a `StatusLine` for each client. The initial state and subsequent `StatusChanged` events carry the expanded rows, style, placement and selected message row. `compose_status_row` in `crates/zz-client/src/status.rs` lays out style runs, alignment, truncation and mouse hit ranges. The raw TUI reserves those cells and draws prompts or messages on the configured row.

| Option | Visible result in the TUI |
| --- | --- |
| `status`, `status-position` | Set the number and position of rows. Turning status off returns those rows to the pane canvas. |
| `status-format[]` | Supplies row templates. Sparse indexes retain intervening blank rows. A session-specific array overrides the global array. |
| `status-left`, `status-right`, `window-status-*-format` | Supply content consumed by the row template. For example, window formats run with each window's values. |
| `status-style`, `message-line` | Supply the base appearance and where to place a prompt or message. |
| `status-interval` | Sets periodic expansion. Zero disables that timer. State changes and job output can still trigger updates. |

A `#(command)` job runs on the daemon's host. The renderer can use cached output while it runs. An initial expansion can be empty, and later output wakes the status path. For this code follow `DaemonFormatHooks` in `crates/zz-daemon/src/status.rs` and `crates/zz-daemon/src/daemon/status_jobs.rs`. For visible proof use `compat/status-row.sh`, which reads the rendered row of an attached client.

# Desktop status bar

The [native status bar](/designs/native-status-bar.md) builds its items from session and window snapshots, host and update data, and app settings. Its settled workspace shows the bar in titlebar mode and hides it when the sidebar is open. A tmux theme that only writes status options changes the TUI. To change the desktop items, use zz's appearance settings. See `StatusBarModel` in `crates/zz-client/src/status_bar.rs`, `render_gui_status_bar` in `crates/zz/src/status_bar.rs` and placement in `crates/zz/src/app_shell.rs`. The registry records the choice under `presentation.native-status`.

---
type: Rust Crate
title: zz-client crate
description: Renderer-independent client state, input routing, scrollback, layout and presentation rules shared by zz clients.
resource: crates/zz-client/src/lib.rs
tags: [client, core, sans-io, keybindings, crate]
timestamp: 2026-11-27T00:00:00-03:00
---

# Client state and effects

`ClientCore` in `crates/zz-client/src/core.rs` consumes protocol messages. It retains the
shared tree and private client view, current attachment, options, key tables, terminal
viewports and agent state. The shell drains `CoreEvent` for presentation and local work and
`Outbound` for requests the reducer needs to send. Seeding or replacing a connection with
a hello establishes its server identity, capabilities and clock.

A tree delta with a bad base queues one `TreeSync`. A terminal patch without a matching
viewport queues `RequestFull(pane)`. Repair is scoped to what failed. The desktop
`MuxClient` handles terminal and command-output frames on its retained paint path to keep
row revisions and history in one place. See [snapshots](/protocol/snapshots.md),
[terminal frames](/protocol/terminal-lanes.md) and [the desktop client](/crates/zz.md).

| Source under `crates/zz-client/src` | Shared behavior |
| --- | --- |
| `scrollback.rs`, `local_scroll.rs` | `RetainedTerminalViewport`, `HistoryRing`, patch application, local pixel scrolling and history prefetch. `HistoryPacer` tracks pending requests per pane, retries after 3 s, and defers background backfill until 100 ms of quiet. |
| `input.rs`, `chrome.rs`, `chrome_override.rs` | Input ownership and effects, client key tables and profiles, `chrome-keybind` and `chrome-unbind` overrides |
| `layout.rs`, `floating.rs` | Split geometry, hit tests, drop targets and floating-pane placement |
| `window_summary.rs`, `sidebar.rs`, `status_bar.rs` | Window summaries, attention and age from daemon time, and status/sidebar presentation |
| `menu.rs`, `chooser.rs`, `palette.rs` | Shared overlay actions, navigation and command matching |
| `agent_lens.rs`, `agent_timeline.rs`, `chat_rows.rs` | Agent state and timeline projection into rows for the shell to draw |

`core.rs` holds `ClientCore` and all of its code. Its tests sit beside it under `core/`:

| File | Tests |
| --- | --- |
| `tree_tests.rs` | Compact batches, client views, tree deltas and their repair |
| `attachment_tests.rs` | Attachment options, detach and reconnect, the welcome clock and timed messages |
| `agent_tests.rs` | Agent state, attention edges, stream payloads and provider reports |
| `command_output_tests.rs` | Command output frames, closes and watermarks |
| `key_table_tests.rs` | Key table patches, the active table and prefix claims |
| `modal_tests.rs` | Choosers, menus, confirms and modal overlays |
| `mux_options_tests.rs` | Mux option patches and terminal negotiation |
| `test_support.rs` | The hello, snapshot, event and drain helpers the tests share |

For the real-daemon convergence checks, run `cargo test -p zz-client --test simulator`.
The test drives clients through frame and snapshot updates and requests full frames to repair
a missing base. See [client core design](/designs/client-core-and-contract.md) and
[key tables](/tmux/key-tables.md).

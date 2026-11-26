---
type: Rust Crate
title: zz-terminal crate
description: Terminal session actors, PTY and libghostty-vt ownership, and the renderer-independent viewport, input and appearance types.
resource: crates/zz-terminal/src/terminal_core.rs
tags: [terminal, libghostty, pty, actor, worker-thread, frames]
timestamp: 2026-11-26T00:00:00-03:00
---

# Terminal state and execution

`crates/zz-terminal/src/terminal_core.rs` is the crate root. The default `session` feature
adds the PTY and libghostty-vt runtime. With default features disabled, clients can still use
the viewport model, appearance, input and interaction types. The daemon owns
`TerminalSession` handles. The actor for each session owns the mutable VT state and publishes
immutable viewports to readers. See [the PTY worker](/concepts/pty-worker.md) and
[terminal frames](/concepts/terminal-frame.md).

| Source under `crates/zz-terminal/src` | Responsibility |
| --- | --- |
| `session.rs` | Public session types and events, round-trip and wake-hold guards, limits, the struct definitions several session files read, and the `session/` module declarations |
| `session/handle.rs` | `TerminalSession` methods: spawn, requests, input, resize, views and shutdown |
| `session/actor_channels.rs` | Actor commands, control slots, input admission and the input lane, actor wakes, `CommandSender` and request replies |
| `session/pane_actor.rs` | Actor turns: input, PTY reads, per-client views and publication |
| `session/engine_filter.rs` | The OSC and CSI filter in front of libghostty, OSC 133 and progress parsing, the tmux passthrough filter and write diagnostics |
| `session/view_state.rs` | Per-view state, the viewport dictionary, viewport generations and kitty graphics storage and decoding |
| `session/publisher.rs` | `Publisher`, the terminal and output view workers, terminal setup and callbacks, `run_terminal` and the dead pane notice |
| `session/view_actions.rs` | View actions, view activation and restore, mouse routing, hover links and native selection |
| `session/capture.rs` | `capture-pane` in chunks, hyperlinks and line flags, styled capture and last command capture |
| `session/copy_mode.rs` | Copy mode actions, cursor, word and bracket motion, and selection on the frozen revision |
| `session/search.rs` | History search snapshots, the search worker and copy mode search |
| `session/snapshot.rs` | Viewport snapshots, copy mode facts and overlays, format grids and pointer context |
| `session/frames.rs` | Frame publishing, synchronized output, echo windows, idle compression and settling unwatched panes |
| `session/input_encoding.rs` | Key, direct key and focus event encoding |
| `session/pty_io.rs` | Unix PTY writer and direct input, wake waits, child exit watching and PTY reads |
| `session/linux_gather.rs` | Linux only: the gather stage that lends busy panes, direct readers and the child watch |
| `session/mode_revision.rs`, `session/copy_grid.rs` | The frozen copy mode revision and the grid copy mode reads |
| `session/*_tests.rs`, `session/test_support.rs` | Tests, one file per subject, and the helpers they share |
| `session/shard.rs` | Assigns PTY actors round-robin to lazily started worker threads. Default count is available parallelism capped at four. `ZZ_PTY_SHARDS=0` selects a dedicated worker for each PTY. |
| `session/unix_pty.rs`, `pty_types.rs` | Unix PTY handling. Windows selects `portable-pty` through the target-specific dependency in `Cargo.toml`. |
| `session/surface_actor.rs` | Terminal rendering for command output and other surfaces without a PTY. Surface actors share the shard pool on Unix and use the dedicated path on Windows. |
| `model.rs` | `TerminalViewport`, `PackedCell`, dictionaries, row spans and viewport diffs |
| `input.rs`, `interaction.rs` | Key input and pane/view actions, including mouse, copy mode, search and scrolling |
| `appearance.rs`, `shell_integration.rs` | Resolved terminal appearance and provenance, and startup integration for supported shells |

The [daemon](/crates/zz-daemon.md) fans the resulting views out to subscribers through the
[packed terminal lane](/protocol/terminal-lanes.md). The
[renderer](/terminal/rendering-parity.md) reads the published frame. See also
[libghostty-vt](/terminal/libghostty-vt.md), [interaction](/terminal/interaction.md) and
[appearance](/terminal/appearance.md).

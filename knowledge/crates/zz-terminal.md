---
type: Rust Crate
title: zz-terminal crate
description: Terminal session actors, PTY and libghostty-vt ownership, and the renderer-independent viewport, input and appearance types.
resource: crates/zz-terminal/src/terminal_core.rs
tags: [terminal, libghostty, pty, actor, worker-thread, frames]
timestamp: 2026-11-24T00:00:00-03:00
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
| `session.rs`, `session/pane_actor.rs` | `TerminalSession::spawn`, input/control queues, actor turns, per-client views and publication |
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

---
type: Concept
title: Daemon-owned PTY workers
description: How terminal actors share worker threads and route each viewer's input and frames.
resource: crates/zz-terminal/src/session.rs
tags: [pty, daemon, terminal, threading, send-keys]
timestamp: 2026-11-24T00:00:00Z
---

## Ownership and spawn

The daemon keeps an `Arc<TerminalSession>` per terminal pane. The actor owns the
PTY and libghostty-vt parser, screen and scrollback. Callers send input, resizes
and view actions through the session handle. Detaching a viewer releases its
subscription while the process keeps running. [Session persistence](/concepts/session-persistence.md)

`TerminalSession::spawn` assigns actors round-robin to lazily started shards.
The default count is available parallelism capped at four. `ZZ_PTY_SHARDS`
overrides the count, and zero gives each actor a dedicated `zz-terminal` thread.
The platform's PTY read mechanism is separate from where the parser runs.
See `crates/zz-terminal/src/session/shard.rs` (`choose`) and [PTY drain topology](/terminal/pty-drain.md).

A mux effect that creates or materializes a terminal supplies its command, cwd,
environment, initial geometry, history limit and appearance. A split can inherit
the donor's live working directory. An unmaterialized picker has no PTY. The
shell integration for supported Bash and zsh installs title and prompt hooks,
which can be disabled with `ZZ_SHELL_INTEGRATION=none`.

## Viewports and delivery

One PTY can have several views keyed by `TerminalViewId(client.0)`. Scroll anchor,
selection, copy mode and search belong to each view. The actor publishes immutable
viewports for streaming views and maintains a fallback for readers without a view.
Foreground, preview and inactive streams can receive different work budgets.

`Shared::watch_terminal` installs `shard_sink::PaneSink`. For a live foreground
view the sink can diff against the last tracked viewport and queue the encoded
frame directly to the subscriber. A frozen view, a mailbox with a frame pending,
or a view needing special handling can go through the daemon loop instead.
The sink and the loop track generations so a slow consumer can catch up to a
newer full frame. `RequestFull` repairs one pane. Copy and URI events carry the
originating view ID and go to that client. See `crates/zz-daemon/src/daemon/shard_sink.rs`
(`TerminalFrameSink::deliver`, `publish_loop_view`), [terminal frames](/concepts/terminal-frame.md)
and [terminal lanes](/protocol/terminal-lanes.md).

## Input and command output

Interactive keys first pass through the mux key tables. A key which reaches a
terminal, literal text, paste and mouse actions go to the actor, which knows the
terminal's current modes. `send-keys` resolves a pane target. Synchronized panes
can fan the input out to multiple sinks. Copy-mode actions and searches use
the viewer's state. Commands such as `capture-pane` can read the terminal
without attaching another interactive viewer. Command output can also use a
frozen terminal surface with no PTY child.
[Terminal interaction](/terminal/interaction.md) · [zz-terminal](/crates/zz-terminal.md) · [Commands](/tmux/commands.md)

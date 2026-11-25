---
type: Architecture
title: Process and threading model
description: Which process owns mux state, terminal actors, native chats and browser runtimes.
resource: crates/zz-daemon/src/daemon.rs
tags: [architecture, daemon, threading, ipc]
timestamp: 2026-11-24T00:00:00Z
---

## Processes

| Process | Responsibility and lifetime |
|---|---|
| Daemon | Owns the mux engine, PTYs, native agent runtimes and client connections. Survives client detach while sessions remain under the default exit policy. |
| Desktop client | Owns ala windows, presentation, local CEF browser instances and the tray. A GUI can hold connections to several hosts. |
| CLI and raw-terminal client | A command client connects, executes and exits. Raw-terminal attach maintains an interactive connection. |
| Chromium helpers | The desktop starts CEF renderer, GPU and utility subprocesses for local browser content. [CEF runtime](/browser/cef-runtime.md) |
| Provider processes | A vendor TUI runs in a terminal pane. For a native chat the daemon hosts a headless driver. The Codex driver can also connect to a shared app server. [Agent panes](/concepts/agent-panes.md) |

`zz app` opens the desktop. A terminal invocation with no command runs
`default-client-command` (normally `new-session -A`). Clients can start a local
daemon on demand. On Unix the spawned daemon creates its own session so closing
the launching terminal leaves it running. The daemon accepts connections during
startup, but ordinary clients wait for startup configuration. Daemon-owned
startup jobs can re-enter through the startup token. See `spawn_daemon` in
`crates/zz-cli/src/lib.rs` and `Daemon::run_foreground` in `crates/zz-daemon/src/daemon.rs`.

## Terminal and chat workers

A terminal actor owns one PTY and its libghostty-vt state. By default up to four
shard threads host multiple actors. `ZZ_PTY_SHARDS=0` selects the dedicated-thread
path. The actor publishes immutable frames per client view to a daemon frame sink.
The daemon routes commands, pane events and subscriber mailboxes.
[PTY worker](/concepts/pty-worker.md) · [Data flow](/architecture/data-flow.md)

`AgentHost::open` in `crates/zz-agent/src/host.rs` starts a thread per
native chat. `AgentFanout` uses a shared flusher for its 25 ms batching window.
The daemon applies events only if the pane still holds the matching runtime
generation. Closing the GUI leaves these runtimes available to other clients.

## IPC

Local IPC uses a Unix-domain socket or a Windows named pipe. `ZZ_SOCKET` selects
the local endpoint. `TMUX` inside a pane provides tmux-compatible metadata.
Unix desktop SSH connections can forward a remote socket. Mobile SSH clients
use an in-process SSH transport and a remote `zz proxy`. Tailnet endpoints and
Windows WSL distribution endpoints are additional routes to a daemon.
[Wire protocol](/protocol/wire-protocol.md) · [Fleet attach](/designs/fleet-attach.md) · [Tailnet](/designs/tailnet-node.md)

## Update and shutdown

`zz update` can install a newer headless executable in zz's data directory.
The `build-info` file marks a complete owned copy. A packaged Unix CLI can hand
over to that copy. The bootstrap path checks wire and revision compatibility,
and `ZZ_OWNED_COPY=0` disables handover. A running daemon keeps its executable
until restarted. `restart-server` ends its sessions. On Unix it holds the start
lock across exec so another client cannot start a competing daemon.
[Daemon update](/designs/daemon-update.md) · [Session persistence](/concepts/session-persistence.md)

## Tray

The desktop builds the tray menu from its snapshot. With the tray enabled and
available, local sessions open, and `quit-daemon-on-exit` false, ordinary quit
can hide the window. **Quit and Stop Sessions** kills the local daemon and
quits. Disabling the tray brings a hidden window back. See `crates/zz/src/tray/mod.rs`
and `crates/zz/src/tray/desktop.rs`.

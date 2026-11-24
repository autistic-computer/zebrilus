---
type: Rust Crate
title: zz-daemon crate
description: Server ownership of mux state, PTYs, agent runtimes, connection dispatch and per-client publication.
resource: crates/zz-daemon/src/daemon.rs
tags: [crate, daemon, ipc, fanout, transport, agent]
timestamp: 2026-11-24T00:00:00-03:00
---

# What survives a disconnect

`Daemon::serve` owns the mux engine, session attachments, PTY sessions and agent runtimes.
Closing a client releases its subscriptions and terminal views. Under the default exit policy
a remaining session keeps its processes alive for another client. A daemon restart ends them.
The daemon retains browser pane descriptors while a client owns the renderer.
See [session persistence](/concepts/session-persistence.md), [mux state](/crates/zz-mux.md)
and [agent panes](/concepts/agent-panes.md).

The local listener uses a Unix-domain socket or a Windows named pipe. Remote routes deliver
the same framed stream. The [client library](/crates/zz-daemon-client.md) owns the dialer,
SSH process or mobile SSH channel, and prompts.

## Connection dispatch

| Entry point | Unix | Windows |
| --- | --- | --- |
| Listener and I/O | `daemon/event_loop.rs` (`EventLoop`, mio) | `accept_connections` and per-connection threads in `daemon.rs` |
| Hello-based requests | `daemon/connection.rs` (`Session`) | `handle_connection_message` in `daemon.rs` |
| One-shot command | `LoopExec` in `daemon/exec.rs` | `serve_exec` and `serve_exec_ready` in the same file |

These paths are relative to `crates/zz-daemon/src`. A new incoming request must reach the
dispatcher on each supported platform. Some services have narrower platform support. The
daemon defaults to the `agent` and `tailnet-own` features. The embedded tailnet dependency
is available on Linux and arm64 macOS. `daemon/agent_login.rs` is built with `agent` on Unix. The hello can still advertise `agent-login-v1` on
Windows or in an agent-disabled build, where `handle_agent_login` in `daemon/accounts.rs`
discards the request.
Consult the feature and target sections in `crates/zz-daemon/Cargo.toml` and the module gates
in `daemon.rs` when a capability has different backend support by platform.

## Mux state and delivery

- `ServerState` holds the `MuxEngine`, connected clients and attachment state. The same
  session can have several viewers with their own focus, overlay, read-only state and
  terminal view. The daemon executes the mux's effects and sends a scoped tree plus
  recipient-specific state. See [snapshots](/protocol/snapshots.md) and
  [multi-device attach](/designs/multi-device-attach.md).
- `OutboundMailbox` has a reliable path for ordered control traffic and coalesces terminal
  frames per pane. `PaneFrameFanout` shares an encoded patch for a matching transition.
  `TerminalFrames` caches encoded full views. A client whose patch base is unavailable can
  request a full viewport for that pane. See [terminal lanes](/protocol/terminal-lanes.md).
- `daemon/shard_sink.rs` connects terminal actors to subscribers. `daemon/pane_summary.rs`
  derives the metadata that the sidebar and status views consume. Terminal state belongs to
  [zz-terminal](/crates/zz-terminal.md). `status.rs` handles daemon-side status rendering
  and format jobs. See [PTY workers](/concepts/pty-worker.md) and [status line](/tmux/status-line.md).
- `agent/host.rs`, `agent/runtime.rs` and the provider modules under `agent/` run agents.
  `agent/fanout.rs` and `daemon/agent_publisher.rs` publish state and timeline updates.
  `daemon/agent_flip.rs` handles switching between a supported terminal CLI and chat.
  See [agent driver contracts](/references/agent-cli-contracts.md).

## When the daemon stops

`Shared::should_shutdown_if_empty` first checks for subscribed clients. With default settings
a fresh empty daemon can wait for its first session. Once session creation arms the empty
check, an empty daemon with no subscribers exits. Explicit `exit-empty` and `exit-unattached`
change the rule once startup is ready. `exit-empty off` disables this automatic shutdown.
`kill-server` requests a forced shutdown. `restart-server` returns a restart disposition
to [zz-cli](/crates/zz-cli.md), which launches the successor.

## Reaching a remote daemon over ssh

The client's `endpoint.rs` probes the remote socket and binary. On macOS/Linux the client can
start a missing daemon and forward a local Unix socket to it. Windows uses SSH stdio and
`zz proxy`. iOS and Android use an in-process SSH channel. [Endpoint and transport
details](/crates/zz-daemon-client.md).

### Installing zz on a remote

`zz fleet add NAME user@host` records the destination. `zz fleet install NAME` installs the
CLI there. `zz --host NAME build-info` inspects the remote binary and `zz --host NAME list-sessions`
queries its daemon. Use `zz fleet install --restart-daemon NAME` when the running daemon also
needs replacement. Its sessions end on restart. [Fleet attach](/designs/fleet-attach.md).

### Connection sharing

Desktop Unix clients can reuse an OpenSSH control master. An existing master receives
`ssh -O forward`. A standalone forwarding child uses `-N -n` with no command input.

### Kept forwards

Repeated CLI calls can reuse the same forwarded socket. `connect_kept` serializes recreation
and probes the replacement before a waiting caller sends its command. See `endpoint.rs` in
[zz-daemon-client](/crates/zz-daemon-client.md).

### zz's own key

`zz fleet key` prints the machine's public key. `zz fleet sync` distributes fleet keys.
`zz fleet trust KEY` enrolls the supplied public key. `fleet_sync.rs` in the
[client library](/crates/zz-daemon-client.md) implements those operations.

For a server-only build check without agents, run
`cargo clippy -p zz-daemon -p zz-daemon-client --no-default-features --all-targets -- -D warnings`.
See [wire compatibility](/protocol/wire-protocol.md) before changing the handshake or publication.

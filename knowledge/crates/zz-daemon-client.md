---
type: Rust Crate
title: zz-daemon-client crate
description: Command and interactive connections, local and remote routes, host discovery, identity and process facts shared with the server.
resource: crates/zz-daemon-client/src/lib.rs
tags: [crate, client, ipc, transport, ssh, endpoint]
timestamp: 2026-11-24T00:00:00-03:00
---

# Connecting to a daemon

`CommandClient` and `InteractiveClient` in `crates/zz-daemon-client/src/client.rs` write
the [wire protocol](/protocol/wire-protocol.md), materialize hello state and expose replies or
events to their callers. `ProtocolSender` checks the daemon's declared capabilities for gated
messages. One-shot exec has a separate opening frame. Client-only consumers link this library
without linking the server implementation.

| Route | How a caller reaches the daemon |
| --- | --- |
| Local | `transport.rs` dials a Unix-domain socket or a Windows named pipe. `ZZ_SOCKET` selects the endpoint. |
| SSH from macOS/Linux | `endpoint.rs` resolves the remote binary and socket, starts the daemon if needed, and uses OpenSSH local socket forwarding. CLI connections can reuse a kept forward. Interactive browser egress can get a SOCKS endpoint. |
| SSH from Windows | OpenSSH runs the remote connect script and `zz proxy` through stdio. The client presents the route through a local pipe. |
| SSH from iOS/Android | `russh_client.rs` runs the SSH channel in-process. `russh_prompt.rs` and `russh_socks.rs` handle prompts and browser forwarding. The Keychain integration in `ios_keychain.rs` is iOS-specific. |
| Tailnet | Desktop `tailnet.rs` provides discovery, address resolution and `TailnetForward`. Mobile interactive clients dial TCP through `client.rs`. |
| WSL | A native Windows client opens a distro with `WslForward` in `wsl.rs`. The endpoint spelling is `wsl://DISTRO`. |

`Endpoint` in `endpoint.rs` parses these forms. The desktop-only `DaemonRoute` in `route.rs`
keeps a local endpoint available to callers that handle framing themselves, including the web
gateway. `open_unattended` and `open_with_prompts` choose the login mode. See
[fleet attach](/designs/fleet-attach.md).

## Remote connection and repair

A cold SSH connection checks the remote probe's marked output so shell startup noise cannot
be mistaken for a socket path. On Unix, a warm CLI call can dial a kept socket. If its
transport has died, `connect_kept` coordinates a replacement. A command-level error from a
working connection is returned to the caller. A live control master accepts forward/cancel
requests. Interactive login can use `askpass.rs` to route SSH prompts back to the UI.

`host_failure.rs` classifies host errors. `lifecycle.rs` handles local daemon identity and
guarded termination of an incompatible process. `classify_local_connect_error` in `lib.rs`
distinguishes a mismatched wire version from a dropped connection during the handshake.
See [remote installation](/crates/zz-daemon.md#installing-zz-on-a-remote) and
[process lifetime](/concepts/session-persistence.md).

## Utilities used by the server and front ends

| Source under `crates/zz-daemon-client/src` | Responsibility |
| --- | --- |
| `fleet_hosts.rs`, `fleet_sync.rs`, `host_probe.rs` | Saved host entries, fleet key propagation and remote availability |
| `identity.rs`, `ssh_key.rs` | Daemon identity proof and the local zz SSH key |
| `paths.rs`, `user_data.rs` | Configuration discovery and application data paths |
| `process_info.rs`, `unmasked.rs` | Platform process facts and child spawning with the appropriate signal mask |
| `terminal_features.rs` | Terminal feature facts used while connecting and rendering tmux formats |

The [daemon](/crates/zz-daemon.md) serves the connection. [zz-cli](/crates/zz-cli.md) chooses
the endpoint and user-facing command behavior. [zz-client](/crates/zz-client.md) interprets
received state independently of the transport.

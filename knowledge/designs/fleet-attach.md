---
type: Design Plan
title: "One desktop client, multiple daemons"
description: "The desktop tracks each host separately and attaches by host and session."
status: "Implemented, verified against source 2026-11-24"
resource: crates/zz/src/mux/client.rs
tags:
- remote
- multi-host
- client
- ssh
- choose-tree
- design-plan
timestamp: 2026-11-27T00:00:00-03:00
---

The desktop lists workspaces from multiple daemons. Sessions, windows and panes have daemon-local IDs, so desktop selection carries the host with the target. The daemon holding the session owns its PTYs and agent processes. Switching machines changes the client's attachment.

`Endpoint` in `crates/zz-daemon-client/src/endpoint.rs` represents a local socket, SSH, tailnet or WSL distribution. SSH forwards the remote daemon's socket and uses the user's SSH configuration and authentication. [Tailnet nodes](/designs/tailnet-node.md) describes peer discovery and admission. The WSL endpoint identifies a distribution and optionally a user and remote socket.

`MuxClient::attach_to_host_target` in `crates/zz/src/mux/client/attach.rs` switches the attachment. Each connection tracks its own state, failures and reconnect timer. A lost connection can leave the last remote view on screen while the client reconnects. Explicitly selecting another machine remains possible during recovery.

The July 2026 direct QUIC pairing protocol was retired. See [direct pairing](/designs/fleet-pairing.md) for that record, [remote browser egress](/designs/remote-browser-egress.md) for SSH browser traffic and [daemon update](/designs/daemon-update.md) for the transports supported by the updater.

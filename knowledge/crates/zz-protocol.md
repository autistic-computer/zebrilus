---
type: Rust Crate
title: zz-protocol crate
description: Shared Rust types and codecs for daemon messages, terminal frames, layout, agent chat, command discovery and keys.
resource: crates/zz-protocol/src/lib.rs
tags: [protocol, crate, wire, ipc]
timestamp: 2026-11-24T00:00:00-03:00
---

# What belongs here

`zz-protocol` defines the values that the [daemon](/crates/zz-daemon.md) and its clients
must interpret the same way. It owns serialization and validation, the command catalog and
key engine. Connection establishment lives in [zz-daemon-client](/crates/zz-daemon-client.md).
[zz-mux](/crates/zz-mux.md) executes commands and [zz-client](/crates/zz-client.md) reduces
events into client state. The public surface is re-exported by `crates/zz-protocol/src/lib.rs`.

| Source under `crates/zz-protocol/src` | Use it for |
| --- | --- |
| `message.rs` | `ProtocolMessage`, `InputMessage`, `EventPayload`, command results, validation limits, epoch and revision constants |
| `control.rs`, `exec.rs` | Hello, subscriptions, capability bits, batch encoding model, client view and one-shot command exchange |
| `framing.rs`, `terminal_codec.rs` | Envelope, length checks, lane selection, public encode/decode and stream I/O |
| `pane_frame.rs`, `key_frame.rs` | Compact viewport/patch/history codec and key-input encoder |
| `id.rs`, `snapshot.rs`, `tree_delta.rs` | Stable identifiers, shared tree, pane descriptors and transactional tree deltas |
| `catalog.rs`, `key.rs`, `style.rs` | Command names and argument specifications, key bindings and resolution, tmux styles |
| `agent_stream.rs`, `chat.rs`, `providers.rs` | Agent stream and chat records, provider status, quotas and login vocabulary |

For a new wire feature, follow [adding a message inside an epoch](/protocol/wire-protocol.md#adding-a-message-inside-an-epoch).
For a decoder, start with [framing](/protocol/wire-protocol.md), [terminal frames](/protocol/terminal-lanes.md),
[snapshots](/protocol/snapshots.md) and [IDs](/protocol/ids.md). `cargo test -p zz-protocol`
exercises the codec and contract tests.

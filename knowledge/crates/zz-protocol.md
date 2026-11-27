---
type: Rust Crate
title: zz-protocol crate
description: Shared Rust types and codecs for daemon messages, terminal frames, layout, agent chat, command discovery and keys.
resource: crates/zz-protocol/src/lib.rs
tags: [protocol, crate, wire, ipc]
timestamp: 2026-11-26T00:00:00-03:00
---

# What belongs here

`zz-protocol` defines the values that the [daemon](/crates/zz-daemon.md) and its clients
must interpret the same way. It owns serialization and validation, the command catalog and
key engine. Connection establishment lives in [zz-daemon-client](/crates/zz-daemon-client.md).
[zz-mux](/crates/zz-mux.md) executes commands and [zz-client](/crates/zz-client.md) reduces
events into client state. The public surface is re-exported by `crates/zz-protocol/src/lib.rs`.

| Source under `crates/zz-protocol/src` | Use it for |
| --- | --- |
| `message.rs` | `ProtocolMessage`, `EventPayload`, `Event`, detach reasons, epoch and revision constants, `CommandInvocation`, `ClientPath`, and the field decoders only those two enums use. It declares the files under `message/` and re-exports them, so every `message::` path resolves as before |
| `message/limits.rs` | Capability names, the `MAX_` payload bounds, and the shared bounded-text and byte-list codec helpers |
| `message/options.rs` | `MuxOptionKey`, `MuxOptions` and config override entries |
| `message/presentation.rs` | `StatusLine`, mode and pane border presentation, client message kinds |
| `message/raw_text.rs` | `RawText` and its trait impls |
| `message/hello.rs` | `ClientHello`, `ServerHello`, key table snapshots, client environment and capability bounds |
| `message/commands.rs` | Command requests, prepared commands, responses and `ServerError` |
| `message/input.rs` | `InputMessage`, `KeyToken`, browser and simulator commands, `GuiResponse` |
| `message/agent.rs` | Agent wire types, agent command defaults and their bounded decoders |
| `message/overlays.rs` | Command prompt, choose-tree, choose-buffer, chooser, pane indicator, popup, menu and confirm state |
| `message/transfers.rs` | Paste upload kinds and client file requests and responses |
| `message_tests.rs`, `wire_bytes_tests.rs` | The tests of `message.rs` and its files, and old-shape wire byte comparisons |
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

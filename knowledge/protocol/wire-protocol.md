---
type: Protocol
title: zz wire protocol
description: Frame layout, connection handshakes, wire epochs, revision floors, and the steps for adding a capability-gated message.
resource: crates/zz-protocol/src/framing.rs
tags: [protocol, wire, framing, postcard, versioning]
timestamp: 2026-11-26T00:00:00-03:00
---

# Framing

The client and daemon exchange ordered binary frames over local IPC or a remote transport. Each
frame starts with eight bytes. The length is the number of bytes **after** the length prefix,
including the four-byte envelope. The largest legal length is 64 MiB. The decoder checks it
before growing the read buffer. See `crates/zz-protocol/src/framing.rs`, `read_enveloped_into`.

| Offset | Field | Encoding |
| --- | --- | --- |
| 0–3 | Length | Unsigned 32-bit little-endian, at least 4 |
| 4 | Lane | 0 for Control, 1 for Terminal |
| 5 | Flags | Zero. Every other value is rejected. |
| 6–7 | Wire epoch | Unsigned 16-bit little-endian, must equal `PROTOCOL_VERSION` |
| 8 onward | Payload | A postcard `ProtocolMessage` or a packed terminal frame |

Use `encode_protocol_message` and `decode_protocol_frame` for a complete frame or
`write_protocol_message` and `read_protocol_message_into` for a stream.
`crates/zz-protocol/src/terminal_codec.rs` selects the lane and validates message-specific
limits. Viewports, patches, populated command-output viewports and history chunks use the
[terminal lane](/protocol/terminal-lanes.md). An empty command-output close travels on Control.

## Wire epochs

`crates/zz-protocol/src/message.rs` defines the current contract. `zz build-info` prints all four
numbers for a binary. `zz protocol-version` prints its wire epoch.

| Constant | Value | Checked by |
| --- | ---: | --- |
| `PROTOCOL_VERSION` | 125 | Both sides on every envelope |
| `REVISION` | 11 | The hello exchange advertises the sender's behavior revision |
| `MIN_DAEMON_REVISION` | 1 | A connecting hello client checks the daemon's reply |
| `MIN_CLIENT_REVISION` | 1 | The daemon checks the client's hello |

Postcard encodes enum variants by position and struct fields in declaration order. Bump the
epoch when changing or removing a shipped field, variant, meaning, or terminal packing. Appending a
variant to a nested enum such as `InputMessage` or `TreeOp` also requires a bump if an old peer
can receive it. An existing field with a serde default still occupies a position on the wire.

For additive features, append a gated `ProtocolMessage` or `EventPayload` variant. To extend a
snapshot, publish the additional data in a gated event keyed by object ID. A new pane kind can
use `PaneKindSnapshot::Other { kind }` with its descriptor in that event. The simulator uses
this pattern. See [snapshots](/protocol/snapshots.md).

Bump `REVISION` by hand when the other side relies on a behavior change, including a fix. Raise
the relevant minimum only when accepting an older peer would produce incorrect behavior. Equality
of revisions is unnecessary. A daemon below the floor and a client below the floor have distinct
errors. Replacing a running daemon to cross an incompatible epoch ends its sessions. Release
notes record the epoch and daemon floor for the updater in `.github/workflows/release.yml`
and `crates/zz-config/src/update.rs` (`ReleaseWire`).

## Connection handshakes

| First message | Reply and purpose |
| --- | --- |
| `Hello` | Interactive and control clients receive `Welcome` and grouped initial state. The caller can include an attach operation, viewport and subscriptions in the first frame. `Welcome` carries IDs, capability bits, daemon clock, platform and revision. |
| `ClientHello` | The explicit command-client handshake returns `ServerHello`, including named capabilities, appearance and initial option/key/status data. |
| `Exec` | One-shot command path. `ExecRequest` supplies caller facts, environment, commands and optional expected server ID. `ExecExit` reports completion, rejection, a server mismatch or a request to resume in an interactive client. There is no revision or receiver capability list in `ExecRequest`. |

`Subscriptions` selects tree scope (none, attached session, all), status, mux options by bit,
key tables (none, hash, full), and pane streaming. A control client can request the tree while
leaving terminal streaming off. A `Batch` groups up to 4,096 separately framed messages and
rejects nested batches when unpacked. See `crates/zz-protocol/src/control.rs` and
`crates/zz-daemon-client/src/client.rs` (`connect_stream_hello`, `materialize_welcome`).

An attached client keeps a shared tree and a private view. A failed tree delta requests
`TreeSync`. A failed terminal patch requests `RequestFull { pane }`. A skipped event sequence
alone requires neither repair. See [snapshots](/protocol/snapshots.md) and
[terminal delivery](/protocol/terminal-lanes.md).

## Adding a message inside an epoch

1. Append the new variant to the end of `ProtocolMessage` or `EventPayload` in `message.rs`.
   Give it an explicit arm returning `Some(CAPABILITY)` in that enum's `required_capability`.
   `ProtocolMessage::Event` delegates to its payload. The matches are exhaustive.
2. Declare and export the capability in `control.rs` and `lib.rs`. For a **daemon receiver**,
   append it to `CAPABILITY_NAMES`. The position is its bit in `Welcome.caps`, so retain
   existing positions within an epoch. `ServerHello.capabilities` also advertises these names
   except the control-plane, control-stdio and tty-input entries. Update
   `a_server_hello_lists_the_daemon_capabilities_without_the_control_plane` in
   `crates/zz-daemon/src/daemon/attach_tests.rs`.
3. For a **client receiver**, add the name to `CLIENT_CAPABILITY_NAMES` for every hello client,
   or `OPT_IN_CLIENT_CAPABILITY_NAMES` when a client must actively enable the feature. For the
   latter, add the name to each supporting client's hello. Agent login uses opt-in. A request
   with a gated response can require entries in both directions, as file editing does.
   The table size assertions reserve 29 of 64 client hello entries and limit the daemon bit
   table to 63 names.
4. Handle an incoming request in both daemon dispatchers. Unix uses `Session::message` in
   `crates/zz-daemon/src/daemon/connection.rs`. Windows uses `handle_connection_message` in
   `crates/zz-daemon/src/daemon/connection_messages.rs`. Both have catch-all arms. One-shot exec logic likewise
   has a Unix `LoopExec` and Windows `serve_exec`/`serve_exec_ready` in
   `crates/zz-daemon/src/daemon/exec.rs`. On Unix the event loop handles transport-level
   messages such as control stdio and TTY handoff in `EventLoop::dispatch` before the session.
5. Gate the client action on the daemon's advertised capability. The native `ProtocolSender`
   refuses unsupported sends with `DaemonLacksCapability`. The web path in
   `clients/app/src/connection.rs` checks `Welcome::has_capability`. A new gated send in
   `clients/dom/src/lib.rs` needs its own check.
6. Deliver a gated event or reply through the daemon's typed reliable mailbox path.
   `OutboundMailbox::accept_client_capabilities` retains the recognized names from the hello.
   `enqueue_reliable_frame` suppresses unsupported messages per recipient. Pre-encoded frames,
   control groups and specialized terminal/image queues bypass that check. An exec-only
   connection has declared no client capabilities. Handle the new event in
   `crates/zz-client/src/core.rs` and any UI that consumes it.

The two `required_capability` matches are the authoritative variant-to-feature mapping.
For example, simulator descriptors and commands both travel **to the client** under
`simulator-pane-v1`. `AgentUnwatch` travels **to the daemon** under `agent-unwatch-v1`.
`IdentityChallenge` and `IdentityProof` share `daemon-identity-v1`. Check both directions with
a peer missing the capability as well as one advertising it.

## Transferring data between hosts

A clipboard image destined for a remote pane travels as `PasteUploadBegin` and ordered
`PasteUploadChunk` messages. Completion occurs when the received bytes reach the declared
length. `PastePath` writes the image on the daemon host and pastes the resulting path into the
pane. `RecordPastedImage` retains the bytes for a numbered placeholder that the client can
retrieve with `FetchPastedImage`. The wire limit is 6 MiB and a chunk is at most 1 MiB.
The daemon admits up to two in-flight uploads per client. See `finish_paste_upload` in
`crates/zz-daemon/src/daemon/paste_uploads.rs`.

File editing has a separate request/reply exchange. `FileReadRequest`, `FileWriteRequest`
and their responses require `file-edit-v1`. Both sides must advertise it for the round trip.
The bounds for editable file content and write chunks are `MAX_FILE_EDIT_BYTES` and
`MAX_FILE_WRITE_CHUNK_BYTES` in `message.rs`. The implementation lives in
`crates/zz-daemon/src/daemon/file_edit.rs`.

## Source and checks

- Message fields, payload bounds and errors are in `message.rs`, `control.rs`, `exec.rs`
  and `terminal_codec.rs` under `crates/zz-protocol/src`.
- Run `cargo test -p zz-protocol` for codec, batch, delta and bounds checks. The scoped
  `cargo test -p zz-daemon a_server_hello_lists_the_daemon_capabilities_without_the_control_plane`
  checks the server's advertised capabilities.
- [Client endpoints](/crates/zz-daemon-client.md) carry the stream.
  [Multi-device attach](/designs/multi-device-attach.md) and
  [terminal appearance](/terminal/appearance.md) cover the state a subscriber receives.

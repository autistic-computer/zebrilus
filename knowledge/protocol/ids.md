---
type: Protocol
title: Stable object and client IDs
description: Six typed u64 identifiers distinguish mux objects, client connections and client instances.
resource: crates/zz-protocol/src/id.rs
tags: [protocol, ids, targets, tmux]
timestamp: 2026-11-24T00:00:00-03:00
---

# IDs on the command line and wire

`crates/zz-protocol/src/id.rs` defines six distinct transparent `u64` types with
`stable_id!`. The prefix is part of `Display` and `FromStr`. Postcard serializes the number.

| Type | Example | Names |
| --- | --- | --- |
| `SessionId` | `$0` | A session |
| `WindowId` | `@2` | A window |
| `PaneId` | `%3` | A pane across moves and layout changes |
| `SplitId` | `^4` | A divider in a window layout |
| `ClientId` | `c5` | A daemon-assigned connection |
| `ClientInstanceId` | `i6` | The caller's instance ID, echoed at handshake |

String parsing requires the exact prefix followed by an unsigned integer. A bare `3` fails
as a `PaneId`. The [mux](/crates/zz-mux.md) separately resolves command target expressions.
Scope object IDs to the daemon that issued them. A different host can also have a pane `%3`.

```sh
zz list-panes -t '$0' -F '#{pane_id} #{pane_current_command}'
zz send-keys -t %3 'pwd' Enter
```

[Snapshots](/protocol/snapshots.md) carry IDs in the tree. The
[wire protocol](/protocol/wire-protocol.md) uses them for routing and per-client state.

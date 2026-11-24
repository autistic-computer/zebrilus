---
type: Protocol
title: Mux snapshots and client views
description: The shared session tree, per-client presentation, pane summaries and delta repair used on attach.
resource: crates/zz-protocol/src/snapshot.rs
tags: [protocol, snapshot, layout, state, presence, sidebar, pane-summary]
timestamp: 2026-11-24T00:00:00-03:00
---

# Shared tree and client view

`MuxSnapshot` in `crates/zz-protocol/src/snapshot.rs` is the renderable session tree.
An attached subscriber receives a raw tree for its requested scope and a separate
`ClientView`. The latter supplies the attachment, focus, read-only state, client flags and
presentation operations. `MuxSnapshot::focused_window_for` uses the recipient's focus if
that window is in the session, otherwise the session's active window. Each session viewer
has a device name, window and recipient-relative `is_self`.

| Record | What it describes |
| --- | --- |
| `MuxSnapshot` | Generation, sessions and optional focused window |
| `SessionSnapshot` | Stable ID, name, active window, windows and viewers |
| `WindowSnapshot` | Index, title, active and zoomed panes, split layout and pane map, pane order and z-order, floating and modal panes, window cell dimensions and presentation |
| `PaneSnapshot` | Stable ID, title, kind, synchronized input, bell and dead state, borders, mode, optional status and summary |

`LayoutNode` is `Empty`, `Pane(PaneId)` or `Split { id, axis, ratio, first, second }`.
The mux projects its cell layout into this binary tree. Floating panes have separate position,
extent and visibility records. The deserializer caps each layout at depth 256 and 65,535 nodes.
See [split-pane layout](/concepts/split-pane-layout.md) and [stable IDs](/protocol/ids.md).

## Updates and repair

1. The daemon sends the initial scoped snapshot in an attach batch. Subsequent tree changes
   arrive as `TreeDelta { base, version, ops }`.
2. `TreeDelta::apply` checks the retained generation and operation targets. An invalid delta
   leaves the prior tree intact. `ClientCore` queues one `TreeSync` and waits for a fresh
   snapshot before accepting more deltas.
3. `ClientView` overlays the retained tree for this recipient. Its `attachment_generation`
   changes on reattach, including to the same session. A changed attachment clears per-session
   overlay and viewport state. Its `layout_generation` tags geometry reports. A resize measured
   before a zoom or layout change is stale even when its dimensions still match.

See `crates/zz-protocol/src/tree_delta.rs`, `crates/zz-protocol/src/control.rs`
(`ClientView`) and `crates/zz-client/src/core.rs` (`handle_payload`).
Terminal cells and scrollback travel on the [terminal lane](/protocol/terminal-lanes.md).

## Pane kinds

`PaneKindSnapshot` carries `Picker`, `Terminal`, `Browser`, `Editor`, `Agent`, `View` or
`Other { kind }`. For a browser, the descriptor contains tab URLs, the active index and the
profile. For an agent it contains the provider plus optional cwd, session, model, effort and
account. A restored client uses the descriptor to identify the pane. Agent state and timeline
updates travel separately. See [agent panes](/concepts/agent-panes.md).

`Other { kind: "simulator" }` keeps the tree readable by clients in the same wire epoch.
Supporting clients receive the device in a gated `SimulatorPane` event. The mux's internal
`PaneKind::Simulator` and the snapshot representation serve different consumers.
See [wire epochs](/protocol/wire-protocol.md#wire-epochs).

## Pane status, summary and time

`PaneStatus` has a state (`Idle`, `Working`, `Done`, `Blocked` or `Error`), optional kind
(permission, question or auth), optional progress, and app, title and message strings.
`PaneSummary` supplies the sidebar's command status, name, shell, cwd, repo, branch, process
list, pending flip (`flip_armed`), change time and unseen flag. A shell command is running or
finished with an 8-bit status.

`changed_at` uses daemon Unix seconds. Shell start and finish marks and status transitions can
advance it. Entering Done, or returning from Working or Blocked to Idle, can mark the pane
unseen if nobody is viewing its window. A new Working or Blocked state clears that marker.
`Welcome.clock` and `ServerHello.clock` let clients compute ages relative to the daemon even on
a machine with a different clock. See `crates/zz-daemon/src/daemon/pane_summary.rs` and
`crates/zz-client/src/window_summary.rs` (`DaemonClock`).

The [program status guide](/terminal/program-status.md) describes inputs to these fields.
[Flat sidebar data](/designs/flat-sidebar-data.md) describes how the client aggregates them.

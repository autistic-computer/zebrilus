---
type: Architecture
title: zz system overview
description: How desktop, thin and terminal clients share a daemon, and which process owns each kind of pane.
resource: crates/zz-protocol/src/snapshot.rs
tags: [architecture, overview, multiplexer, daemon]
timestamp: 2026-11-24T00:00:00Z
---

The daemon owns sessions, windows, layout, terminal processes and native agent chats.
Clients attach to the same session and can leave independently. A pane ID identifies
the same pane across views. Each client can select a window and scroll independently.
Layouts are measured in cells and projected into snapshots for the clients.
[Session persistence](/concepts/session-persistence.md) · [Split-pane layout](/concepts/split-pane-layout.md)

## Panes

| Pane | Where it runs |
|---|---|
| Terminal | A daemon-owned PTY feeds libghostty-vt. Clients receive frames for their own views. [PTY worker](/concepts/pty-worker.md) |
| Agent | A daemon-owned driver runs Claude Code, Codex, Antigravity, Grok or Cursor. Clients draw the chat from its event stream. Supported providers can switch between chat and their terminal UI in the same pane. [Agent panes](/concepts/agent-panes.md) |
| Browser | The daemon keeps the tab descriptors. The desktop client runs Chromium through CEF and paints its frames. Native mobile clients use platform web views. [Browser lifecycle](/browser/lifecycle.md) |
| Picker, view and editor | The picker materializes a pane, a view displays referenced content, and the editor is feature- and config-gated. The mux also supports an iOS Simulator pane on supported hosts. See `PaneKindSnapshot` in `crates/zz-protocol/src/snapshot.rs` and `simulator_descriptor` in `crates/zz-mux/src/model.rs`. |

## Clients and crates

| Responsibility | Code |
|---|---|
| Resolve tmux commands, targets, options and layouts | [zz-mux](/crates/zz-mux.md) |
| Serve mux state, drive PTYs and agent runtimes, publish updates | [zz-daemon](/crates/zz-daemon.md), [zz-terminal](/crates/zz-terminal.md) |
| Exchange snapshots, events and packed terminal frames | [zz-protocol](/crates/zz-protocol.md) |
| Connect to a host and maintain the client view | [zz-daemon-client](/crates/zz-daemon-client.md), [zz-client](/crates/zz-client.md) |
| Draw the desktop workspace | [zz](/crates/zz.md), zz-ui, [penna](/references/penna.md), [ala](/references/ala.md), [ala-cef](/crates/ala-cef.md) |
| Present the session on other clients | zz-app for the ala web, iOS and Android clients, zz-dom for the HTML client, zz-tui for raw-terminal attach through [zz-cli](/crates/zz-cli.md) |

`Endpoint` in `crates/zz-daemon-client/src/endpoint.rs` distinguishes local, SSH,
tailnet and WSL endpoints. Browser clients can connect through the zz-web gateway.
See [process model](/architecture/process-model.md), [data flow](/architecture/data-flow.md)
and [fleet attach](/designs/fleet-attach.md) for the paths across those boundaries.

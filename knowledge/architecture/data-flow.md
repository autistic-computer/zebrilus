---
type: Architecture
title: End-to-end data flow
description: How terminal frames, browser pixels, chat events and input reach the pane that owns them.
resource: crates/zz-daemon/src/daemon.rs
tags: [architecture, data-flow, frames, input, rendering]
timestamp: 2026-11-24T00:00:00Z
---

## Terminal

The PTY writes bytes to a terminal actor. Libghostty-vt parses the stream once.
The actor publishes immutable viewports for subscribed views, each with its own
scroll position, selection and copy-mode state. The daemon's `shard_sink::PaneSink`
can deliver them directly to subscriber mailboxes. A client receives a full
viewport or a patch against its retained grid. If its base generation is wrong,
`RequestFull { pane }` repairs the affected pane. History requests backfill the
client's `HistoryRing`. The desktop paints the resulting rows as an ala element.
[PTY worker](/concepts/pty-worker.md) · [Terminal lanes](/protocol/terminal-lanes.md) · [Terminal interaction](/terminal/interaction.md)

## Browser

Desktop CEF off-screen rendering publishes its latest frame to a local mailbox.
The client paints a shared GPU surface when available and falls back to BGRA pixels.
Page keys, pointer events, IME and focus go to the browser input handler.
The daemon tracks the tab list, selected tab and profile so a client can reopen them.
[OSR rendering](/browser/osr-rendering.md) · [Browser input](/browser/input-translation.md) · [Browser lifecycle](/browser/lifecycle.md)

## Agent chat

1. The composer sends `AgentPrompt`. The daemon routes it to the pane's
   `AgentHost`, which drives the provider's headless protocol.
2. The driver translates provider events. `AgentFanout` batches sequenced stream
   items into `AgentUpdates`. `AgentState` carries the current status and the first pending
   permission. The client tracks all open requests from the stream’s `RequestOpened` and
   `RequestClosed` events alongside messages, tool calls and turns.
3. The client sends `AgentRespondPermission` for a pending request or `AgentCancel`
   for a running turn. The driver performs the requested operation.
4. On attach or a gap, `AgentReplay` asks for the stream from a sequence number.
   The daemon serves retained items or rebuilds the missing history for that client.

The driver, replay and reducer live in `crates/zz-daemon/src/agent/host.rs`,
`crates/zz-daemon/src/agent/fanout.rs` and `crates/zz/src/agent/controller.rs`.
A vendor TUI running in a terminal uses the terminal path. [Agent panes](/concepts/agent-panes.md)

## Commands and configuration

The window's prefix handler can claim the tmux prefix even when a composer or an
address bar has focus. The daemon resolves its key tables. An unbound prefix key
leaves the table, while an unbound root key can reach the pane. `ChromeKeymap`
resolves client actions. CLI commands reach the same mux command engine.
[Key tables](/tmux/key-tables.md) · [Commands](/tmux/commands.md)

The desktop loads and watches its platform's `zz/config`. Local chrome settings
apply in the client. `SetConfigOverrides` carries ordered appearance and mux
overrides to the local daemon. The daemon loads `zz/mux.conf` or explicit `-f`
files, then reapplies overrides on reload. [App config](/configuration/app-config.md) · [Terminal appearance](/terminal/appearance.md)

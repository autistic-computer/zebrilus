---
type: Architecture
title: Named terminal peers
description: How Claude Code on the same Unix host discovers and sends messages to a named terminal pane.
resource: crates/zz-daemon/src/agent/claude_peers.rs
tags: [architecture, agent, messaging, claude-code]
timestamp: 2026-11-24T00:00:00Z
---

With the daemon's `agent` feature on Unix, a pane-scoped `@name` makes a terminal
discoverable through Claude Code's local peer registry. Incoming peer messages submit
input to the process in that terminal. Name a pane intended to receive prompts.

```sh
zz set-option -p -t %3 @name reviewer
zz set-option -pu -t %3 @name
```

## Registration

`Shared::update_terminal_peer` in `crates/zz-daemon/src/daemon.rs` checks the pane kind,
pane option and live shell PID. If Claude Code already owns a matching record, it keeps
its own registration. Matching checks the pane component and process ancestry because
pane numbers repeat across servers.

`PeerInbox::register` writes `<shell-pid>.json` under
`$CLAUDE_CONFIG_DIR/sessions` (default `~/.claude/sessions`). The record identifies
the name, working directory, tmux target, PID/start time and a local Unix socket.
It advertises peer protocol 1 with an empty feature list and a zz terminal marker.
The socket has mode 0600. Changing the name updates the record. Unsetting the option
or closing the peer removes the record and socket. Startup sweeps stale zz registrations.

## Delivery

The listener passes a user message's content, including its attribution, to
`Shared::deliver_to_terminal_pane`. Its callback ignores control messages.
An ordinary recipient receives a paste and submit. For a Codex terminal,
`agent/codex_queue.rs` resolves the title's thread name and the pane's directory
against the app server's thread list, then calls `codex queue --thread ID --message TEXT`.
An unnamed thread gets its first prompt through the terminal. A failed or ambiguous
lookup for a named thread stops delivery.

Native [agent panes](/concepts/agent-panes.md) accept `agent-send` through the daemon's
chat runtime. For a recognized terminal agent, a new `agent-send` prompt asks the caller
to flip the pane to chat. Remote chats are addressed through `zz --host NAME agent-send`.
The peer record and socket described here belong to one machine.

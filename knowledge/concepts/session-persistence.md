---
type: Concept
title: Session persistence and daemon lifecycle
description: What survives detach, which state belongs to each viewer and what ends when the daemon restarts.
resource: crates/zz-daemon/src/daemon.rs
tags: [daemon, persistence, attach, detach, lifecycle]
timestamp: 2026-11-26T00:00:00-03:00
---

## Lifetime

Sessions live in the daemon's memory. A client can detach, lose its connection or
quit while another client watches the same session and its terminals and native
chats keep working. Restarting the daemon, killing it or rebooting ends the mux
sessions and their PTYs. A new daemon can run its startup configuration and
create a numeric session on its first default interactive attach.

| State | Across client detach | Across daemon restart |
|---|---|---|
| Sessions, windows, layout, pane IDs, paste buffers and runtime options | Retained in the daemon | A new daemon starts fresh and loads config |
| PTY process, screen and scrollback | Continue in the terminal actor | Lost |
| Native chat descriptor, host, pending turn and permission | Continue in the daemon | The mux pane and runtime end. Vendor sessions can be resumed where supported. |
| Chat history | A client can replay the stream when it returns | The CLI can retain its session, and zz's optional disk journal can retain replay data keyed by provider and session. Neither restores the mux layout. |
| Browser tabs, URLs and selected profile | The daemon retains descriptors. A client recreates its browser from them. | Pane descriptors end with the session |
| Chromium page state | Lives in the browser instance while its client keeps it open | A newly opened page starts a fresh renderer |
| Browser profile data, recent pages and main-window bounds | Stored on the client | Their files remain |

See `AgentJournal::replay_for` in `crates/zz-agent/src/journal.rs`,
[browser profiles](/browser/profile.md) and [agent panes](/concepts/agent-panes.md).

## Daemon lifecycle

By default, an unused empty daemon can wait for its first session. Once it has
had a session, closing the last pane can leave an empty workspace for a connected
interactive client. With zero sessions and subscribers it can exit. Explicit
`exit-empty`, `exit-unattached` and `destroy-unattached` options change lifetime
policy. The desktop setting `quit-daemon-on-exit = true` opts into stopping sessions
on quit. `kill-server` stops an existing daemon without starting a missing one.
[Process model](/architecture/process-model.md) · [App config](/configuration/app-config.md)

### Guarded recovery of an incompatible daemon

Handshake checks the wire epoch and revision floors. Clients can use compatible
peers of a different revision and negotiate optional messages by capability. To
replace an incompatible local daemon, the recovery path checks the socket and
the recorded process identity before termination. If it cannot establish ownership
of the endpoint, automatic recovery stops. On Unix it also checks that the
socket identity stays the same across connection and cleanup. Restarting ends
all sessions on that daemon. See `terminate_incompatible_daemon` in
`crates/zz-daemon-client/src/lifecycle.rs`. [Wire protocol](/protocol/wire-protocol.md) · [Daemon update](/designs/daemon-update.md)

### Restarting onto a newer build

`zz update` installs a newer headless copy. The running daemon continues using its
current build. `zz update --restart` requests a restart and reports the affected
sessions. `restart-server` starts the successor on the same endpoint with a new
server identity. On Unix the start lock spans the handover. See `restart_server`
in `crates/zz-cli/src/update.rs` and `crates/zz-config/src/owned.rs`.

## Attach and detach

Many clients can attach to one session. One connection can be attached to one
session at a time. Registration alone does not select or create a session.
Attach seeds a snapshot stamped for the client, subscribes its terminal views
and sends current content. Changing sessions releases its previous attachment.
`attach-session -d` evicts other attached clients. Destroying a session sends a
detach event to its viewers. A regular detach can keep the connection open for
a later attach. Disconnection permanently releases that client's resources.

Per-client state includes the focused window, key and prefix state, pending
prompts, reported geometry, terminal selection, scroll anchor, search and copy
mode. Layout, active pane, zoom, process and scrollback belong to the workspace.
`snapshot_presence` reports attached devices and the window each is viewing.

PTY sizing uses `window-size`. Latest uses one geometry owner, largest and
smallest aggregate eligible clients' dimensions, and manual uses the stored
extent. `aggressive-resize` narrows eligibility to viewers of the window.
See `terminal_resize_for_pane` in `crates/zz-daemon/src/daemon/terminal_geometry.rs`.

A lost connection detaches that client. A reconnecting desktop host can keep
the last snapshot visible and reattach to the same session, or to the default
when the old session has gone. [Fleet attach](/designs/fleet-attach.md)

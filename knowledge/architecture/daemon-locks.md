---
type: Architecture
title: Daemon lock order
description: Which daemon mutexes are taken before the server state lock and which after it, with a code site for each.
resource: crates/zz-daemon/src/daemon.rs
tags: [architecture, daemon, locking, concurrency]
timestamp: 2026-11-26T00:00:00-03:00
---

# The rule

`SharedServer::inner` is a `parking_lot::Mutex<ServerState>`. It is not re-entrant: a thread
that holds it and locks it again deadlocks. Code that already holds the guard calls free
functions that take `&ServerState` or `&mut ServerState`, not `Shared` methods that lock `inner`
themselves.

Other mutexes on `SharedServer` fall on one side of `inner`. Take a mutex from the first table
before `inner`, never while holding it. A mutex from the second table may be taken while
`inner` is held, so never lock `inner` while holding one of them. Code comments are not allowed in
this repo, so this page is where the order is written down. Each row names one site that holds
both locks. Paths are relative to `crates/zz-daemon/src`.

The crate map is in [zz-daemon](/crates/zz-daemon.md).

# Taken before `inner`

| Mutex | What it serializes | Site that holds it, then locks `inner` |
| --- | --- | --- |
| `snapshot_order` | Snapshot publication, one publisher at a time | `daemon/publication.rs` `publish_mux_snapshots_except`; `daemon/clients.rs` `send_attached` |
| `pipe_effects` | `pipe-pane` changes, one at a time | `daemon/pane_pipes.rs` `pipe_pane` |
| `agent_effects` | Agent runtime changes | `daemon/agent_runtime.rs` `reconfigure_agents` (through `agent_spawn_config`), `close_agent_panes` |
| `agent_peers` | Claude peer inboxes | `daemon/agent_runtime.rs` `update_terminal_peer`. Taken after `agent_effects` in `close_agent_panes` and `shutdown_agents` |
| `default_attach_effects` | Creating the default session on attach | `daemon/attach.rs` `attach_target_with_materialization_observer` |
| `prompt_history_source` | Loading prompt history | `daemon/history_io.rs` `ensure_prompt_history` (the load callback) |
| `prompt_history_effects` | Saving prompt history | `daemon/history_io.rs` `persist_prompt_history_with_observer` |
| `shutdown_announced` | Sending `ServerStopping` once | `daemon/shutdown.rs` `announce_shutdown` |

So the order for the agent locks is `agent_effects`, then `agent_peers`, then `inner`.

# Taken after `inner`

| Mutex | What it guards | Site that holds `inner`, then locks it |
| --- | --- | --- |
| `tmux_shim` | The `tmux` wrapper directory | `daemon/mux_effects_panes.rs` `apply_pane_effect` |
| `startup_ready` | Whether startup finished | `daemon/shutdown.rs` `should_shutdown_if_empty`, called with the guard from `daemon/mux_effects.rs` `execute_with_mux_source_inner` |
| `pasted_images` | Pasted images per pane | `daemon/paste_uploads.rs` `stage_pending_pasted_image` |
| A `TerminalSession`'s internal locks | Terminal actor state in [zz-terminal](/crates/zz-terminal.md) | `daemon.rs` `donor_working_directory` reads the foreground process through `foreground_process_id` |
| `command_item` (on `Shared`) | The running command's queue item | `daemon/wait_channels.rs` `wait_for_in`; `daemon/hook_events.rs` `wait_for_event` |
| `Attachment::border_cycle` | Clients whose borders cycle | `daemon.rs` `ServerState::border_cycling`; `daemon/publication.rs` `stamp_snapshot_for_client_with` |
| `Publication::border_presentations_cache` | Cached border presentations | `daemon/border_presentations.rs` `cached_live_border_presentations` |
| `Publication::status_parameters_cache` | Cached status parameters | `daemon/status_requests.rs` `status_parameters` |
| `Publication::status_preparation_cache` | Cached status preparation | `daemon/status_requests.rs` `status_request_with_selected_options` |

The last three and `border_cycle` live inside `ServerState`, so they can only be reached through
the `inner` guard.

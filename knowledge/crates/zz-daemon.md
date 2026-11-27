---
type: Rust Crate
title: zz-daemon crate
description: Map of the daemon crate, from connection to command routing, effect application and publication, with the state layout and the file that owns each concern.
resource: crates/zz-daemon/src/daemon.rs
tags: [crate, daemon, ipc, fanout, transport, agent, map]
timestamp: 2026-11-27T00:00:00-03:00
---

# What the crate owns

`Daemon::serve` owns the mux engine, session attachments, PTY sessions and agent runtimes.
Closing a client releases its subscriptions and terminal views. Under the default exit policy
a remaining session keeps its processes alive for another client. A daemon restart ends them.
The daemon retains browser pane descriptors while a client owns the renderer.
See [session persistence](/concepts/session-persistence.md), [mux state](/crates/zz-mux.md)
and [agent panes](/concepts/agent-panes.md).

The local listener uses a Unix-domain socket or a Windows named pipe. Remote routes deliver
the same framed stream. The [client library](/crates/zz-daemon-client.md) owns the dialer,
SSH process or mobile SSH channel, and prompts.

Paths below are relative to `crates/zz-daemon/src`.

| File | Holds |
| --- | --- |
| `lib.rs` | Crate exports, pane and shell job environment for the bundled `tmux` wrapper, `shell_process` |
| `daemon.rs` | `Daemon` and `Daemon::serve`, `Shared`, `SharedServer`, `ServerState` and its group structs, `Client`, helpers several subjects share, the module list |
| `daemon/*.rs` | One file per subject. Each adds an `impl Shared` block or free functions over `ServerState` |
| `status.rs` | The status types whose fields several status files share (`StatusRenderer`, `StatusRequest`, `DaemonFormatHooks`, the completed status, status parts and shell job cache entries), the context format lists, cycle frame tracking and the status module list |
| `status/*.rs` | One file per status subject, listed under [Status](#status) |
| `keys.rs` | Key names and key actions for choosers, overlays and prompts |
| `wake.rs` | `AcceptWake` and the loop thread wake |
| `tailnet.rs` | The tailnet listener and `zz tailnet` command |
| `agent_fork.rs`, `agent_hooks.rs` | Forking a Claude Code or Codex session on disk, and the agent CLIs' session hooks |

# How a request flows

1. **Connection.** On Unix, `EventLoop` in `daemon/event_loop.rs` accepts and reads with mio.
   `Session` in `daemon/connection.rs` takes the hello and dispatches each message. On Windows,
   `accept_connections` and `handle_connection` in `daemon/daemon_socket.rs` run a thread per
   connection, and `handle_connection_message` in `daemon/connection_messages.rs` dispatches.
   One-shot `zz` commands go through `LoopExec` (Unix) or `serve_exec` (Windows) in `daemon/exec.rs`.
2. **Command request.** A `CommandRequest` reaches `execute_command_request_with_prepared_into` in
   `daemon/command_requests.rs`. It runs inside the client's command item (`daemon/cmdq.rs`), splits
   the request into segments and writes the `CommandResponse`.
3. **Routing.** The `execute*` family in `daemon/command_routing.rs` resolves aliases, caller stdin
   and mouse targets. Commands the daemon owns (`DAEMON_COMMAND_DISPATCHES`: `run-shell`,
   `capture-pane`, `agent-send`, `display-popup`, `wait-for` and the rest) go to their subject file
   and never reach the engine.
4. **Engine execution.** `Shared::execute_with_mux_source_inner` in `daemon/mux_effects.rs` locks
   `inner`, calls `MuxEngine::execute_without_alias_expansion` in [zz-mux](/crates/zz-mux.md), and
   gets back an `Execution` with output and a list of `MuxEffect`s.
5. **Effect application, under the lock.** Each `MuxEffect` goes to one helper by subject:
   `apply_pane_effect`, `apply_view_effect`, `apply_key_effect`, `apply_overlay_effect`,
   `apply_message_effect`, `apply_option_effect`, `apply_client_effect` and `apply_flag_effect` in
   `daemon/mux_effects_{panes,views,keys,overlays,messages,options,clients,flags}.rs`. They change
   `ServerState` and record follow-up work in a `CommandAftermath`. Hook events are computed here
   from the before and after state.
6. **Post-lock drain.** After the lock is released, `execute_with_mux_source_inner` runs the
   named steps in `daemon/mux_effects_drain.rs` in this order: `drain_server_effects`,
   `drain_terminal_effects`, `drain_mode_effects`, `drain_detach_effect`, `drain_attach_effect`,
   `drain_watched_terminals`, `drain_client_events`, `drain_alert_effects`, `drain_agent_effects`,
   `drain_publication_effects`, `drain_stream_inputs`. Then `run_source_effects` in
   `daemon/mux_effects_sources.rs` reads and replays sourced files, reloads config, runs the
   pending event hooks and submits a pending command prompt.
7. **Publication.** `daemon/publication.rs` sends snapshots, resyncs and events to each client's
   `OutboundMailbox` (`daemon/outbound_mailbox.rs`). Clients on the control protocol get scoped
   tree deltas from `daemon/ctrl.rs`. Terminal frames go through `daemon/shard_sink.rs` and
   `daemon/terminal_frames.rs`. The event loop (Unix) or a writer thread per client (Windows,
   `spawn_writer` in `daemon/attach.rs`) writes the mailbox to the socket.

Input that is not a command (keys, mouse, text, resize) enters at `Shared::input` in
`daemon/input.rs`. Key bindings resolve in `daemon/key_input.rs` and run their commands through
the same routing.

The locks taken along this path, and their order, are in [daemon lock order](/architecture/daemon-locks.md).

# State layout

`Shared` is a cheap handle: an `Arc<SharedServer>` plus per-command fields (the command item,
a link to the owning `Shared`, deferred watcher effects). It derefs to
`SharedServer`, which holds the side mutexes, channels, atomics and `inner: Mutex<ServerState>`.
`ServerState` is the state guarded by `inner`. Its fields:

| Field | Type | Holds |
| --- | --- | --- |
| `engine` | `MuxEngine` | Sessions, windows, panes, options, key tables, hooks ([zz-mux](/crates/zz-mux.md)) |
| `clients` | `BTreeMap<ClientId, Box<Client>>` | Per-client state: identity, size, terminal facts, overlays (prompt, menu, choosers, popup), copy session, key engine, subscriber mailbox, control-mode output and subscriptions |
| `terminals` | `Arc<BTreeMap<PaneId, Arc<TerminalSession>>>` | The live terminal actor of each pane |
| `attachment` | `Attachment` | Which clients are attached to which session, client flags, control routes, cold bootstrap lease, last-attached times, border cycling set |
| `panes` | `PaneTables` | Pane locations, terminal geometry per client, pane exit waits, reported places, read observations, silence deadlines, name checks, pending terminal spawns, popup panes |
| `modes` | `Modes` | Pane mode requests and transitions, panes-mode frames, panes to kill after a chooser, copy mode or pane mode, editor sessions, open pane prompts |
| `config` | `Config` | Loaded config file list, terminal appearance and its provenance, color scheme, mux options with their overrides and underlay |
| `publication` | `Publication` | Last published mux generation, key table generations, cached control trees, and the border, status parameter and status preparation caches |
| `jobs` | `Jobs` | `pipe-pane` pipes, running shell jobs, copy-pipe and shell job counts, tokens |
| `buffers` | `Buffers` | Paste buffers, the automatic buffer limit, next buffer id |
| `waits` | `Waits` | Event waiters, deferred event hooks, `wait-for` channels, captured control command events |
| `history` | `History` | Command and search prompt history, the server message log |
| `transfers` | `Transfers` | Pending GUI requests, paste uploads, client file waiters, file writes |
| `artifact_state` | `ArtifactState` | The `html` artifact registry, whether artifacts are watched, published simulator descriptors |
| `agents` | `Agents` | Agent pane set, published agent parents, Claude peer states, armed flips and accounts, CLI exits, forks |

# Which file owns what

All paths are under `crates/zz-daemon/src/daemon/`. Files marked (Unix) are compiled only on
Unix; files marked (agent) only with the `agent` feature.

## tmux command pipeline

| File | Owns |
| --- | --- |
| `command_requests.rs` | Command requests from clients: command items, request segments, prepared command lists, environment and home directory resolution |
| `command_routing.rs` | The `execute*` entry points, daemon-owned command dispatch (`daemon_command_dispatch`), background insertions |
| `mux_effects.rs` | `execute_with_mux_source_inner`: engine call under the lock, `CommandAftermath`, effect dispatch, hook event capture |
| `mux_effects_panes.rs` | Pane created, respawned, removed, relocated and agent pane effects |
| `mux_effects_views.rs` | Terminal view, copy mode source and kill, mouse drag effects |
| `mux_effects_keys.rs` | `send-keys`, client keys and pane reset effects |
| `mux_effects_overlays.rs` | Command prompt, choose-tree, choose-buffer, display-panes, path picker and sidebar focus effects |
| `mux_effects_messages.rs` | `display-message` and print-or-message effects |
| `mux_effects_options.rs` | Option change effects: buffer limit, word separators, mode keys, window size, status formats |
| `mux_effects_clients.rs` | Attach, detach and suspend effects |
| `mux_effects_flags.rs` | Effects that record work for the drain: source-file, run-hook, reload, config import, kill-server, snapshot, style and pane mode changes |
| `mux_effects_drain.rs` | The post-lock drain steps |
| `mux_effects_sources.rs` | `run_source_effects`: sourced files, config reload, pending event hooks, prompt submission |
| `cmdq.rs` | Command items, queue ids and wait continuations |
| `wait_queue.rs` (Unix) | Command and inserted tasks parked on the event loop |
| `inserted_frames.rs` | Commands run as children of another command: alias groups, hook commands, `run-shell` and `if-shell` commands, sourced commands |
| `loop_handoff.rs` (Unix) | Handing a command off the loop thread to a worker |
| `command_stdout.rs` | Command stdout and stderr records, caller stdin streamed to panes (`CommandStdinSink`) |
| `command_output.rs` | The command output view shown to an interactive client, control command event capture |
| `effect_clients.rs` | Which client an effect targets: display-message client, detach victims, preview clients |
| `deferred_terminal_commands.rs` | Terminal changes queued under the lock and sent to the terminal after it |
| `capture.rs` | `capture-pane` |
| `inspect.rs` | `inspect` |

## tmux overlays and modes

| File | Owns |
| --- | --- |
| `overlays.rs` | `display-popup`, `display-menu`, the popup editor, overlay waits and overlay styles |
| `prompts.rs` | `command-prompt` and `confirm-before` |
| `history_io.rs` | Prompt history load and save, `show-messages`, timed client messages |
| `choose_tree.rs` | `choose-tree` sessions, rows, search and kill prompts |
| `choose_buffer.rs` | `choose-buffer` sessions |
| `chooser_presentation.rs` | Chooser rows, previews and styles sent to clients, chooser keys |
| `display_panes.rs` | `display-panes` |
| `pane_modes.rs` | Pane modes (tree, customize, clock), their keys and mouse, mode kill panes |
| `copy_mode.rs` | Copy sessions and which keys copy mode owns |
| `mode_requests.rs` | Status line mode requests for copy mode and command output views |

## Shell jobs and pipes

| File | Owns |
| --- | --- |
| `shell_jobs.rs` | `run-shell`, `if-shell`, delays, shell job spawn, output routing and reaping |
| `jobs.rs` (Unix) | The event loop's child process and pipe registry |
| `pipe_jobs.rs` (Unix) | Job launch requests from other threads to the loop |
| `pane_pipes.rs` | `pipe-pane` and copy-pipe processes |

## Buffers

| File | Owns |
| --- | --- |
| `buffers.rs` | Paste buffer commands, buffer lookup, buffer paths, the automatic buffer limit |
| `clipboard.rs` | Clipboard writes to clients and clipboard queries |
| `paste_uploads.rs` | Pasted text and image uploads from clients, pasted image storage per pane |
| `file_commands.rs` | `load-buffer` and `save-buffer` through the client, `read-file`, `write-file`, client file waiters |

## Hooks and waits

| File | Owns |
| --- | --- |
| `hook_commands.rs` | Command hooks, event hooks, client lifecycle hooks, deferred control notifications |
| `hook_events.rs` | Computing hook events from a before and after view of the mux, event waiters, read-only command checks |
| `hook_queue.rs` (Unix) | Event hooks queued on the loop |
| `wait_channels.rs` | `wait-for` channels |
| `pane_exit.rs` | Waiting for a pane's command to exit |
| `agent_waits.rs` | Waiting for a terminal agent's state to change |

## Config

| File | Owns |
| --- | --- |
| `config.rs` | Building `Shared` (`configured`), mux option overrides, effective options, daemon environment and default shell |
| `config_loading.rs` | Startup config, reload, tmux config import, source and config error routing |
| `config_frames.rs` | Replaying a config file as queue frames |
| `source_queue.rs` | Reading and running sourced files on the queue |
| `source_paths.rs` | `source-file` path expansion, globs and read error text |
| `appearance.rs` | Terminal appearance and themes per pane, worker options |

## Clients and attach

| File | Owns |
| --- | --- |
| `clients.rs` | Client registration, subscribe, unregister, `list-clients`, `switch-client`, `refresh-client`, control client flags and size |
| `attach.rs` | Attach, detach, destroy-unattached, held attach terminals, Windows writer threads |
| `client_facts.rs` | Per-client format facts: tty, size, features, environment, effective options |
| `client_targets.rs` | Resolving a client target, best client for a session, sizing candidates |
| `input.rs` | Client input entry: text, size reports, focus, committed text, input sinks |
| `key_input.rs` | Key and mouse input, key table decisions, prefix, key command execution |
| `pane_focus.rs` | Pane focus probes and focus hook events |
| `window_extents.rs` | Window size from attached clients |
| `ctrl.rs` | Scoped tree deltas, client views and batched replies for clients on the control protocol |
| `control_output.rs` | Control-mode pane output, its pacing and age |
| `control_stdio.rs` (Unix) | Control-mode clients over the caller's stdio, a submodule of `event_loop` |
| `tty_input.rs` (Unix) | Reading keys from a client's tty, a submodule of `event_loop` |

## Publication and frames

| File | Owns |
| --- | --- |
| `publication.rs` | Snapshots, resyncs, full sends, history, key tables, per-client and broadcast events, snapshot stamping |
| `outbound_mailbox.rs` | `OutboundMailbox`: ordered reliable messages, per-pane coalesced terminal frames, images |
| `terminal_frames.rs` | `PaneFrameFanout` and `TerminalFrames`: encode a patch or full view once and share it |
| `shard_sink.rs` | `PaneSink` between terminal actors and subscribers, control-mode wake |
| `kitty_images.rs` | Kitty image frames per viewport |
| `border_presentations.rs` | Pane border presentations and their cache |

## Terminals and panes

| File | Owns |
| --- | --- |
| `pane_runtime.rs` | Watching a terminal, closing an exited one, syncing title, progress, program status and path |
| `pane_summary.rs` | Pane locations and program sightings that sidebar and status views read |
| `terminal_views.rs` | Which panes each client sees and streams, input sinks, control routes, view areas |
| `terminal_geometry.rs` | Terminal geometry per client and pane, resizes |
| `terminal_reads.rs` | `send-text`, `wait-pane`, `run-pane`, pane output observation, mouse bindings |
| `terminal_requests.rs` | Pending terminal requests polled on the loop, `CommandWait` |
| `alerts.rs` | Bells, activity, silence and shell marks |

## Agent adapter

The [zz-agent crate](/crates/zz-agent.md) runs agents; the `agent` feature pulls it in and
`crate::agent` names it. These files connect it to the daemon. See
[agent driver contracts](/references/agent-cli-contracts.md).

| File | Owns |
| --- | --- |
| `agent_runtime.rs` (agent) | Building and reconfiguring the runtime, opening and closing agent panes, Claude peer states |
| `agent_commands.rs` | `agent-send`, `agent-respond`, `agent-catalog`, `send-text`, `wait-pane`, `run-pane`, `send-last-output` |
| `agent_chats.rs` (agent) | Chat delivery, child chats, chat replies |
| `agent_publisher.rs` (agent) | Publishing agent state, timeline and replies to clients |
| `agent_relay.rs` (agent) | Relaying `agent-send` to a chat on another host |
| `agent_inbox.rs` (agent) | Draining the runtime's published messages and applying them |
| `agent_flip.rs` | `flip-agent`: switching between a terminal CLI and chat |
| `agent_account.rs` | `agent-account`: switching a provider's login |
| `agent_login.rs` (agent, Unix) | Driving a vendor CLI's login |
| `accounts.rs` | `agent-accounts`, quotas, account identity; `handle_agent_login` |
| `providers.rs` | `providers`: finding, versioning and updating agent CLIs |

## Artifacts and files

| File | Owns |
| --- | --- |
| `artifacts.rs` | `html` artifacts: registry, tokens, serving and reloading pages |
| `file_edit.rs` | Client file reads and writes, saved attachments |
| `path_listing.rs` | Streaming directory listings with git marks for the path picker |
| `gui_requests.rs` | `load-browser-url`, `capture-browser`, requests a GUI client answers |
| `simulators.rs` | Simulator pane commands |
| `workspace_tools_text.rs` | The text `zz tools` prints |

## Lifecycle and transport

| File | Owns |
| --- | --- |
| `daemon_socket.rs` | Socket preparation, start lock, socket guard, permissions; Windows accept and per-connection handling |
| `event_loop.rs` (Unix) | `EventLoop`: accept, read, dispatch, exec, jobs, timers and the shutdown phases |
| `connection.rs` (Unix) | `Session`: hello, initialize, per-message dispatch |
| `connection_messages.rs` | `handle_connection_message` (Windows), hello validation, error replies |
| `exec.rs` | One-shot commands: `LoopExec` (Unix), `serve_exec` (Windows), connection worker threads |
| `flow.rs` (Unix) | `FlowWindow` for remote clients ([remote flow window](/designs/remote-flow-window.md)) |
| `helpers.rs` | Helper thread pool for blocking reads, file operations, path listings, peer scans, history load |
| `lifecycle.rs` (Unix) | Client releases, shutdown requests and hooks posted to the loop |
| `startup.rs` | Startup readiness, initialization, `tmux` wrapper install, terminfo warmup |
| `shutdown.rs` | Shutdown requests, blockers, announcement, writer drain, `should_shutdown_if_empty` |
| `restart.rs` | `restart-server` and the `Stopped` result |
| `update_server.rs` | `update-server` |
| `tmux_shim.rs` (Unix) | The `tmux` wrapper directory that panes and jobs run |
| `timers.rs` | Loop timers: key table, silence, message and publish deadlines |
| `watchers.rs` | Terminal and command output watchers on the loop |

## Status

| File | Owns |
| --- | --- |
| `status_jobs.rs` | Status `#()` format jobs and their output |
| `status_refresh.rs` | Status refresh, control-mode subscriptions, format monitors, diagnostic snapshot |
| `status_requests.rs` | Status requests per client and their caches |
| `format_facts.rs` | Format facts borrowed from `ServerState` for hooks and format expansion |

The status line itself is `status.rs` and the files under `status/`, outside `daemon/`:

| File | Owns |
| --- | --- |
| `status/renderer.rs` | `StatusRenderer` methods, status contexts, and composing the rows, title and styles of a `StatusLine` |
| `status/hooks.rs` | `DaemonFormatHooks`: the `StatusHooks` implementation, pane output and program status variables, buffer samples, content search |
| `status/facts.rs` | `FormatHookFacts`, the `FormatFactSource` trait, buffer, client, viewport and message facts |
| `status/parts.rs` | Status parts and the completed status cache, callbacks and their retained bytes |
| `status/formats.rs` | Mode line formats, `expand_style`, theme colour slots and the status template inventory |
| `status/terminfo.rs` | Terminfo entries read through `infocmp`, their warm cache and client terminal facts |
| `status/shell_jobs.rs` | Polling `#()` shell job cache entries and storing their output, starting a job and choosing its working directory |

Their tests are `status/<subject>_tests.rs` beside them, with shared fixtures in
`status/test_support.rs`. `status/format_status_tests.rs` covers status expansion end to end.

## Tests

Tests that drive a whole `Shared` are in `daemon/tests/`, one module per subject (`buffers.rs`,
`prompts.rs`, `shell_jobs.rs`, `agent.rs` and so on), shared fixtures in `daemon/tests/mod.rs`.
Tests of one module sit next to it as `daemon/<subject>_tests.rs`; those for the event loop are
`daemon/event_loop_<subject>_tests.rs`. Those for `ctrl.rs` are `daemon/ctrl_<subject>_tests.rs`
(attach, attach frames, config, exec, flags, frame groups, quiet queries, resize and
subscriptions), with the compact client fixtures in `daemon/ctrl_test_support.rs`.
Process-level tests are in `crates/zz-daemon/tests/`.

# Platform and features

A new incoming request must reach the dispatcher on each supported platform: `connection.rs` on
Unix and `handle_connection_message` on Windows. Some services have narrower platform support. The
daemon defaults to the `agent` and `tailnet-own` features. The embedded tailnet dependency is
available on Linux and arm64 macOS. `daemon/agent_login.rs` is built with `agent` on Unix. The
hello can still advertise `agent-login-v1` on Windows or in an agent-disabled build, where
`handle_agent_login` in `daemon/accounts.rs` discards the request. Consult the feature and target
sections in `crates/zz-daemon/Cargo.toml` and the module list in `daemon.rs` when a capability has
different backend support by platform.

For a server-only build check without agents, run
`cargo clippy -p zz-daemon -p zz-daemon-client --no-default-features --all-targets -- -D warnings`.
See [wire compatibility](/protocol/wire-protocol.md) before changing the handshake or publication.

# Mux state and delivery

- The same session can have several viewers with their own focus, overlay, read-only state and
  terminal view. The daemon executes the mux's effects and sends a scoped tree plus
  recipient-specific state. See [snapshots](/protocol/snapshots.md) and
  [multi-device attach](/designs/multi-device-attach.md).
- `OutboundMailbox` has a reliable path for ordered control traffic and coalesces terminal
  frames per pane. `PaneFrameFanout` shares an encoded patch for a matching transition.
  `TerminalFrames` caches encoded full views. A client whose patch base is unavailable can
  request a full viewport for that pane. See [terminal lanes](/protocol/terminal-lanes.md).
- Terminal state belongs to [zz-terminal](/crates/zz-terminal.md). See
  [PTY workers](/concepts/pty-worker.md) and [status line](/tmux/status-line.md).

# When the daemon stops

`Shared::should_shutdown_if_empty` first checks for subscribed clients. With default settings
a fresh empty daemon can wait for its first session. Once session creation arms the empty
check, an empty daemon with no subscribers exits. Explicit `exit-empty` and `exit-unattached`
change the rule once startup is ready. `exit-empty off` disables this automatic shutdown.
`kill-server` requests a forced shutdown. `restart-server` returns a restart disposition
to [zz-cli](/crates/zz-cli.md), which launches the successor.

# Reaching a remote daemon over ssh

The client's `endpoint.rs` probes the remote socket and binary. On macOS/Linux the client can
start a missing daemon and forward a local Unix socket to it. Windows uses SSH stdio and
`zz proxy`. iOS and Android use an in-process SSH channel. [Endpoint and transport
details](/crates/zz-daemon-client.md).

## Installing zz on a remote

`zz fleet add NAME user@host` records the destination. `zz fleet install NAME` installs the
CLI there. `zz --host NAME build-info` inspects the remote binary and `zz --host NAME list-sessions`
queries its daemon. Use `zz fleet install --restart-daemon NAME` when the running daemon also
needs replacement. Its sessions end on restart. [Fleet attach](/designs/fleet-attach.md).

The desktop app marks a host on an older zz on its sidebar row and in Settings › Hosts and
updates it on request with the steps `zz update <host> --restart` runs. Nothing opens on connect.
See [host updates in the app](/designs/host-update-ui.md).

## Connection sharing

Desktop Unix clients can reuse an OpenSSH control master. An existing master receives
`ssh -O forward`. A standalone forwarding child uses `-N -n` with no command input.

## Kept forwards

Repeated CLI calls can reuse the same forwarded socket. `connect_kept` serializes recreation
and probes the replacement before a waiting caller sends its command. See `endpoint.rs` in
[zz-daemon-client](/crates/zz-daemon-client.md).

## zz's own key

`zz fleet key` prints the machine's public key. `zz fleet sync` distributes fleet keys.
`zz fleet trust KEY` enrolls the supplied public key. `fleet_sync.rs` in the
[client library](/crates/zz-daemon-client.md) implements those operations.

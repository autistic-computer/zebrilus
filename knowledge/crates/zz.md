---
type: Rust Crate
title: zz desktop crate
description: Native desktop startup, daemon connections, workspace reconciliation, terminal painting, browser sessions and agent chat.
resource: crates/zz/src/lib.rs
tags: [ala, crate, client, terminal, browser, agent, ui]
timestamp: 2026-11-27T00:00:00-03:00
---

# Startup and ownership

The `zz` package builds the ala desktop app. `crates/zz/src/lib.rs` calls
`zz_cli::run_startup` for command dispatch before creating a GUI. Desktop startup bootstraps
CEF, initializes the UI and opens the workspace. macOS needs the framework in the app bundle.
On Windows the bundled bootstrap executable loads `zz.dll` and supplies the CEF sandbox
information. The standalone Rust binary handles windowless verbs. See
[zz-cli](/crates/zz-cli.md), [CEF bundling](/crates/zz-xtask.md) and
[process model](/architecture/process-model.md).

The app is a client of the daemon on the selected host. When a window closes, the daemon
continues to own its sessions, terminal processes and agent runtimes. Browser tabs and
profile selection are described in mux state. Live browser sessions and their frames belong
to the desktop process. See [session persistence](/concepts/session-persistence.md).

| Source under `crates/zz/src` | Responsibility |
| --- | --- |
| `app_shell.rs`, `window/` | Native window chrome |
| `workspace/view.rs`, `workspace/view/` | The workspace window: focus, overlays, drags and pane reconciliation. See [workspace view](#workspace-view). |
| `mux/client.rs`, `mux/client/` | `MuxClient`: connections, per-host snapshots, attachment and reconnect. See [mux client](#mux-client). |
| `terminal/view.rs`, `terminal/element.rs` | Bind pane input, history, selection and paint to the retained grid |
| `browser/controller.rs`, `browser/view.rs`, `browser/element.rs` | Create sessions, route tab and browser input, present off-screen frames from [ala-cef](/crates/ala-cef.md) |
| `agent/controller.rs`, `agent/view.rs` | Consume daemon agent events, show chat state and send prompts, answers or actions |
| `workspace/sidebar.rs`, `workspace/sidebar/flat.rs` | Present host, session, window and pane data from the client model |
| `mux/client/host_update.rs`, `workspace/host_update.rs`, `workspace/ssh_prompt.rs` | Host updates, the question before one, and interactive authentication dialogs |
| `config/mod.rs`, `theme.rs`, `diagnostics/mod.rs` | Desktop preference integration, theme application, logs and main-thread stall sampling |

The `desktop` and `agent-pane` features are on by default. `editor-pane` enables the optional
editor tools. The desktop simulator view is under `agent-pane` with a live macOS implementation
and a stub for other targets (`simulator.rs`). Shared widgets live in `libs/penna`, app UI in
`crates/zz-ui`, and renderer-independent input and projection in [zz-client](/crates/zz-client.md).
The web, iOS and Android shells share `clients/app`.

## Workspace view

`AppView` in `workspace/view.rs` is the workspace window. It owns the panes' views, focus,
overlays and drags, and `AppView::synchronize_panes` reconciles the rendered panes with the
mux tree. Its methods live in subject files under `workspace/view/`.

| Source under `crates/zz/src/workspace` | Responsibility |
| --- | --- |
| `view.rs` | `AppView`, its state structs and drag state types, `TerminalHeaderView`, `AppView::new`, `impl Render for AppView`, the stale daemon prompt, shared free helpers and the `view/` module declarations |
| `view/keys.rs` | Window shortcut hints, keystroke interception, chrome actions, which-key and the dialog prefix barrier |
| `view/palettes.rs` | The command palette, the path picker palette and the folder palette |
| `view/focus.rs` | Pane focus targets, the focus audit, focusing the active pane, the visible overlay and pane click focus |
| `view/synchronize.rs` | `synchronize_panes` and the GUI request drain |
| `view/agents.rs` | The pane picker, agent views, agent parking, account switching, pane flips and agent events |
| `view/drag.rs` | Split, float, pip and pane drags, the drag state methods and the split ratio helpers |
| `view/pane_content.rs` | Pane headers, pane content, kind buttons, title menus, simulator and agent views, pane action keys |
| `view/layout.rs` | `render_layout`, pane corners, window control placement, pane indicators and tile size reports |
| `view/floating.rs` | The floating layer, modal scrim, float frames, floating panes, pip headers and float grips |
| `view/overlays.rs` | Menu pointer routing and the menu and confirm overlays |
| `view/*_tests.rs`, `view/test_support.rs` | Tests, one file per subject, and the helpers they share. The test-only views stay in the inline `tests` module of `view.rs`. |

## Mux client

`MuxClient` in `mux/client.rs` manages connections, per-host snapshots, attachment and reconnect.
It reduces ordinary state through `ClientCore` and applies terminal frames to the retained pane
viewport. Its methods live in subject files under `mux/client/`.

| Source under `crates/zz/src/mux` | Responsibility |
| --- | --- |
| `client.rs` | `MuxClient`, `HostConnection`, the kitty image cache and the other client types, constructors and `new_inner`, state accessors, test hooks, color scheme reports, notifications, GUI requests, diagnostics, the event emitters and the `client/` module declarations |
| `client/connection.rs` | Initial connection, host reconciliation and lookup, connecting, ssh prompts, reconnect timers, connect results, installs on hosts and host disconnects |
| `client/daemon_restart.rs` | Dismissing and restarting a stale or outdated local daemon |
| `client/overlays.rs` | Command prompt, choose-tree, choose-buffer, display-panes, menu and confirm state and revisions |
| `client/input.rs` | Sending input, client window focus and its attach epochs, host path listings, paste and prefix cancel |
| `client/commands.rs` | Executing, tracking and answering commands, failure reports and command responses |
| `client/providers.rs` | Providers, agent accounts and quotas, and agent and provider logins |
| `client/attach.rs` | New sessions, attaching to a host or target, releasing hosts, session state resets and finishing an attach |
| `client/images.rs` | Paste uploads, the kitty image cache, pasted images and artifact fetches |
| `client/agent_streams.rs` | Agent updates and replays, watched agent panes, agent events and agent requests |
| `client/messages.rs` | Protocol messages, core events, appearance adoption, open-uri routing, terminal viewports and patches, command output and bells |
| `client/history.rs` | History backfill and prefetch requests and history chunks |
| `client/*_tests.rs`, `client/test_support.rs` | Tests, one file per subject, and the helpers they share. The test-only kitty views stay in the inline `tests` module of `client.rs`. |

## Running the desktop client

`just run mac` or `just run linux` launches zz Dev with its isolated daemon and data. Add
`--verbose` to capture diagnostics. `just profile build mac` prepares a symbolized bundle
for Instruments. See [running and profiling](/playbooks/running-zz.md),
[terminal appearance](/terminal/appearance.md), [browser rendering](/browser/osr-rendering.md),
[UI conventions](/configuration/ui-conventions.md) and [agent panes](/concepts/agent-panes.md).

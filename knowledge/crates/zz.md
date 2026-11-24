---
type: Rust Crate
title: zz desktop crate
description: Native desktop startup, daemon connections, workspace reconciliation, terminal painting, browser sessions and agent chat.
resource: crates/zz/src/lib.rs
tags: [ala, crate, client, terminal, browser, agent, ui]
timestamp: 2026-11-24T00:00:00-03:00
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
| `app_shell.rs`, `window/`, `workspace/view.rs` | Native window chrome, focus, overlays and `AppView::synchronize_panes`, which reconciles the rendered panes with the mux tree |
| `mux/client.rs` | `MuxClient` manages connections, per-host snapshots, attachment and reconnect. It reduces ordinary state through `ClientCore` and applies terminal frames to the retained pane viewport. |
| `terminal/view.rs`, `terminal/element.rs` | Bind pane input, history, selection and paint to the retained grid |
| `browser/controller.rs`, `browser/view.rs`, `browser/element.rs` | Create sessions, route tab and browser input, present off-screen frames from [ala-cef](/crates/ala-cef.md) |
| `agent/controller.rs`, `agent/view.rs` | Consume daemon agent events, show chat state and send prompts, answers or actions |
| `workspace/sidebar.rs`, `workspace/sidebar/flat.rs` | Present host, session, window and pane data from the client model |
| `workspace/remote_install.rs`, `workspace/ssh_prompt.rs` | Remote install and interactive authentication dialogs |
| `config/mod.rs`, `theme.rs`, `diagnostics/mod.rs` | Desktop preference integration, theme application, logs and main-thread stall sampling |

The `desktop` and `agent-pane` features are on by default. `editor-pane` enables the optional
editor tools. The desktop simulator view is under `agent-pane` with a live macOS implementation
and a stub for other targets (`simulator.rs`). Shared widgets live in `libs/penna`, app UI in
`crates/zz-ui`, and renderer-independent input and projection in [zz-client](/crates/zz-client.md).
The web, iOS and Android shells share `clients/app`.

## Running the desktop client

`just run mac` or `just run linux` launches zz Dev with its isolated daemon and data. Add
`--verbose` to capture diagnostics. `just profile build mac` prepares a symbolized bundle
for Instruments. See [running and profiling](/playbooks/running-zz.md),
[terminal appearance](/terminal/appearance.md), [browser rendering](/browser/osr-rendering.md),
[UI conventions](/configuration/ui-conventions.md) and [agent panes](/concepts/agent-panes.md).

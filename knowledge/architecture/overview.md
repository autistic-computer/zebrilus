---
type: Architecture
title: zz system overview
description: zz multiplexes terminal and Chromium browser panes, with an agent lens over terminals running Claude Code or Codex, over a persistent daemon shared by desktop, web, mobile ala, and terminal clients.
resource: crates/zz/src/lib.rs
tags: [architecture, overview, ala, multiplexer, daemon]
timestamp: 2026-10-31T00:00:00Z
---

# Overview

`zz` is a multiplexer with ala desktop and web clients, experimental ala iOS and Android clients, and a
raw-terminal client over one persistent daemon. Its model contains these pane surfaces:

- a **live local terminal** powered by `libghostty-vt` and a daemon-owned PTY worker
  (see [zz-terminal](/crates/zz-terminal.md));
- a **Chromium browser** powered by CEF Alloy off-screen rendering (OSR), composited as an ala image
  (see [ala-cef](/crates/ala-cef.md));
- an **agent lens** over a terminal pane whose foreground program is Claude Code or Codex: the
  header gets a toggle that swaps the cell grid for the agent timeline of that same pane, read by the
  daemon from the transcript the CLI writes, with a composer that types into the CLI (see
  [agent lens](/concepts/agent-lens.md)). There is no agent pane kind;
- an Editor pane, compiled out behind the `editor-pane` cargo feature as well as
  `experimental-editor-pane`.

The browser is **not** a native child window. CEF publishes frames into a one-slot mailbox; the
default path is a shared GPU texture (Linux wgpu `external_texture`, macOS Metal-IOSurface
`paint_surface`, Windows D3D11). CPU BGRA readback is the fallback tier. See
[OSR rendering](/browser/osr-rendering.md). Pointer, wheel, keyboard, committed text, IME, focus,
resize, navigation, and cursor state travel through browser-neutral types.

Sessions, windows, and binary split-pane layouts live in a **persistent daemon**
([server](/crates/zz-daemon.md)) reached over owner-only local IPC. Desktop clients reach a remote
daemon through an OpenSSH `ssh -L` Unix-socket forward. The ala iOS and Android clients use in-process
`russh` and carries the protocol through `zz proxy` over SSH channel stdio. Every device the user
owns can attach to one session at the same time, each with its own viewport, scroll position, and
focused window, while the layout tree, pane focus, and PTYs stay shared. Closing every ala window
detaches that client without stopping terminal processes; see
[session persistence](/concepts/session-persistence.md). Browser panes
restore their last URL and zz [profile](/browser/profile.md) when a GUI reattaches, and transient
Chromium state does not survive without a GUI. An agent CLI is a terminal process, so its turn keeps
running with every window closed, and a client that opens the lens again gets the timeline rebuilt
from the CLI's own transcript.

# Design pillars

| Pillar | What it means | Where |
|--------|---------------|-------|
| tmux-familiar UX | prefix key tables, a `.tmux.conf` subset, stable target IDs, `send-keys` | [mux](/crates/zz-mux.md), [tmux compat](/tmux/tmux-compat.md) |
| Heterogeneous panes | terminal, browser, picker, and editor panes are uniform for layout/focus/targeting/lifecycle | [split-pane layout](/concepts/split-pane-layout.md) |
| Renderer-free core | mux state machine has no UI/renderer dependency | [mux](/crates/zz-mux.md) |
| Persistent daemon | PTYs + mux state outlive GUI detach; every device attaches to one session at once | [session persistence](/concepts/session-persistence.md) |
| Portable transport | Unix-domain socket on Linux/macOS, named pipe on Windows, OpenSSH forwarding on desktop, and `zz proxy` over in-process SSH on iOS and Android | [fleet attach](/designs/fleet-attach.md) |
| Reimplemented, not linked | tmux behavior ported to safe Rust against a pinned upstream | [tmux upstream](/references/tmux-upstream.md) |

# Crate map

| Crate | Role |
|-------|------|
| [zz-protocol](/crates/zz-protocol.md) | stable IDs, versioned length-prefixed control protocol, packed terminal lanes |
| [zz-mux](/crates/zz-mux.md) | renderer-free state machine: layouts, targets, commands, key tables, `.tmux.conf` |
| [zz-daemon](/crates/zz-daemon.md) | persistent daemon: mux state, PTYs, agent lens runtimes, frame and agent-stream fanout, the listening socket, attachment |
| [zz-daemon-client](/crates/zz-daemon-client.md) | the client half every client links: local and ssh endpoints, askpass, `CommandClient`/`InteractiveClient`, the shared transport |
| [zz-terminal](/crates/zz-terminal.md) | per-PTY child + libghostty on a worker thread; publishes terminal frames |
| [ala-cef](/crates/ala-cef.md) | CEF init, subprocess dispatch, request context, input translation, frame mailboxes |
| [zz-client](/crates/zz-client.md) | sans-IO protocol reduction and client-local chrome key tables shared by client shells |
| [zz](/crates/zz.md) | long-lived ala mux client; reconciles layouts; hosts terminal and CEF runtimes and the agent lens viewport |
| `ala`, `ala-macros`, `ala-platform` | ala, our GPUI: the core with Zed's utility code folded in, its macros, and every platform backend (iOS's UIKit and Android's NDK ones included) as a module per target |
| `penna` | widget kit on ala: theme, primitives, widgets, settings and phone chrome, icons; no zz dependencies (see [penna](/references/penna.md)) |
| `zz-ui` | zz's application UI on penna, shared by the desktop and thin clients |
| `zz-app` (`clients/app`) | the thin-client app the web, iOS and Android clients share |
| `zz-ios` (`clients/ios`) | iOS entry point around zz-app and ala-platform's UIKit backend |
| `zz-android` (`clients/android`) | Android entry point around zz-app and ala-platform's NDK backend, with the Java glue and APK files |
| [zz-chrome-import](/crates/zz-chrome-import.md) | Chrome profile discovery, cookie decryption, read-only history extraction |
| [zz-xtask](/crates/zz-xtask.md) | builds and validates platform CEF bundles |

Workspace members without a dedicated page here: the ala crates (see [ala](/references/ala.md)), `penna` (see [penna](/references/penna.md)), `zz-ui`, `zz-app`, `zz-ios`, `zz-android`, and `zz-tui` (`zz attach`).
The UIKit backend is `libs/ala/crates/ala-platform/src/ios`; the iOS app and its launch resources live in `clients/ios`.
The Android backend is `libs/ala/crates/ala-platform/src/android`; the Android app, its Java glue and manifest live in `clients/android`.
See [crates](/crates/index.md).

# Platform status

Linux/Wayland remains the most extensively runtime-validated host. On macOS, the release CEF bundle
and daemon PTY detach/reattach lifecycle are runtime-validated; full interactive GUI PTY/browser
validation remains outstanding. Windows maps the protocol to a local named pipe and has CI bundle
coverage, but full host validation remains outstanding. The experimental ala iOS terminal runs
on iPhone and iPad simulators; physical-device validation remains outstanding. The experimental
ala Android client runs on an API 36 emulator over a local rig and over ssh; physical-device
validation remains outstanding.

# External pins

- CEF pinned to Rust packages `154.0.0+154.0.23`, Chromium `154.0.8037.17` . see
  [CEF artifacts](/references/cef-artifacts.md).
- ala is our own copy, the ala crates . see [ala](/references/ala.md).
- tmux behavior checked against a pinned commit . see [tmux upstream](/references/tmux-upstream.md).
- X11 named colors sourced from Ghostty . see [Ghostty color reference](/references/ghostty-color-reference.md).

# Related

- [Process model](/architecture/process-model.md) . daemon, GUI client, CEF subprocesses, PTY workers.
- [Data flow](/architecture/data-flow.md) . how frames and input move end to end.
- [Agent lens](/concepts/agent-lens.md) . the agent timeline over a terminal running Claude Code
  or Codex: lens runtime, agent stream, input, and approvals.
- [Design plans](/designs/index.md) . feature plans and decision records.

---
type: Design Plan
title: "Browser egress through an SSH host"
description: "On macOS and Linux, a desktop browser pane can resolve names and open connections from its attached SSH host."
status: "Implemented, verified against source 2026-11-24"
resource: crates/zz/src/browser/view.rs
tags:
- remote
- browser
- quic
- proxy
- fleet
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

A browser pane opened on an attached SSH host can reach a development server bound to that host's loopback interface. Chromium runs on the desktop. Its request context uses `socks5://127.0.0.1:<port>`, backed by the SSH dynamic forward. Chromium passes hostnames through SOCKS5 for resolution on the remote machine. The CEF proxy bypass list contains `<-loopback>` so `localhost` and `127.0.0.1` also use the forward.

- `ssh_forward_command` in `crates/zz-daemon-client/src/endpoint.rs` adds a loopback-only `-D` listener to the connection that carries the daemon socket. A bind failure can retry on a fresh port. Closing the forward cancels the dynamic and local forwards.
- `browser_egress_spec` in `crates/zz/src/browser/view.rs` checks the desktop's `browser-egress` setting, enabled by default. The route requires an attached SSH forward with a SOCKS port. The Windows desktop skips this route.
- `BrowserProfilePaths::egress_profile_name` in `libs/ala-cef/src/profile.rs` keys local profile storage by browser profile and egress host. Cookies stay on the client, separate from local pages and other hosts. Changing the forward's port leaves that storage identity intact.
- `BrowserController::refresh_egress` updates the port for previously routed tabs with the same composite profile after reconnect. The actual proxy preference lives in `libs/ala-cef/src/cef_runtime.rs`.

The July 2026 QUIC design used a custom HTTP CONNECT proxy. It was retired with that transport. See [fleet attach](/designs/fleet-attach.md) and [browser profiles](/browser/profile.md).

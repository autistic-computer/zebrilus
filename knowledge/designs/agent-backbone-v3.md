---
type: Design Plan
title: "Headless agent commands and browser control"
description: "The CLI controls sessions and agent panes. A rendering client owns browser operations and exposes CDP when enabled."
status: "September 2026 design implemented except for the TUI screenshot path"
resource: crates/zz-tui/src/app.rs
tags:
- agent
- browser
- cdp
- tmux-compat
- daemon
- tui
- roadmap
timestamp: 2026-11-24T00:00:00-03:00
---

The September 2026 audit chose two interfaces for shell-driven automation. Agents use the normal zz CLI for mux and chat operations. A running browser client exposes Chromium's debugging endpoint when the user enables it. The daemon owns mux state and can dispatch operations while its rendering client is hidden. It cannot supply a Chromium screenshot on its own.

- [Browser access through CDP](/browser/agent-cdp.md) covers configuration and target selection. The native browser snapshot command family proposed in the audit was dropped after a measured saving of about 20 ms on a 50 ms step in the September setup.
- `CoreEvent::BrowserCommand` in `crates/zz-tui/src/app.rs` forwards ordinary browser commands to its provider. A screenshot still returns “browser screenshots require the zz app”.
- [Agent panes](/concepts/agent-panes.md) defines pane creation, targeted sends, replies and capture. The [mentions design](/designs/agent-mentions.md) covers child and peer chats. The audit predates both the removal of the old pane projection and the return of native driver-backed chats.
- `zz tools` describes the CLI available to an agent. [Shell integration](/terminal/program-status.md) supplies the program state available for ordinary terminals.

The original lane briefs, measured steps and reproduction scripts are available with `git show 918e62838:knowledge/designs/agent-backbone-v3.md`.

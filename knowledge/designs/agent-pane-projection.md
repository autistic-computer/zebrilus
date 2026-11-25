---
type: Design Plan
title: "Shadow terminal for agent panes (superseded)"
description: "SUPERSEDED: Agent capture reads chat events directly."
status: "Superseded 2026-10-23"
resource: crates/zz-agent/src/capture.rs
tags:
- agent
- terminal
- tmux-compat
- daemon
timestamp: 2026-11-24T00:00:00-03:00
---

The former agent pane fed a text projection and shell integration markers into a second terminal session to serve tmux read commands. Current agent panes render capture text from chat events in `agent::capture::chat_text`. See [agent panes](/concepts/agent-panes.md) for the current commands.

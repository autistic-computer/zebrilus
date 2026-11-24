---
type: Design Plan
title: "Terminal transcript lens (superseded)"
description: "SUPERSEDED: Agent panes replaced the transcript lens on 2026-11-01."
status: "Superseded 2026-11-01"
resource: crates/zz-daemon/src/daemon.rs
tags:
- agent
- agent-pane
- claude-code
- codex
- transcript
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

The October 2026 lens followed a running terminal CLI by tailing its transcript. The replacement opens a driver-backed chat and can flip a supported vendor session between chat and terminal in the same pane. See [agent panes](/concepts/agent-panes.md) for the provider and session rules.

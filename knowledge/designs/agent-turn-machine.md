---
type: Design Plan
title: "Agent turn state"
description: "The daemon reconciles prompts, vendor command IDs, late completions and cancellation in the Claude and Codex drivers."
status: "Implemented, verified against source 2026-11-24"
resource: crates/zz-agent/src/turn.rs
tags:
- agent
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

`TurnMachine` in `crates/zz-agent/src/turn.rs` accepts vendor facts and returns a new phase, effects and rejected inputs. The phases are offline, idle, running, cancelling and exited. A running turn can own multiple command IDs after steering. Remembering settled commands prevents a late completion from settling the next prompt. A cancelled turn gets a deadline so a missing final vendor record cannot hold the queue forever.

The Claude and Codex drivers translate their own frames, apply `TurnMachine::step` and service the deadline returned by `wake_at`. The [chat machine](/designs/agent-chat-machine.md) also uses the transition when projecting facts into chat events. The provider-specific recording tests are in `crates/zz-agent/src/claude/replay_tests.rs` and `crates/zz-agent/src/codex/replay_tests.rs`.

This machine covers the turn. The host handles provider startup and authentication, and the [agent pane contract](/concepts/agent-panes.md) covers the commands clients can send.

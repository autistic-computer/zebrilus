---
type: Design Plan
title: "Daemon-owned agent runtime"
description: "The daemon owns agent processes, command queues and transcript replay across client detach."
status: "Implemented, verified against source 2026-11-24"
tags:
- agent
- acp
- daemon
- protocol
- persistence
resource: crates/zz-agent/src/fanout.rs
timestamp: 2026-11-24T00:00:00-03:00
---

A driver runs on the machine that owns the pane. Detaching a desktop or thin client leaves the driver and its turn alive. Reattaching asks for updates after the last sequence the client applied. The fanout lane retains a bounded replay ring and can rebuild state for a client that fell behind.

| Owner | Source |
| --- | --- |
| Pane creation and lifetime | `Shared::open_agent_pane` in `crates/zz-daemon/src/daemon.rs` |
| Commands, pending asks and queued prompts | `AgentHost` in `crates/zz-agent/src/host.rs` |
| Subscription, ordered updates and replay | `AgentRuntime::replay` in `crates/zz-agent/src/fanout.rs` |
| Durable records and compaction | `AgentJournal` in `crates/zz-agent/src/journal.rs` |

The host moved into the daemon in August 2026. The short-lived [terminal lens](/designs/agent-lens.md) also used this infrastructure. Current [agent panes](/concepts/agent-panes.md) use provider drivers and the [chat machine](/designs/agent-chat-machine.md).

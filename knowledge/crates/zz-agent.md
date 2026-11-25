---
type: Rust Crate
title: zz-agent crate
description: The agent runtime the daemon drives agent panes with, its Claude Code, Codex and ACP drivers, host, fanout, journal and logins.
resource: crates/zz-agent/src/lib.rs
tags: [crate, agent, daemon, fanout, journal]
timestamp: 2026-11-25T00:00:00-03:00
---

# What it owns

zz-agent runs the agents behind agent panes. It knows nothing of the daemon's `Shared`,
`ServerState` or event loop. [zz-daemon](/crates/zz-daemon.md) builds it under its default
`agent` feature, names it `crate::agent`, and keeps the adapter in `daemon/agent_*.rs`,
`daemon/accounts.rs` and `daemon/providers.rs`.

| Concern | Where |
| --- | --- |
| Per-pane command queue, pending asks and queued prompts | `AgentHost` in `host.rs` |
| Subscription, ordered updates and replay | `AgentRuntime` in `fanout.rs` |
| ACP connection and permission handling | `runtime.rs` |
| Durable records and compaction | `AgentJournal` in `journal.rs` |
| Native drivers | `claude/`, `codex/` (process plumbing in `child.rs`) |
| Other CLIs | `agy.rs`, `grok.rs`, `cursor.rs` |
| Stored logins | `logins.rs`, `login.rs`, `accounts.rs` |

Paths are relative to `crates/zz-agent/src`. See [agent panes](/concepts/agent-panes.md), the
[daemon runtime design](/designs/agent-daemon-runtime.md) and the
[agent driver contracts](/references/agent-cli-contracts.md).

# Tests

`cargo test -p zz-agent` runs the driver, replay and journal tests. `sandboxed()` in `lib.rs`
is true under the crate's own tests and under its `test-support` feature, which zz-daemon's
dev-dependency turns on. While it is true, zz-agent keeps its logins in a temporary root,
writes no Codex rule, starts no shared Codex app server, skips prewarm and shortens the cancel
grace. `test-support` also exposes the fixture runner and the runner-factory hooks the daemon
tests use.

Log calls without an explicit target log under `zz_agent`; the headless CLI's filters list it
next to `zz_daemon`.

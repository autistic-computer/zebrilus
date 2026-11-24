---
type: Design Plan
title: "tmux compatibility and zz-specific commands"
description: "Compatibility decisions, scope and evidence for the alias campaign and the subsequent 3.8 catch-up."
status: "Frozen campaign scope completed in cycle 18, 2026-09-08"
resource: compat/tmux-gaps.json
tags:
- tmux
- compatibility
- roadmap
- cli
- fleet
- native-superset
timestamp: 2026-11-24T00:00:00-03:00
---

The supported workload includes shell invocations, imported bindings, config files, the pinned plugin corpus and terminal attach. zz keeps tmux syntax for those paths. A script selects a server with `--host` and keeps tmux session, window and pane targets within that server. Explicit zz verbs and flags cover native pickers, browsers, agents and fleet operations.

The mux has one owner per window. Linked windows, grouped sessions, interoperation with tmux's private socket and admitting a second OS identity through `server-access` are outside the contract. The detailed exceptions and their classifications live in [compatibility gaps](/tmux/gaps.md).

# Milestone 5: caller streams

The original roadmap deferred binary command input until its cost could be bounded. TUI-018 implemented a [command stream channel](/designs/command-stream-channel.md) for standard input and output with limits and backpressure.

# Campaign evidence

- The [drop-in phases](/designs/tmux-drop-in.md#phases) closed on 2026-08-20.
- The September alias campaign closed its frozen scope at 304 of 304 items in cycle 18. Its three post-freeze items were tracked separately. The stamped run covered 254 scenarios and 3,086 steps. `compat/orchestration/HANDOFF.md` records that run and its known rows.
- The October catch-up moved the reference to tmux 3.8 and added floating panes. `compat/catchup/ledger.json` records the work, including the deferred session-sharing and desktop menu mouse-input items.

For current status, read `compat/tmux-gaps.json` and the [compatibility harness playbook](/playbooks/compat-harness.md). The historical counts above refer to their recorded runs.

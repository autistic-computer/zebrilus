---
type: Design Plan
title: "tmux drop-in campaign"
description: "The original compatibility campaign covered configs, shell use, control mode and terminal attach."
status: "Original phases completed 2026-08-20"
resource: compat/tmux-gaps.json
tags:
- tmux
- compatibility
- drop-in
- layout
- control-mode
- roadmap
timestamp: 2026-11-24T00:00:00-03:00
---

zz accepts tmux command names and argv for config files, scripts and terminal attach. New picker, agent, browser and fleet operations have zz-specific verbs or flags. The command parser and the differential harness determine the supported behavior. [Compatibility gaps](/tmux/gaps.md) tracks the remaining differences.

# Phases

| Phase | Delivered in August 2026 |
| --- | --- |
| 0–2 | Initial command coverage, separate zz extensions and an isolated differential harness |
| 3–4 | Cell-based mux layouts and command coverage against the pinned reference |
| 5–7 | Shell execution, control mode and command-line entry points |
| 8 | Terminal attach for a bare invocation, new session or explicit attach |

Each window belongs to one session. Linked windows and grouped sessions remain outside the contract. zz uses its own daemon protocol. The GUI can keep native defaults while explicit tmux commands and the raw-terminal client follow the documented tmux behavior. The original phase log is in `TMUX_COMPAT_TRACKER.md`. For subsequent decisions see the [superset roadmap](/designs/tmux-superset-roadmap.md) and [compatibility harness](/playbooks/compat-harness.md).

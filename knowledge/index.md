---
okf_version: "0.1"
---

# zz knowledge

The daemon owns sessions, windows, panes and PTYs. Desktop, terminal, web and mobile clients attach to it.
Start with the [system overview](architecture/overview.md), [process model](architecture/process-model.md)
and [data flow](architecture/data-flow.md).

| Looking for | Start here |
| --- | --- |
| Code for a subsystem | [Crates](crates/index.md), [terminal](terminal/index.md), [browser](browser/index.md) |
| Wire compatibility, pane snapshots and terminal frames | [Protocol](protocol/index.md) |
| Command behavior and measured differences | [tmux](tmux/index.md) |
| Agent chats, pane layout and persistence | [Concepts](concepts/index.md) |
| Configuration, key bindings and UI conventions | [Configuration](configuration/index.md) |
| Build, run, profile or release zz | [Playbooks](playbooks/index.md) |
| Design decisions and open work | [Designs](designs/index.md) |
| Measurements and their limits | [Research](research/index.md) |
| Libraries, vendor contracts and upstream pins | [References](references/index.md) |
| What changed | [Update log](log.md) |

Each concept's `resource` points to its source when available. The current wire epoch and revision
live in `crates/zz-protocol/src/message.rs`.

<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Subdirectories

* [architecture](architecture/index.md)
* [browser](browser/index.md)
* [concepts](concepts/index.md)
* [configuration](configuration/index.md)
* [crates](crates/index.md)
* [designs](designs/index.md)
* [playbooks](playbooks/index.md)
* [protocol](protocol/index.md)
* [references](references/index.md)
* [research](research/index.md)
* [terminal](terminal/index.md)
* [tmux](tmux/index.md)
<!-- okf:listing:end -->

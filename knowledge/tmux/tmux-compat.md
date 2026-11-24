---
type: Concept
title: tmux compatibility philosophy
description: What a tmux command promises in zz, which client draws it, and where to check a measured exception.
resource: third_party/tmux-reference/UPSTREAM.md
tags: [tmux, compatibility, cli]
timestamp: 2026-11-24T00:00:00-03:00
---

# Command contract

A copied tmux binding, config or shell script should keep the meaning of its commands, targets, flags and output. An unsupported operation must report an error. zz extensions have their own verbs or explicit options. For example, `split-window -h` opens a terminal and `split-window -h --kind browser URL` opens a browser. A zz default binding may call either. See [command lookup](/tmux/commands.md) and [key tables](/tmux/key-tables.md).

The alias `tmux=zz` selects the zz executable. zz owns its daemon protocol. It cannot attach to sessions on a tmux socket. Each zz window belongs to one session, so `link-window`, `unlink-window` and session groups remain outside the contract. Floating panes are implemented. A zz popup is a modal floating pane in its window. See [floating panes](/designs/floating-panes.md) and [other exceptions](/tmux/divergences.md).

# Pinned reference

The reference is the tmux 3.8 tag, `7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53`. The harness builds that checkout. `compat/tmux-oracle.json` inventories names, aliases, arguments, option defaults and scopes, formats, hooks and bindings from the pin. The runtime ships separately from this reference. See the [upstream source map](/references/tmux-upstream.md) and [updating the pin](/playbooks/updating-tmux-reference.md).

| Question | Where to look |
| --- | --- |
| Is this difference intentional, open or closed? | Each group has a stable ID in the [gap registry](../../compat/tmux-gaps.json). The [gap report](/tmux/gaps.md) is generated. |
| What happens with two running servers? | `compat/scenarios/`, `compat/results/summary.md` and the [differential harness](/playbooks/compat-harness.md). |
| What did attached clients actually draw? | The [TUI ledger](../../compat/tui/campaign.json) holds the pin and proof revision for each obligation. The [TUI report](/tmux/tui-parity.md) records the scope verified there. |
| What changed when the pin moved to 3.8? | The [catch-up ledger](../../compat/catchup/ledger.json) and `compat/catchup/README.md` record the port, fixture reruns and deferred items. |

The [original drop-in campaign](/designs/tmux-drop-in.md) and [superset roadmap](/designs/tmux-superset-roadmap.md) describe the agreed scope. For the measurements behind the pin change, see the [initial CLI audit](/research/2026-08-22-tmux-cli-compatibility-audit.md) and [3.8 revisit](/research/2026-10-09-tmux-compat-revisit.md).

# Client and config behavior

The raw TUI draws the daemon's tmux status rows. Graphical clients use native chrome and retain the command's effects. A theme which only sets `status-format[]` can change the TUI without changing the desktop. See [status rows](/tmux/status-line.md).

The daemon reads zz's own mux config. Supply `-f` at startup to select files explicitly, or use `import-tmux-config [path]` to copy a donor into the managed block of `zz/mux.conf`. A normal restart need not rediscover a donor. See [config selection and replay](/tmux/conf-parser.md).

# Empty boot and attach

A command query can start a daemon with no sessions. Bare packaged `zz` enters through `new-session -A`, creating a session when needed. Explicit `zz attach` with no session returns `no sessions` and exit 1, so `zz attach || zz new-session` works. `compat/packaged-cli.sh` exercises bare launch, explicit attach, the read-only client and detach through the actual macOS bundle.

---
type: Reference
title: tmux divergence matrix
description: How to find a compatibility decision and its evidence, with examples of differences observable in zz.
resource: compat/tmux-gaps.json
tags: [tmux, compatibility, divergences, gaps]
timestamp: 2026-11-24T00:00:00-03:00
---

# Find a decision

Search `gaps[].items` in the [gap registry](../../compat/tmux-gaps.json) for a spelling such as `command:link-window`, `flag:new-session:-t` or `format:cursor_shape`. The enclosing group names its owner, decision, reason, acceptance checks and evidence. Closed entries retain their ID, date, resolution and proof. These queries work from the repository root.

```sh
jq --arg item 'command:link-window' '.gaps[] | select(.items | index($item))' compat/tmux-gaps.json
jq --arg id 'control-mode.notifications' '.closed[] | select(.id == $id)' compat/tmux-gaps.json
jq '.gaps[] | select(.status != "accepted") | {id, status, items}' compat/tmux-gaps.json
```

Use the [generated gap report](/tmux/gaps.md) for the inventory and the [harness](/playbooks/compat-harness.md) to reproduce a difference. `known_differentials` gives the exact result tuple for each accepted scenario. A changed tuple needs investigation even when its total gets smaller.

# Product decisions

| What the user sees | Registry group |
| --- | --- |
| A window has one owning session. Linking, unlinking, group targets and notifications that require a second membership cannot be reproduced. | `sessions.linked-groups` |
| A read-only control client can query state but receives `client is read-only` for a mutating command. | `control-mode.read-only-commands` |
| A popup is a window-owned modal floating pane. A menu belongs to the client that opened it. Two clients looking at the same window can therefore see different menus. | `display-popup.modal-pane`, `menus.client-owned` |
| A status theme affects the raw TUI. The desktop uses native status items. A native scrollbar sits outside the pane's cell grid. | `presentation.native-status`, `options.native-pane-scrollbars` |
| zz rejects invalid or oversized layouts and preserves a mixed split tree when spreading panes. | `layout.safety-invariants`, `layout.spread-mixed-upstream-bug` |
| Resizing a pane can rewrap its history differently in the terminal engine. | `terminal.zoom-reflow`, `terminal.resize-pane-trim` |
| `capture-pane -I` cannot report a time for each history line. The engine does not store those timestamps. Raw-grid and pending-input capture forms also have recorded limits. | `capture.line-times`, `capture.rich-transports` |
| An open desktop menu can let mouse motion or wheel events through. | `desktop.overlay-consumers` |
| Some option, command-print, format and control paths still lose non-UTF-8 bytes. | `options.byte-path-residue` |

# Format variables that remain unbacked

A recognized name can expand to a placeholder. For example, knowing the spelling of `cursor_shape` does not tell zz the live terminal cursor shape. Find individual names with the item query above. `formats.terminal-runtime` holds terminal-mode limits and `formats.history-counters` covers history accounting. The [format implementation](/tmux/status-line.md) distinguishes mux state, daemon-supplied values and scoped producers.

# Protocol and process level

`protocol.socket-interop` and `protocol.socket-acl` cover the private tmux wire and shared-server access model. zz implements the text control protocol through its own daemon. Panes receive tmux-style identity variables for tools that inspect them. The terminal launcher also sets `CLAUDE_CODE_TMUX_TRUECOLOR=1` to preserve true colour in that client when it sees `TMUX`. The setting lives in `crates/zz-terminal/src/session/pane_actor.rs`.

## Control notification remeasurement (2026-09-05)

At the September pin (`d77c9dc6`), the two servers both emitted late foreground shell output after the command guard and a flags-0 block for an after-hook. The test had to retain notification lines and wait for each guard to distinguish ordering from missing events. The next day's closed record `control-mode.notifications` documents the repairs to ordering, environment expansion and unquoted percent words. Notification cases that require linked windows belong to `sessions.linked-groups`.

For a change at the 3.8 pin, start with `compat/scenarios/smoke/fixtures/control-notify.py` and the `pin.control-3-8` entry in `compat/catchup/ledger.json`. A transcript exercises the control stream. A real terminal application's integration requires its own run.

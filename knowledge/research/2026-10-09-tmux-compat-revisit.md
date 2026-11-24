---
type: Research Report
title: tmux compatibility revisit (refusals and the 3.8 drift)
description: The measured delta to tmux 3.8, the floating-pane decision and the two deferred items in the catch-up ledger.
tags: [tmux, compatibility, floating-panes, tmux-3.8, gaps, research]
resource: compat/tmux-gaps.json
timestamp: 2026-10-09T00:00:00-03:00
---

# October decisions and their outcome

At zz `70c0e22bf` (0.16.0, protocol 107), the audit revisited refusals after the raw TUI, caller streams
and daemon popups became available. The rulings were to adopt the tmux 3.8 pin, build floating panes,
and defer session sharing and desktop menu input capture. The
[August CLI baseline](/research/2026-08-22-tmux-cli-compatibility-audit.md) covers the preceding pin.

At `1ce632dce`, `compat/catchup/ledger.json` records 32 merged items and two deferred.
`third_party/tmux-reference/UPSTREAM.md` pins 3.8 at `7f2a35ad`. `crates/zz-mux/src/model.rs` carries
floating panes on the window. The protocol catalog lists two unsupported commands, `link-window` and
`unlink-window`, and their aliases. Consult `compat/tmux-gaps.json` for the current flag and behavior
decisions. The
[original audit](https://github.com/autistic-computer/zebrilus/blob/918e62838/knowledge/research/2026-10-09-tmux-compat-revisit.md)
records the old refusal rationales, source references and candidate scopes.

The recheck identified stale refusals for terminal capability negotiation, binary caller streams,
hyperlink capture, chooser previews and styles, fallback process names, and the raw terminal's OSC 52
clipboard query. It distinguished layout-owned floating panes from per-client popups. The intended
checks covered position/size clamping, zoom, top-first pointer targeting and multiple clients. Linked
windows would require a multi-owner window model. The audit counted about 243 `window_for_pane` call
sites, without implementing or benchmarking that redesign.

## Measured pin delta

The October inventory compared builds of `d77c9dc6` and the 3.8 tag. It first reproduced the checked-in
old oracle. The release branch diverged from master at `5aeacf1c`. There were 446 commits in the
old-pin-to-release range and 577 in the old-pin-to-`82abcd17` master range. No command, alias or arity
changed between the two pins.

| Command | Change |
|---|---|
| capture-pane | `+-I` (time each line entered history) |
| display-message | `+-j` (parse as JSON and print) |
| display-panes | `-b` removed; `+-k -Z`, `+-s source-window`; `-t` is now a target pane, not a client; it is a pane mode now (`window-panes.c`) |
| new-pane | `+-A -C -D -K -M -O` (modal panes and mouse-drag creation) |
| split-window | `+-B border-lines` |
| set-hook | `+-E` (fire user event `@name`), `+-T` (monitor fires only while true) |
| show-options, show-window-options, show-hooks | `+-F format` |
| wait-for | `+-E -l -v`, `+-F format`, `+-w waiter`; positional `channel` becomes `name` |

- Three new options were `clear-on-attach`, `copy-mode-current-line-style` and
  `display-panes-border-style`. Four display-panes options moved to window scope and `remain-on-exit`
  gained `failed-key`. Changes to defaults and scopes required source/attached-fixture checks beyond the
  option-name inventory.
- The format table added 16 names for history, command/output timestamps and generation, pane modes,
  unzoomed dimensions and modal panes. New context families included refresh, clipboard, environment,
  option and event payloads. The modifier count rose 36 → 37 (`A` for animations). Literal format-scope
  counts rose 31/153/108 → 37/204/122. Argument-parser callbacks fell nine → eight.
- Hook names moved 68 → 89 with 22 additions and removal of `after-queue`. Default keys moved 303 →
  308. Copy mode gained line-number control and refresh. Attached fixtures exposed changed ACS chooser
  arrows, terminal capabilities (233 → 236), sync negotiation, window-scoped menus, and the resize/key
  behavior of display-panes mode.
- Script-visible changes included JSON v2 layouts with v1 input/control compatibility, removal of the
  `active-pane` client flag, pane-owned display-panes mode, window-owned menus, quote escaping in `%%`,
  rejection of invalid relative targets and control notifications outside command response blocks.
  Post-3.8 master behavior was outside the selected pin.

## Deferred

The ledger names two items. `later.share` requires server-enforced connection/session scoping and
read-only permissions for sharing with another person. The existing read-only attach flag was inadequate
for that purpose. `later.menu-input` asks whether a desktop menu consumes pointer motion and wheel
events or allows scrolling underneath. The October investigation did not establish either as delivered.

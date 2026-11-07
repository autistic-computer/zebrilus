---
type: Design Plan
title: Native status bar for GUI clients
description: The desktop status bar combines the sidebar toggle and left-aligned window pills from structured state while tmux status rows remain a TUI compatibility surface.
status: Complete
resource: crates/zz/src/status_bar.rs
tags:
- status-bar
- desktop
- ala
- client
- snapshot
- configuration
timestamp: 2026-10-24T00:00:00-03:00
---

# Decision

The desktop GUI builds its status bar from structured mux state. It does not
consume `StatusLine`, parse tmux styles, recognize powerline separators, or map private glyphs to
icons. The raw-terminal attach client keeps the daemon-expanded tmux rows and renders them with the
shared cell composer.

Desktop and web share the status widgets. The titlebar's only chrome control is
the sidebar toggle, with no divider after it; settings sits at the bottom of the sidebar
(`navigation::workspace_sidebar_footer`), and sessions are switched from the sidebar.

# Data and presentation

`zz_client::StatusBarModel::from_snapshot` is the pure boundary between mux facts and GUI layout. It
selects the attached session, returns its full window list, marks the focused window, and derives
bell and activity from each window. Each window also carries its panes in layout order, with pane
identity, kind, label, and focus state for its icon deck.
Automatically named windows whose active pane is a browser show the page; every other window
shows its name. The model retains the raw window name for rename prompts. Until the Agent pane was
removed on 2026-10-23 ([agent lens](/designs/agent-lens.md)), automatically named windows showed
the active agent's session name. The model
still names the attached session (the iPhone overview shows it).

The model owns no truncation, overflow menu, click action, color, update state, or time source. The
desktop view owns those concerns:

| Item | Desktop behavior |
| --- | --- |
| Windows | Always left-aligned; the name (no index, since 2026-10-15) for at most five visible windows, with a close button on hover inset like the sidebar tree's; the visible range stays centered around the active window, and the overflow menu reaches the rest |
| Pane deck | A packed fan of pane tiles, the focused pane in front with its Terminal, Browser, Agent provider, Editor, or Picker glyph; pointing at it opens every pane in a row with details on hover; clicking a tile selects that pane |

The sidebar and status bar are mutually exclusive desktop views: an expanded sidebar hides the
status bar; retracting it shows the status bar in the title bar. There is no bottom placement.

Desktop shortcuts Cmd+1–9 on macOS and Ctrl+1–9 on Linux select windows by their
position in the sidebar's list, across sessions and connected hosts, whether the
sidebar or titlebar is visible, so with the titlebar showing a number can open a
window of another session. They ignore missing positions. The `ui` chrome actions `select-window-1` through `select-window-9`
support rebinding and take priority over pane shortcuts.

Window pills sit in one strip (penna's `TabStrip`, the `tab.strip` role: a wash with no edge, 2px inside and 2px between pills, its corner the pills' corner plus 2px so the open pill's glass sits concentric in it; with glass off the open pill takes a hairline edge in place of the rim). The pills are 28px high, so the strip is 32px, vertically centered inside the titlebar, a pane header tall (34px) plus one pane margin (`Theme::title_bar_over_panes_height()`), 40px at the default margin, leaving 4px above it and 4px down to the panes, since the panes' own top margin finishes the gap. The pills
start at 240px wide regardless of the title and shrink together only when the window strip runs
out of room; titles truncate within the remaining space. Their pane deck is the shared
`navigation::pane_stack` at its 20px size: the focused pane's tile with its glyph in front and at most two blank tiles fanned out behind it, each tile outlined on its own (dark in both modes and lit like a program icon, with no theme hairline: a gray line outside and a white rim inside in light mode, a dark line and a faint rim in dark mode) and in the running program's colors as in the flat sidebar, 5px apart, so it is never
wider than 30px and carries no count. The ring follows the pill's hover fill. Pointing at the deck
opens every pane, focused first, in a row of tiles drawn on the pill, the window name hidden
under them and fading back in past them; tooltips there show the pane
name, type, identity, focus state, and available URL, browser profile and tab count, or working
path. Clicking a tile selects both
its window and pane directly, without a popup menu; a disconnected deck still opens but does not
select.

Browser cards use the active tab URL to retrieve a cached favicon and page title within the
browser profile. The globe and current URL are fallbacks, with “New tab” for a blank page. Browser
tabs do not increase the pane count. The web client shares the deck, tooltips, and direct selection with globe fallback.

Chrome comes from the active zz-ui theme. There is no time/date item or alignment preference.
Until 2026-10-24 the bar also had an agent activity item: a status dot with a summary such as
"1 needs input" and a menu of the live terminals running Claude Code or Codex that selected each
one's pane. It and its `status-agents` setting were removed, and the sidebar still shows each
agent's state. An old `status-agents` line is reported and otherwise
ignored.

# Settings

None since 2026-11-06. The Status bar page under Settings had three switches, `status-badges` (bell
and activity dots on each window pill), `status-host` (the remote host's name) and `status-update`
(an available release with an install action). The title bar dropped those items and the page went
with them: the sidebar marks bells and names the host, and Settings > About offers
updates. The three keys are retired (`zz_config::RETIRED_STATUS_KEYS`) and a leftover line is
reported and ignored. The model still carries each window's bell and activity, which the compact
view reads for its page attention.

# Protocol boundary

Wire v86 appends `activity: bool` to `WindowSnapshot`. The other bar facts already existed in the
snapshot or app state, so the desktop needs no native status payload. `StatusLine` remains on the
wire for the cell-faithful TUI and tmux compatibility; GUI clients have no presentation consumer for
it.

# Deferred

Custom text is optional and unbuilt. A future version may carry one daemon-expanded format string
as plain text with a bounded refresh cadence for `#(cmd)`. The GUI would use theme colors and honor
no tmux style. That work needs a protocol addition and does not expand the native item vocabulary.

Further FFI and mobile-client activity presentation remains separate. There is no faithful-tmux toggle in the GUI.

# Key files

- `crates/zz-client/src/status_bar.rs`: pure status-bar model.
- `crates/zz/src/status_bar.rs`: ala layout, overflow and actions.
- `crates/zz/src/app_shell.rs`: snapshot and agent-state redraw subscriptions.
- `crates/zz-tui/src/main.rs`: terminal client that retains the composed tmux status row.

# Related

- [tmux status line](/tmux/status-line.md)
- [Application configuration](/configuration/app-config.md)
- [Snapshot schema](/protocol/snapshots.md)

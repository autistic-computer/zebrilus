---
type: Concept
title: Unified palette and tmux command completions
description: Desktop session, window, and pane navigation alongside command and host search, the `@` target mode that prefix F also opens, shared zz-ui presentation with key glyphs per platform, and preserved tmux prompt behavior.
resource: crates/zz-ui/src/command.rs
tags: [gpui, command-prompt, tmux, completions, ui]
timestamp: 2026-10-26T00:00:00-03:00
---

# Overview

The desktop unified palette combines a session, window, and pane tree with command and host search.
`Cmd-K` and `Cmd-P` open a fresh search on macOS; `Ctrl-Shift-K` and `Ctrl-Shift-P` do so on
other desktop platforms. View > Choose Window opens the palette in `@` mode. Daemon
`choose-tree -s` and `choose-tree -w` requests, including `prefix s`, `prefix w`, remaps, and
direct commands, use the same tree presentation with their daemon-selected expansion and row.
The desktop lets the daemon resolve prefix bindings.
The desktop and browser clients share the surface and row components in `zz-ui::command`.
The browser retains its command-prompt adapter.

# Modes and navigation

| Prefix | Content | Activation |
| --- | --- | --- |
| none | Workspace tree, three recent commands, then hosts; typed search groups Navigation, Commands, and Hosts | Attach to a session, switch window, focus pane, execute, or scope according to the selected row |
| `:` | Command catalog with descriptions, shortcuts, and highlighted-command usage | Execute a command or choose its target |
| `@` | A target in zz's grammar: hosts, sessions, windows, panes and files ([below](#targets)) | Switch to a session or window, focus a pane, open a host, or put a file's path at the pane's cursor |
| `>` | The Driver chat, while there is a driver | See [durable driver chat](/designs/durable-driver-chat.md) |

The default Workspace tree opens the current session to its windows and leaves other sessions
collapsed. Windows expand to panes. Session, window, and pane rows all activate their targets;
the disclosure control only toggles the branch. Left/Right collapse or expand tree branches.
A host row in the Hosts section opens `@host:`, connecting the host first when it is offline.

Typed search includes descendants hidden by collapsed branches and shows their session/window
breadcrumbs. The default search keeps Navigation, Commands, and Hosts in separate sections.
A terminal whose agent lens is open carries its agent state in the tree and search results.

# Targets

The `@` mode, the Driver chat's `@` mentions and `prefix F` share one grammar and one picker,
`zz_ui::target::TargetPicker` (2026-10-26; before, the palette had `@` Navigate, `%` Pane and `~`
Host modes, the chat its own picker, and `prefix F` a list under the shell cursor). A target is
tmux's `-t` with an optional host in front, or a file on a host in scp form:

| Typed | Means |
| --- | --- |
| `work:2.%7` | session work, window 2, pane %7, on the host the window is attached to |
| `%7` | pane %7 there |
| `~/dev/zz/`, `/etc/` | that folder there |
| `xps:` | xps's sessions, then its home folder |
| `xps:work:2` | window 2 of work on xps |
| `xps:~/src/` | a folder on xps |

Without a host a target means the attached host, the way tmux's `-t` means the current server, and
a host's name wins over a session with the same name (`parse_target`). Before any `:` the list is
the sessions here, the other hosts, then `~/`, and typing searches the whole fleet: hosts, then
each connected host's sessions, windows (by name too) and panes (by program or title), labelled
as you would type them. After the last `/` typing fuzzy-matches everything under that folder with
the path picker's ranking, and an empty filter lists the folder's own entries, folders first, with
git's letter. A folder under one already walked whole is listed from that walk without asking the
host again.

Tab completes the highlighted row and opens what is under it (`xps` to `xps:`, a session to
`work:`, a window to `work:2.`, a folder to `src/`); `:`, `.` and `/` typed by hand do the same.
Enter goes to a session or window, focuses a pane, opens a host, and puts a file or folder at the
pane's cursor. Backspace at a level's edge removes the whole level (`xps:work:` to `xps:`,
`~/dev/zz/` to `~/dev/`, `pop_level`), and on an empty field leaves the mode. The footer's Enter
label says what it does to the highlighted row. Files are listed by the host that has them,
through the pane a path goes into when it is on that host and any terminal there otherwise
(`PaletteBackend::list_path`, `MuxClient::request_host_path_list`), so the insert style is that
pane's: quoted for its shell, `@path` for Claude Code. A path from another host goes in as
`host:/abs/path`.

`prefix F` (`choose-path`) opens the palette in `@` mode on the pane's folder, or `-c`'s, written
under `~` on this machine (`CommandPaletteView::new_target`), and the pick goes into that pane.
The thin clients have the grammar and the session, window and pane levels but list no files.

Keys in the footer and on rows are drawn as key icons, Apple's on macOS and iOS and ISO 9995's
elsewhere (`zz_gpui_kit::kbd::KeyGlyphs`, [design system](/designs/design-system.md)), and a
command's tmux shortcut (`C-b c`) as one small cap per key.

Typing a prefix into an empty default search replaces it with a mode Badge. Choosing a target
command adds its name as another Badge, and a host Badge before it while the command runs on the
target's host. Backspace on an empty field removes one layer: command, mode, then host. Escape and clicking outside close the
palette; reopening resets the mode, query, and scope. Up/Down wrap across selectable rows and
skip headers. Moving beyond the visible rows scrolls only far enough to reveal the selected row,
including in daemon command prompts. Pointer movement selects the hovered row, and clicking activates it.

Session, window, and pane targets use stable IDs for execution; rows display names and breadcrumbs.
IDs remain searchable metadata. On the attached host, a terminal row whose
[agent lens](/concepts/agent-lens.md) runtime is open reads `Agent · running`, `Agent · waiting for
input`, `Agent · idle` or `Agent · failed` from that pane's agent state (`tree_pane` in
`crates/zz/src/command/palette.rs`), without a duplicate status dot; other terminal rows read
Terminal.
Successful local navigation and command requests use the app's existing notification surface.

The fuzzy matcher ignores query whitespace and case. It scores an ordered subsequence by the
first matching character's position plus twice the skipped characters between matches, and
returns UTF-8 byte ranges for text highlights. Each section sorts matches by score.

# Settings

Settings > Advanced > Command palette exposes two preferences:

| Config key | Values | Default |
| --- | --- | --- |
| `palette-window-layout` | `grouped`, `flat` | `grouped` |
| `palette-show-keys` | `true`, `false` | `true` |

The `open-command-palette` chrome action lives in the UI key table and can be remapped.
Its default shortcuts replace the terminal Clear History bindings on those same keys.
Clear History remains available through the menu and custom bindings.

# Daemon prompts

`prefix :` and daemon-issued command prompts use the same desktop surface. Value prompts such as
`prefix $` rename-session and `prefix ,` rename-window retain their daemon-owned labels, initial values,
and private substitution templates. Raw-key (`-1`, `-N`, `-k`), incremental (`-i`), and backspace-exit
(`-e`) prompt modes retain their original handling. The client relays raw keys on the pane-targeted
key path and does not rewrite those prompts through unified search.

Native palette actions use existing mux navigation and execution APIs. Catalog actions submit
structured command invocations. Daemon prompts submit through `CommandPromptAction::Submit`.
Local freeform commands use a structured `if-shell -F 1` invocation with the entire command as
one argument, so the daemon retains command parsing, aliases, variables, and remote execution
context. The client does not interpret the command as an operating-system shell string.

# Daemon tree requests

`prefix s` opens collapsed sessions with the invoking session selected. `prefix w` opens
expanded sessions and collapsed windows with the invoking window selected. Both publish
`ChooseTreeKind::Windows`; the palette keeps the session, window, and pane rows supplied by the
daemon, including their depth, expansion flags, custom row text, and original source indices.
Backspace on an empty search closes the daemon chooser and returns the same palette instance
to the default Workspace view, keeping input focus. Backspace with text only edits that text.

Enter sends `ChooseTreeAction::ActivateIndex` with the daemon row index; Escape or clicking
outside sends `ChooseTreeAction::Close`. The daemon retains custom selection templates, target
validation, and zoom restoration. Returning to the default view uses the same close action.
The workspace ignores queued chooser updates until the client records the close; a later chooser
request can then open normally, including when close and open events arrive together.

The daemon publishes visible rows, so palette search expands collapsed branches through
`Select` followed by `Expand`, one branch at a time. The adapter waits for state that confirms
each expansion before choosing another branch and matches against the resulting daemon rows.
Clearing the query collapses only branches opened for search. The adapter resolves targets against
fresh source indices as rows appear or disappear; it uses the existing chooser messages without
a wire change. `CommandPaletteView::advance_chooser_search` in
`crates/zz-ui/src/command/palette_view.rs` owns this sequence.

# How completions are sourced

```
zz-protocol command catalog (static, renderer-neutral)
        │  canonical name + aliases + description + option/flag completion kinds
        ▼
zz-client completion engine (token-aware, over the current MuxSnapshot)
        │  recognizes: command token / option tokens / option values / quoted values / cursor token
        ▼
Ranked suggestions: exact → canonical-prefix → alias-prefix → fuzzy → description
        │  restricted to: unused flags, required enum values, live session/window/pane targets
        ▼
CommandPaletteView renders a scrollable list of 28px row slots
```

The catalog is plain Rust data with no GPUI dependency, so it is unit-testable inside `zz-protocol`
(every canonical name unique, every alias resolves to exactly one canonical command, static enum
values valid for their command) independent of rendering. Live target completions (sessions,
windows, panes) are resolved client-side from the same `MuxSnapshot` [`AppView`](/crates/zz.md)
already reconciles panes from. A target that disappears between suggestion and submission is
rejected by the daemon's existing target validation rather than trusted client-side.

# How it is rendered in GPUI

`zz-ui::command::CommandPaletteSurface` owns the shared panel, usage line, and shortcut footer.
`command/palette.rs` builds its Input, Badge pills, ListItem rows, section headers, status dots,
and matched-label spans. `PaletteRow::icon` and `PaletteRow::expanded` carry the tree icon and
disclosure state. `command_palette_tree_entry` gives the disclosure its own toggle handler while
the row keeps its activation handler. One `CommandPaletteView` in
`crates/zz-ui/src/command/palette_view.rs` serves desktop, web, and iOS; the mode, tree, and
selection model lives beside it in `palette_model.rs`. Each client supplies a `PaletteBackend`:
desktop builds a multi-host `PaletteTree` from its mux fleet, the thin clients a single-host tree
from their daemon connection, and both route input, command execution, and target activation
through the same trait.
`InputState` continues to handle selection, IME, clipboard, and undo. Mode changes update its
placeholder without replacing the field.

The desktop overlay starts near 12% of the viewport height, caps its width at 560px, and leaves
16px side margins. The panel is the `palette` role (a popover): in the Default theme an opaque
raised background, the outline and the float shadow, and the theme radius plus the 8px list inset. A 40px input
row contains 12px search text and pills. Rows match dropdown typography with 12px medium labels,
10px secondary text, 16px line heights, 26px highlights, and 2px gaps; the scrolling area reaches
at most 440px. Usage and footer text are 10px. The normal UI font follows the theme; usage, prefixes, command pills, and shortcut
badges use its monospace font.

Colors derive from `cx.theme()`. Selection uses the shared opaque accent treatment with normal
foreground text. Muted text brightens when selected, matches use semibold foreground, online
hosts use success, reconnecting hosts use warning, and session/window running-agent summary dots
and mode prefixes use accent. No prototype colors enter the application code.

The desktop opening animation uses the palette entity's identity, so chooser revisions and
returning to the default view cannot restart it. The overlay keeps focus in the
single Input and does not resize the workspace. The browser and specialized daemon prompts
retain their completion contract: Tab accepts a suggestion, while Enter either accepts an
engaged suggestion or submits the current input.

# Protocol shape

`CommandPromptAction` variants live under `zz_protocol::InputMessage`:

| Variant | Purpose |
|---------|---------|
| `Update { input, cursor }` | Persist local text + Unicode-scalar cursor after every accepted edit, without the daemon publishing an echo `CommandPrompt` event back (so `InputState` selection/undo is never reset by the client's own edits) |
| `Submit { input }` | Submit the complete current value through the existing daemon parse/template/execute path, guarding against a race between a final edit and execution |
| `Close` | Cancel and remove the daemon-side prompt |

`CommandPromptState { prompt, input, cursor, kind, history }` is published on open, explicit server
change, and resync. `kind` separates command and value prompts; `history` is populated only for
command prompts. The `Update`/`Submit`/`Close` actions and this state shape entered the wire at v32;
the live version is the `PROTOCOL_VERSION` constant in `crates/zz-protocol/src/message.rs`. See
[the wire protocol](/protocol/wire-protocol.md).

# Key files

| File | Role |
| --- | --- |
| `crates/zz-ui/src/command/palette_view.rs` | `CommandPaletteView` and `PaletteBackend`: input, suggestion selection, pointer dismissal, prompt and chooser synchronization for every client |
| `crates/zz-ui/src/command/palette_model.rs` | Host-agnostic navigation tree, search modes, result grouping, target selection, and fuzzy matching |
| `crates/zz/src/command/palette.rs` | Desktop `PaletteBackend`: multi-host tree, config-driven settings, and mux activation |
| `clients/app/src/command_palette.rs` | Web and iOS `PaletteBackend`: single-host tree, client preferences, and connection commands |
| `crates/zz-ui/src/command.rs` | Shared input, completion rows, badges, shortcut hints, and floating palette surface |
| `crates/zz-ui/src/command/palette.rs` | Shared tree disclosure, icon, row, pill, section, highlight, and status presentation |
| `crates/zz-client/src/completion.rs` | Tokenizes and ranks catalog, option, enum, and live-target completions against the current `MuxSnapshot` |
| `crates/zz-protocol/src/catalog.rs` | The renderer-free command catalog shared by execution parsing and UI completion; tests enforce unique canonical names, aliases, and options |
| `crates/zz-protocol/src/message.rs` | Prompt kind/history and the native edit/submit/close actions |
| `crates/zz-daemon/src/daemon.rs` | Prompt state, bounded history, template substitution, and final execution |

`AppView` gives the palette highest focus precedence and renders it as an overlay, so it never
resizes the pane workspace. `chooser/tree.rs` renders requests with `ChooseTreeKind::Panes` or
`ChooseTreeKind::Clients`; `ChooseTreeKind::Windows` requests render their session/window/pane
tree through the palette. `chooser/buffer.rs` and `pane/display.rs` remain
separate daemon-driven surfaces.

# Related

- [`zz` crate](/crates/zz.md) . `AppView` hosts the palette; `mux/client.rs` delivers
  `CommandPromptState` to its desktop adapter.
- [`zz-client` crate](/crates/zz-client.md) . shared completion engine.
- [`zz-protocol` crate](/crates/zz-protocol.md) . shared command catalog.
- [`zz-mux` crate](/crates/zz-mux.md) . command execution.
- [Tmux command catalog](/tmux/commands.md) . the catalog entries this palette's completions read.
- [Tmux compatibility](/tmux/tmux-compat.md) and [key tables](/tmux/key-tables.md) . the broader
  tmux-compatible surface `command-prompt` is one part of.
- [Wire protocol](/protocol/wire-protocol.md) . carries `CommandPromptState` and
  `CommandPromptAction`.
- [Split-pane layout](/concepts/split-pane-layout.md) . the workspace this overlay never resizes or
  dims.

---
type: Concept
title: Unified palette and tmux command completions
description: The palette's line grammar (a place typed with tmux's sigils, then an action), chips and keys, the & files level that prefix F opens, the empty palette and recents, the written @host:target form, `>` GUI actions with a live theme preview, `?` to ask the Driver Agent, shared zz-ui presentation with key glyphs per platform, and preserved tmux prompt behavior.
resource: crates/zz-ui/src/command.rs
tags: [gpui, command-prompt, tmux, completions, ui]
timestamp: 2026-10-27T00:00:00-03:00
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

# Lines: a place, then an action

Since 2026-10-27 a line in the palette is a place followed by an action (design: the palette spec,
decisions D1 to D12). A place is typed with tmux's sigils plus one for the host, and every part
matches fuzzily:

| Sigil | Level | Matches |
| --- | --- | --- |
| `^` | host | its name |
| `$` | session | the label the row shows (`zz 3`, `Working on foo`), or the tmux name exactly when you type only digits |
| `@` | window | `index name`, then the programs in it; digits only are the index inside the parent session |
| `%` | pane | `index title`, then its program; digits only are the index inside the parent window |

`#N` after a sigil pins an id (`$#3`, `@#41`, `%#52`). A skipped level is a wildcard
(`^xps@editor` finds windows named editor in every session on xps), while digits stay inside the
chip or the current session or window, the way tmux reads `:2` and `.1`. A place sigil inside the
field counts only when it is deeper than the part before it, so `%fabrico@xps` is one pane query,
and `"…"` keeps sigils literal. Rows rank by the fuzzy score below, then nearer first (this
window's panes, this session, this host, other hosts), then a boost for recent picks.

An action ends the line and takes everything after it raw: `:` a tmux or zz command, `>` a GUI
action, `&` files, `?` the driver. Plain text that starts the field is a search: places under the
chips, then commands, then hosts when no host chip is set. The parser, resolver and compiler are
sans-IO in `zz_client::place` (`parse`, `candidates`, `resolve`, `command_target`, `written`,
`read_written`).

# Chips and keys

Places you enter become chips in the input (`UnifiedPalette::chips`). Cmd-K opens with the host the
window is attached to as a chip, which filters (D3); View > Choose Window opens on `@`, `prefix F` on
`&` and the pane's folder. Rows for an empty field are the empty palette while only that host chip
is set, and the children of the last chip once you entered a level.

| Key | Does |
| --- | --- |
| Enter | Acts on the highlighted row: goes to a place (attaching or reconnecting its host), runs a command, opens a recent. With no row, goes to the place the chips and the line name |
| Tab | Makes the highlighted place a chip and leaves the next level's sigil in the field. Commands complete their name |
| Shift-Tab | Drops the last chip and highlights it in its parent's list, so Tab goes back in |
| a sigil | Takes the best match for the part before it as a chip; an action sigil does the same and hands the rest to its level. At the field's start it replaces the chips at that level and below |
| Backspace on an empty field | Drops the last chip. Right after a typed sigil the chip turns back into what you typed (`$worki`). Dropping the host chip makes search cover every host |
| Up, Down | Move, wrapping, skipping headers |
| Alt-Enter | `CopyAddress`: copies the highlighted place as tmux writes it, `@host:%52` (D9) |
| Esc | Closes |

A command after a place runs on the place's host and never attaches you (D6), with `-t` added from
the catalog's target kind unless the line has one: a pane id, a window id, or the session id
(single-quoted in command text, since tmux expands `$7`); `new-window` always gets the session and
`new-session` none. `:kill-window` with no place runs on the current window, as in tmux.

`&`, `>` and `?` are levels: `PaletteBackend::action_level` supplies a `PaletteActionLevel` that gets
an `ActionContext` (the resolved place, which levels were written, whether the host is the opening
chip, the raw text after the sigil and the pane a path goes into), the first look at the keys, and
draws its rows and hints. A backend with no level for `&` falls back to the target picker below.

# Files

`&` lists the files under a folder on one host (`zz_ui::files`: `FilesAction` is the level,
`FilesLevel` the entity that lists and draws). It starts in a folder and writes it after the `&`
(D5): after a pane, window or session that pane's folder (a window's or session's active pane);
on the host you are on with nothing under it, the pane a path goes into; on another host its home.
A pane with no folder (a browser, an editor) starts at `~/`. `prefix F` opens straight into it, on
the `-c` folder when given, else its pane's. The folder is written under `~` where the backend
knows the home (`PaletteBackend::home_dir`: the desktop's own machine).

The text after `&` is a path with plain spaces: up to the last `/` is a folder, `/…` and `~…` as
written and anything else under the start folder, and the rest filters (`split_rest`). The client
folds `.` and `..` and only asks the host for absolute or `~` folders, through the pane a path
goes into when that is a terminal there and another terminal not running ssh or the like
otherwise (`listing_pane`). An empty filter lists the folder's own entries, folders first, and
typing ranks everything under it with git's letter. A folder under one walked whole is listed
from that walk without asking the host again. Backends hand it listings through
`PaletteBackend::feed_listings`.

| Key | Does |
| --- | --- |
| Tab, or Right with the caret at the end | Opens the highlighted folder |
| Shift-Tab, or Left with the caret at the end and no filter | Goes to the parent, highlighting the folder it left; past `~/` once the host said where the home is |
| Backspace right after a `/` | Goes to the parent; on an empty path it drops the `&` |
| Enter | Puts the path at the cursor of `prefix F`'s pane, else the focused pane (`insert_path`), and closes; copies it instead when that pane is a browser or editor |
| Alt-Enter | Copies the plain path (absolute, `host:/abs/path` from another host, never quoted) and keeps the palette open (D10) |

An insert or a copy is kept as a recent file (`RecentKey::File` with the absolute path). Enter on
one in the empty palette writes `&<path>`, so its folder lists with the name as the filter, and
`open_recent` inserts it as soon as its row arrives. Under a coarse pointer a tap opens a folder
and a folder row has its own insert button.

The iPad and web clients list files too: they announce `client-path-picker-v1`, so the daemon
lists for them and sends `prefix F`'s `OpenPathPicker`. `clients/app/src/command_palette.rs`
sends `PathListRequest`, feeds the replies from the connection's unreduced messages and pastes
with `path_insert::insert_text`. They do not know the host's home, so their start folder is
written in full.

# The empty palette and recents

Needs you (a window per pane whose status is blocked or failed, on any connected host), recent
places, recent commands, recent GUI actions and files (three each), then every host.
`zz_client::recents` keeps one decayed score per pick (three-day half-life, at most 200), recorded
through `record_recent`. The desktop keeps it per client in `palette-recents.json` in the app's
data folder (`app_identity::palette_recents_path`, `command::palette::init_recents`); the iPad and
web keep it in memory. Their only host is named after the endpoint on iOS and `local` on the web.

# Written places

Anything that outlives the palette writes a place the way tmux does with the host in front:
`@xps:%52`, `@xps:@41`, `@xps:$7`, `@xps`, and files as `@xps:~/path:42`. Chat mentions and links keep
that form (D9), and pasting either it or the chat picker's readable form (`@xps:work:2.%7`) into an
empty palette turns it into chips.

# Targets (chat mentions)

The Driver chat's `@` mentions and links still use `zz_ui::target::TargetPicker` and its grammar,
tmux's `-t` with an optional host in front, or a file on a host in scp form:

| Typed | Means |
| --- | --- |
| `work:2.%7` | session work, window 2, pane %7, on the host the window is attached to |
| `%7` | pane %7 there |
| `~/dev/zz/`, `/etc/` | that folder there |
| `xps:` | xps's sessions, then its home folder |
| `xps:work:2` | window 2 of work on xps |
| `xps:~/src/` | a folder on xps |

Without a host a target means the attached host, and a host's name wins over a session with the
same name (`parse_target`). Tab completes the highlighted row and opens what is under it; `:`, `.`
and `/` typed by hand do the same. Files are listed by the host that has them, through the pane a
path goes into when it is on that host and any terminal there otherwise
(`PaletteBackend::list_path`, `MuxClient::request_host_path_list`), so the insert style is that
pane's: quoted for its shell, `@path` for Claude Code. A path from another host goes in as
`host:/abs/path`. `&` lists the same way ([Files](#files)).

Keys in the footer and on rows are drawn as key icons, Apple's on macOS and iOS and ISO 9995's
elsewhere (`zz_gpui_kit::kbd::KeyGlyphs`, [design system](/designs/design-system.md)), and a
command's tmux shortcut (`C-b c`) as one small cap per key.

Session, window, and pane targets use stable IDs for execution; rows display names and breadcrumbs.
On the attached host, a terminal row whose [agent lens](/concepts/agent-lens.md) runtime is open
reads `Agent · running`, `Agent · waiting for input`, `Agent · idle` or `Agent · failed` from that
pane's agent state (`tree_pane` in `crates/zz/src/command/palette.rs`).
Successful local navigation and command requests use the app's existing notification surface.

The fuzzy matcher (`zz_client::fuzzy::fuzzy_match`) ignores query whitespace and case. It scores an
ordered subsequence by the first matching character's position plus twice the skipped characters
between matches, and returns UTF-8 byte ranges for text highlights.

# GUI actions

`>` lists settings and app actions, each with its value now on the right. The list is data in
`zz_client::gui_actions` (`GUI_ACTIONS`): an id, a title, aliases, its values (fixed, or listed by
the client, like the themes) and whether a highlighted value previews. `>` is the `GuiLevel`
(`crates/zz-ui/src/command/gui_level.rs`) each client's `PaletteBackend::action_level` returns,
built with the actions that client has and their values when the palette opens
(`palette_backend_with` on desktop and in `clients/app`). The level emits `GuiEvent`s through
`CommandPaletteEvent::Gui`, and each client writes what its own Settings writes: config keys on
desktop (`crates/zz/src/command/gui.rs`), `Preferences` on iPad and web
(`clients/app/src/app/gui.rs`). Themes are named by `theme_offer`, which uses the Theme picker's
`theme_labels`. A test in `zz-config` checks that every value an action writes is one the config
reads.

| Action | Values | Desktop writes | iPad and web |
| --- | --- | --- | --- |
| Titlebar mode | Sidebar, Title bar | the sidebar's mode, as Toggle Sidebar flips it | `sidebar`, wide windows only |
| Pane gaps | On, Off | `pane-gaps` | `gaps` |
| Window blur | On, Off | `window-background-blur`, where the platform has blur | not offered |
| Theme | the built-in and saved themes | `interface-theme`, as Settings picks one | `theme` |
| Sidebar layout | List, Tree | `flat-sidebar` | `flat_sidebar` |

Typing matches titles, aliases and every value (`Theme › macOS`). Enter on an action with two values
flips it; Enter or Tab on the others opens their values under the action's header with the one in
use highlighted, and Enter there applies one and closes (`LevelEffect::Finish`). Backspace on an
empty field or Shift-Tab goes back to the actions. A search that matches nothing ends with the Ask
row. Each apply is recorded as `RecentKey::Gui("id=value")`, labelled like `Theme › Yaru`; Enter on
it in the empty palette opens `>` and applies that value again (`open_recent`).

Theme previews: the highlighted theme shows at once (`GuiEvent::Preview`). It goes back when the
highlight returns to the theme in use or Backspace leaves the themes (`GuiEvent::Revert`), and
whenever the palette closes without Enter, by Esc, a click outside or being replaced: each client
reverts when the palette entity is released (`observe_release_in`), since a level that is closing
cannot emit. On
desktop a preview reads the config file, applies the theme edit in memory
(`zz_config::interface_theme_source`), and installs only the parsed look and theme globals
(`config::preview_interface_theme`); nothing is written until Enter, and a revert reloads the file.
On iPad and web a preview applies a copy of `Preferences` without saving it.

# Asking the driver

`?` is the `AskLevel` (`crates/zz-ui/src/command/ask_level.rs`). While there is a driver it swaps
the palette for the Driver chat as soon as it is typed (`CommandPaletteEvent::Driver`), with the
composer filled: the place as a mention when one was typed (`$foo?` gives `@macbook:$7 `, from
`place::written`), then what followed the `?` (all of it when the line was pasted). A search that
matches nothing ends with an Ask row, and text ending in `?` puts it first (`why is ci red?` still
searches, D12); Enter on it opens the chat with the typed text in the composer, where Enter sends. With no driver running the row reads Wake the driver, which
runs the same recovery as the sidebar's Wake, and with `driver-agent` off it reads Driver Agent is
off and opens Settings › Agents. `prefix A`, the sidebar's last message and the title bar's chat
button still open the chat directly.

# Settings

Settings > Advanced > Command palette exposes three preferences:

| Config key | Values | Default |
| --- | --- | --- |
| `palette-window-layout` | `grouped`, `flat` | `grouped` (the daemon window chooser's tree) |
| `palette-show-keys` | `true`, `false` | `true` |
| `palette-vim-mode` | `true`, `false` | `false` |

The iPad and web clients keep the same three in their own preferences
(`Preferences::palette_settings` in `clients/app/src/preferences.rs`); the iPhone hides the group.

The `open-command-palette` chrome action lives in the UI key table and can be remapped.
Its default shortcuts replace the terminal Clear History bindings on those same keys.
Clear History remains available through the menu and custom bindings.

# Vim mode

With `palette-vim-mode` on, the unified palette, `prefix :` and the `prefix s`/`prefix w` chooser
have vi's insert and normal modes. Daemon value prompts and raw-key prompts do not. Every open
starts in insert; the chooser going back to the default view keeps the mode. A badge at the right
of the field names the mode, NORMAL in the accent, and normal mode draws a block caret.

| Normal key | Does |
| --- | --- |
| `j` `k` | Down, Up |
| `l` `→` | Tab: into the highlighted row |
| `h` `←` | Shift-Tab: up a level, keeping the chip |
| `x` | Backspace: a character, then the last chip once the field is empty, or a typed sigil's chip back into its text |
| `yy` | Alt-Enter (`CopyAddress`): copy the row as `@host:target`, or an action level's `copy_text` |
| `gg` `G` | first, last row, asking an action level first (`ActionKey::First`, `Last`) |
| `i` `a` | insert at, after the caret |
| `/` | clear the text after the chips and any action sigil, insert |
| a sigil (`^ $ @ % : > & ?`) | insert, then type it |
| Esc, `ctrl-[`, `q` | close |

In insert, Esc and `ctrl-[` go to normal. Keys that have a plain twin dispatch the twin's action
(`CommandPaletteView::dispatch_twin`), so the two cannot drift apart. The reducer is pure
(`crates/zz-ui/src/command/palette_vim.rs`) and matches the typed character, falling back to the
key on layouts without Latin letters, so `@` typed with Option still counts as a sigil. Sigils are
`zz_client::place::is_sigil`.

Normal mode puts the field in `InputState::set_normal_mode`: it reports that it takes no text, so
macOS stops routing letters to a CJK input method first and Wayland turns text-input off, and it
drops text that still arrives through the input handler. The field's actions still edit it, which
is how `x` deletes.

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

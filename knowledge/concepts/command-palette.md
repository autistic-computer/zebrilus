---
type: Concept
title: Unified palette and tmux command completions
description: Searching places and commands, filling arguments, picking files, and editing daemon prompts.
resource: crates/zz-ui/src/command/palette_view.rs
tags: [command-prompt, tmux, completions, ui]
timestamp: 2026-11-24T00:00:00Z
---

## Words, places and verbs

Open the desktop palette with Cmd-K or Cmd-P on macOS, Ctrl-Shift-K or
Ctrl-Shift-P elsewhere. Search across places (hosts, sessions, windows and panes)
and actions, or type `:` for a raw tmux command. Places match across path
components. The current host chip scopes the search. Removing it includes
other hosts. Results rank by match, proximity and recent picks.

Enter on a place navigates there. Enter on an action asks for any missing
arguments in order. A place, text, setting value or path can fill a slot.
Drilling into a place first offers actions for that target. Completing the slots
executes on the target's host. The footer shows the command or local action.
See `place::search` and `verbs::compile` in `crates/zz-client/src`.

With an empty query the palette offers attention items, recents and navigation.
Chat mode includes a row per pane recognized as an agent program across connected
hosts. `palette_chats` in `crates/zz-ui/src/command/palette_model.rs` uses
`window_summary::pane_agent`, which recognizes Claude Code, Codex, Antigravity,
Grok and Cursor. A pane appearing here can still be running the vendor's terminal UI.
Recent places, actions, settings and files are stored per client.

## Chips and keys

| Key | Effect |
|---|---|
| Up, Down, Ctrl-P, Ctrl-N | Move through selectable rows, wrapping past section headings |
| Enter | Go to the place, start or run the action, accept the argument or choose the file |
| Alt-Enter | Copy the place address, raw mention or plain file path and keep the picker open |
| Tab | Enter the highlighted level or complete a raw tmux command |
| Shift-Tab, Backspace in an empty field | Back out of a folder, argument or chip |
| Left, Right | Move the caret in ordinary search. A daemon tree chooser can use them for branches. |
| Escape | Dismiss the palette, or leave insert mode when Vim mode is active |

`palette-vim-mode` opens the local palette and supported daemon choosers in insert
mode. In normal mode `j/k` navigate, `h/l` change level, `yy` copies an address,
`gg/G` jump to an end, `i/a` enter insert mode, `/` starts a new search and `q`
closes. See `libs/penna/src/blocks/palette/vim.rs`.

## Your own verbs

Each host's tmux `command-alias` entries become verbs on that host alongside
built-in actions. Actions that need unsupported arguments can open the raw
command line for editing. The desktop also offers local settings as actions.
See `alias_verbs` in `crates/zz-client/src/verbs.rs`.

## Files

`insert path` or `prefix F` opens a host's file picker. Its starting directory
comes from the target or an explicit `-c`. Type a directory and a filter, Tab
into folders, and Shift-Tab back out. Enter inserts into a terminal or copies
the path when the target cannot accept it. The editor's open-file operation
and the new-chat folder picker select files and directories for their callers.
Alt-Enter copies a plain path and leaves the picker open. The daemon lists a
remote directory on the host that owns it. See `crates/zz-ui/src/files.rs` and
[the path picker](/designs/path-picker-and-which-key.md).

<a id="targets-the-lens-composers-mentions"></a>

## Targets

A written address such as `@host:%7`, `@host:@41` or `@host:$7` identifies
a pane, window or session. File references can name a host and line number.
The chat composer's `@` menu also offers chats, providers and files. Enter inserts
the friendly name with the underlying reference, and Alt-Enter copies that
reference. See `crates/zz-ui/src/target.rs` and [agent mentions](/designs/agent-mentions.md).

## Daemon prompts and choosers

The local search and daemon-issued prompts share the presentation. In a daemon
prompt, the daemon owns the template, initial value and bounded history. Value
prompts such as rename preserve their initial text. Raw command suggestions use
the catalog, supported options and live targets. `CommandPromptAction::Update`
synchronizes the text and cursor, `Submit` carries the final text, and `Close`
cancels. The daemon expands the template and executes it.

`choose-tree -s` and `-w` use the daemon's rows, selection and expansion.
The desktop handles a windows tree with `CommandPaletteView`. Client and other
chooser kinds use their dedicated surfaces. See the chooser dispatch in
`crates/zz/src/workspace/view.rs`, `crates/zz-client/src/completion.rs`,
`crates/zz-protocol/src/catalog.rs` and [choose-tree](/tmux/choose-tree.md).

---
type: Configuration
title: Application configuration
description: Where zz reads and writes desktop settings, how overrides reach the local daemon, and which values the config accepts.
resource: crates/zz-config/src/lib.rs
tags: [configuration, appearance, mux, settings]
timestamp: 2026-11-24T00:00:00Z
---

## Discovery and loading

The desktop reads the first existing `zz/config` in this order. Unset, relative
and duplicate roots are skipped. An edit goes to the selected file, or to the
first available absolute root if the file does not exist. An isolated dev build
uses its own application directory.

| Platform | Configuration roots, first to last |
|---|---|
| Linux and other Unix | `$XDG_CONFIG_HOME`, `$HOME/.config` |
| macOS | `$XDG_CONFIG_HOME`, `$HOME/.config`, `$HOME/Library/Application Support` |
| Windows | `%XDG_CONFIG_HOME%`, `%APPDATA%`, `%LOCALAPPDATA%`, `%USERPROFILE%\.config`, `%HOME%\.config` |

The file is UTF-8, limited to 64 KiB. Each assignment is `key = value`. Blank
lines and comments are ignored. A trailing comment starts at an unquoted,
whitespace-delimited `#`, so a color such as `#64a2ff` is a value. Later valid
scalar entries win. Invalid values produce line diagnostics. Numeric values
must be finite and within the key's range. Booleans accept true/false, on/off,
yes/no and 1/0. See `config_candidates_for`, `read_config_source`, `parse_config`
and `ConfigKey::numeric_range` in `crates/zz-config/src/lib.rs`.

## Which process applies a change

| Setting | Owner |
|---|---|
| Desktop UI, browser, editor and chrome bindings | The desktop applies the selected file and watches for changes. Linux watches directories. The fallback polls file metadata. Thin clients store their own device preferences. |
| Terminal appearance and supported mux options | The desktop sends the complete ordered override list to its local daemon on connect and reload, including an empty list after removal. Connecting to a remote daemon leaves that host's config alone. |
| tmux commands and bindings | The daemon loads explicit `-f` files in order, or `zz/mux.conf`. `reload-config` repeats that selection. The local overrides apply over sourced settings. |
| Saved hosts | `host-NAME = ENDPOINT` entries belong to the client. Use `zz fleet add`, `zz fleet remove` or Settings to edit them. `local` and `all` are reserved names. |
| Tailnet | Apps and daemons can read tailnet settings from the file before a client connects. `tailnet` accepts `auto` (default), `on`, `off` and `own`. `tailnet-allow` accepts `owner` or `members`. `tailnet-hide` hides discovered peers. `tailnet-hostname` and `tailnet-control-url` configure the owned node. |

See `SetConfigOverrides` in `crates/zz-protocol/src/message.rs`,
`crates/zz/src/config/watch.rs`, [fleet attach](/designs/fleet-attach.md)
and [tailnet settings](/designs/tailnet-node.md).

## Desktop settings

The defaults below are for the desktop. The effective look can also depend
on the platform's theme, available fonts, OS preferences and terminal palette.

| Setting | Default | Values or effect |
|---|---|---|
| `theme-mode` / `app-icon` | `system` / `automatic` | Each can be pinned to `light` or `dark` |
| `ui-font-family` / `ui-chat-font-family` / `ui-mono-font-family` | Plumula Sans / Plumula Serif / terminal font | Installed family. `.SystemUIFont` selects the system UI font. |
| `chrome-terminal-colors` | true | Let a matching terminal theme supply chrome colors |
| `chrome-accent` | `theme` | `ink`, blue, purple, pink, red, orange, yellow, green, graphite or a hex color |
| `chrome-tint` / `chrome-tint-hue` | `0.15` / `60` | Tint strength 0–1 and hue 0–360°. The look can derive them from the backdrop while unset. |
| `chrome-contrast` / `chrome-density` | `1` / `regular` | Contrast 1–2. Density accepts `compact`, `regular` and `comfortable`. |
| `motion` / `pointing-hand` | `default` / false | Motion accepts `off`, `default` and `fast`. The pointer switch selects hand cursors for controls. |
| `control-shape` / `interface-glass` | `rounded` / `liquid` | Controls can be `pill`. Glass accepts `none`, `liquid` or one JSON material object. |
| `widget-corner-radius` / `window-corner-radius` | `12.25` / `13.5` | 0–25 and 0–32 logical pixels. Window corners apply to the app-drawn frame. |
| `pane-gaps` / `pane-margin` | true / `6` | Draw separate pane frames. Margin accepts 0–32 logical pixels. |
| `pane-glow-strength` / `pane-inactive-opacity` | `1` / `0.7` | Selected-pane contrast 0–2 and retained inactive opacity 0–1 |
| `flat-sidebar` / `program-colors` | true / true | Chat-mode sidebar and program-colored tiles. Turning chat mode off shows the session tree. |
| `palette-window-layout` / `palette-show-keys` / `palette-vim-mode` | `grouped` / true / false | Window layout can also be `flat`. See [command palette](/concepts/command-palette.md). |
| `which-key-delay` | `400` | Delay in milliseconds, 0–2000 |
| `browser-search-provider` / `browser-egress` | `google` / true | Providers include `duckduckgo` and `brave`. Egress selects routing for remote-host panes. |
| `browser-element-selector-hotkey` | `cmd-shift-c` on macOS, `ctrl-shift-c` elsewhere | One modified keystroke for the element picker |
| `browser-remote-debugging-port` | `0` | Off at 0. Use 1024–65535 for [browser debugging](/browser/agent-cdp.md). |
| `experimental-editor-pane` / `editor-font-size` | false / `13` | Gate editor pane creation in builds with the editor feature. Font size accepts 8–32. |
| `editor-line-numbers`, `editor-relative-line-numbers`, `editor-soft-wrap`, `editor-vim-mode` | true for each | Editor display and navigation |
| `tray`, `sounds`, `notifications`, `check-for-updates` | true for each | Desktop integration, attention and update checks |
| `quit-daemon-on-exit`, `auto-restart-stale-daemon` | false for each | Opt in to ending sessions on app quit or replacing an incompatible local daemon on connect |

### Repeatable chrome bindings

`chrome-keybind = TABLE:KEY=ACTION` and `chrome-unbind = TABLE:KEY` apply in
file order. Tables are `ui`, `sidebar`, `browser`, `terminal` and `prefix`.
The client prefix table applies to keys the daemon's prefix table leaves unbound.
Chrome bindings use client action names. The mux's `bind-key` belongs in
`zz/mux.conf`. See [key tables](/tmux/key-tables.md).

## Terminal, mux and agent settings

Appearance keys use the [terminal appearance](/terminal/appearance.md) grammar for
theme, palette, background, foreground, fonts, spacing, cursor, selection and
search colors. Repeated palette and font directives preserve file order.
The terminal loader supplies defaults and resolves light/dark themes. Settings
can show the source of the current value.

| Supported mux overrides | Keys |
|---|---|
| Keys and focus | `prefix`, `prefix2`, `mode-keys`, `escape-time`, `extended-keys`, `mouse`, `focus-events`, `focus-follows-mouse`, `clear-on-attach` |
| History and selection | `history-limit`, `history-trickle`, `word-separators`, `copy-command`, `set-clipboard`, `buffer-limit` |
| Panes | `synchronize-panes`, `experimental-editor-pane` |
| Chats | `agent-command` (default `codex`), `agent-claude-code-command` (default `claude`), `agent-auto-approve` (`off`, `reads` by default, or `all`) |

`MuxOptionKey::from_config_key` is the accepted set. Other tmux options can
be set in `zz/mux.conf`. Agent command overrides accept a command or a JSON
adapter specification. For the `reads` tier, `tier_approves` in
`crates/zz-daemon/src/agent/runtime.rs` accepts read, search, fetch and think
requests. The provider's own approval mode also affects what it asks. [Agent panes](/concepts/agent-panes.md)

## Editing and importing

Scalar edits target the last matching assignment, preserve unrelated lines and
comments, and append a line if needed. Removing that last occurrence can reveal
an earlier one. Some grouped operations replace or remove every entry for a
key. The writer checks the size bound and replaces the file atomically. For a
symlink it first writes the target. If the target denies replacement or is
read-only, the writer retries at the original path and can replace the link. See `edit_config_source` in
`crates/zz-config/src/lib.rs` and `atomic_write` in `crates/zz-daemon-client/src/fleet_hosts.rs`.

Settings can import a Ghostty config once. The import copies theme files and
writes the donor's explicit appearance settings into `zz/config`. Scalar values
replace existing settings, and cumulative palette and font groups are rebuilt.
`import-tmux-config [path]` copies tmux commands into a marked block in
`zz/mux.conf`. Re-import replaces that block and keeps surrounding user
config. See `crates/zz-config/src/import.rs` and [tmux config](/tmux/conf-parser.md).

```ini
motion = off
ui-chat-font-family = Plumula Serif
pane-inactive-opacity = 0.85
chrome-keybind = ui:D-2=select-window-4
prefix = C-a
history-limit = 20000
```

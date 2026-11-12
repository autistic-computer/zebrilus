---
type: Configuration
title: Application configuration search paths
description: Platform-aware discovery, bounded parsing, agent driver options, settings editing, and one-shot donor import for client-local knobs plus ordered daemon-owned overrides.
resource: crates/zz/src/config/mod.rs
tags:
- configuration
- ala
- window
- appearance
- mux
timestamp: 2026-10-31T00:00:00Z
---

# Overview

The GUI process loads the first existing `zz/config` file from the user's platform configuration
roots. `crates/zz/src/config/mod.rs` resolves the candidates when `run_app` enters the ala application
closure, parses named client-local behavior/layout/theme/browser knobs plus
chrome-color entries into typed
`AppConfig`, `BrowserConfig`, and `UiFontConfig` values,
collects repeatable `chrome-keybind`/`chrome-unbind` entries for the client-local keymap, and collects the supported
daemon-owned appearance and mux entries as ordered raw `(key, value)` pairs. Client-reserved
`host-<name> = <uri>` entries ([fleet attach](/designs/fleet-attach.md)) are matched before any
of that: validated via `zz_daemon_client::Endpoint::parse`, published through a dedicated `FleetHosts`
global (not `AppConfig`, which stays `Copy`), and never forwarded to any daemon.
Three surfaces write these lines and nothing else: `zz fleet add <name> <ssh-destination>` from the
CLI (which `zz fleet sync` also runs on each host for the lines it lacks), the sidebar's final
**Add host** row, and the inline form in **Settings › Hosts**. The names `local` and `all` are
reserved: `all` is the CLI's `--host all` fan-out. The
two GUI fields share the same `parse_add_host` validation and name the entry after the destination's
host component. The underlying writer replaces an existing `host-<name>` value in place while
preserving every other byte, comments included; the GUI fields reject a duplicate name before they
call it. `zz list-hosts` prints each host in config order with its probed `host_state` and
`host_version` (a `-F` that names no probed variable skips the ssh probe; `--json` adds
`host_protocol` and `host_error`), and `zz fleet list` keeps the old name-and-endpoint listing; rejected `host-` lines go to stderr. `zz fleet remove <name>` deletes every matching
host line, as does a remote row's **Close host** . which additionally republishes `FleetHosts`
(`config::remove_fleet_host_live`) so the running fleet drops the machine immediately. A
`--host` value that names no host line is read as an ssh destination (`user@box`, `box:2222`,
`ssh://...`), so the CLI can reach a machine before it has a line. There is no daemon-side setup:
ssh owns transport, and zz adds only its own ed25519 key (`zz fleet key`), which the first
login to a host listed here appends to that host's `authorized_keys`; a raw `--host` destination
gets nothing written. A host line can also name a tailnet machine (`host-arch = tailnet://arch`).

Five keys belong to [tailnet hosts](/designs/tailnet-node.md) and are read by both the daemon and
the app from the file itself, so they work before any client connects: `tailnet` (`auto` by default,
`on`, `off`, or `own` for zz's own Tailscale node), `tailnet-allow` (`owner` by default, or
`members`), `tailnet-hide` (a comma-separated list of found machines to leave out), and, for `own`
only, `tailnet-hostname` (letters, digits and dashes; `zz-<machine>` by default) and
`tailnet-control-url` (an `http://` or `https://` control server). The app scans for machines every 30 seconds
and publishes them through a `DiscoveredHosts` global; `config::known_hosts` merges them after the
host lines, skipping any whose name or tailnet machine a line already has, and the host registry is
built from that merged list. **Remove host** on a found machine adds its name to `tailnet-hide`
instead of editing host lines, and **Settings › Hosts** has a Tailscale group with the on/off
switch, the sign-in state, a Show button for hidden machines, and the own node's **Use**, **Sign in**
and **Sign out** buttons. These local schema entries retain
`Default`/`Override` provenance. Daemon-owned value
grammar deliberately stays in the zz-terminal appearance loader or
mux `set-option` engine.

On Linux the GUI watches the candidate directories, the selected file's resolved directory and the
`themes` folder with inotify, and re-checks path, mtime and size after each change; other platforms
poll every 500ms. Either way it live-applies edits, file appearance, precedence changes, and deletion.
On connect and every detected change it sends the complete
ordered daemon-owned set through `SetConfigOverrides`, including an empty set after the
last override is removed. The daemon never reads `zz/config`; it stores the latest appearance and
mux subsets, layers appearance above built-in defaults (it reads no donor appearance config of its
own; the one external file it opens is the Ghostty theme a `theme` override names, see
[Terminal appearance](/terminal/appearance.md)), and
dispatches mux values through the existing global `set-option` grammar. Appearance overrides replay
after `reload-config` and color-scheme changes (the effective light/dark mode, pinned or system); mux overrides replay after every
`zz/mux.conf` load so a reload cannot revert a GUI choice.

The daemon loads only `zz/mux.conf`, or explicit `-f` files that replace it in argument order.
Reload repeats that selection. It never reads tmux donors implicitly. The first-run offer imports
Ghostty appearance into `zz/config` and tmux commands into a marked `zz/mux.conf` block.
Settings imports from any file the system picker returns and edits the same files through structured rows.

# Discovery and loading

| Platform | Search order |
| --- | --- |
| Linux and other Unix | `$XDG_CONFIG_HOME/zz/config`, then `$HOME/.config/zz/config` |
| macOS | `$XDG_CONFIG_HOME/zz/config`, `$HOME/.config/zz/config`, then `$HOME/Library/Application Support/zz/config` |
| Windows | `%XDG_CONFIG_HOME%\zz\config`, `%APPDATA%\zz\config`, `%LOCALAPPDATA%\zz\config`, `%USERPROFILE%\.config\zz\config`, then `%HOME%\.config\zz\config` |

Only absolute configuration roots are accepted, as required for `XDG_CONFIG_HOME`; duplicate paths
are removed while retaining precedence. A missing file or unavailable configuration root silently
selects built-in defaults. Other I/O or UTF-8 errors warn and select defaults. Reads are capped at
64 KiB before parsing so an accidentally large file cannot create unbounded configuration work.

When a settings edit must create the file, it uses the first available absolute root in the
platform search order above and creates parent directories as needed. On Windows without an
`XDG_CONFIG_HOME` override, this normally means `%APPDATA%\zz\config`; `HOME` is not required.
Once any candidate exists, edits continue to target the first existing file selected by normal
discovery. Fleet-host edits use the same rule.

# Schema

## Repeatable chrome bindings

`chrome-keybind = <table>:<key>=<action>` replaces or adds one binding, and
`chrome-unbind = <table>:<key>` removes one. The allowed tables are `ui`, `sidebar`, `browser`,
`terminal`, and `prefix`; action names come from `zz_client::ChromeAction`. Keys use the chrome grammar from
`zz-client`, including `D-` for Command/Super and `S-` for Shift. These directives may appear more
than once, preserve file order, and stay client-side. They are separate from the scalar
knobs below and from the daemon-owned tmux tables in `zz/mux.conf`.

The `prefix` table holds the key pressed after the prefix, and only for keys the daemon's prefix
table leaves unbound: a `bind A …` in `zz/mux.conf` wins over it. zz binds nothing there by
default since `prefix A` (`open-driver-chat`) went with the Driver Agent on 2026-10-28. A key bound
there does not show in `list-keys` or the which-key sheet, which read the daemon's tables.

The desktop binds `Cmd+1` through `Cmd+9` on macOS and `Ctrl+1` through `Ctrl+9`
on Linux to `ui:select-window-1` through `ui:select-window-9`. The numbers run
through the windows the sidebar lists, top to bottom across sessions and connected
hosts, folded sessions included, so a window keeps its number while you move
around; a number in another session or host switches there and focuses that window.
Holding the modifier for 300ms puts each number's key on its window row in both
sidebars, in place of the age. Missing positions do nothing. The workspace
intercepts these shortcuts before pane shortcuts, including browser tab selection.
Dialogs, popups, menus, local palettes, and Settings retain their keyboard handling.
For example, `chrome-keybind = ui:D-2=select-window-4` selects the fourth window
with Cmd+2; `chrome-unbind = ui:D-2` restores the pane's handling of that chord.

## Client-local keys

The client-local schema includes these scalar settings and chrome colors.

| Key | Default | Valid range | Consumer |
| --- | ---: | --- | --- |
| `window-corner-radius` | `13.5` | `0..=32` | The app-drawn window frame's corner, visible only under Linux client-side decorations |
| `motion` | `default` | `off`, `default` or `fast` | Interface motion. `off` stops transitions, loading indicators, scrollbar fades, and animated UI images; `fast` runs them at twice their designed speed, through the process-wide `ala::animation_speed` that scales ala `with_animation` timings, every spring step, and the hand-timed fades (chevrons, pane arrivals, the sidebar slide, press highlights, streaming text, the agent pulse). Scroll physics, cursor blink and timers stay as they are. Settings > Interface > Behavior shows it as Motion |
| `tray` | `true` | `true` or `false` | Shows a native tray icon while the GUI runs. Closing the window or quitting hides zz when the icon is available, `quit-daemon-on-exit` is false, and the local daemon has sessions; otherwise the usual close or quit behavior applies. Applies live from Settings → System or file edits. Quit and Stop Sessions in the tray menu stops the daemon |
| `notifications` | `true` | `true` or `false` | Whether the desktop app posts a system notification when a pane needs input, fails or finishes while no zz window is active. Clicking it raises the window and selects the pane. macOS asks for permission on the first one. Chimes are separate (`ZZ_AGENT_SOUND=0`) |
| `quit-daemon-on-exit` | `false` | `true` or `false` | Whether quitting the app stops the daemon even while sessions are live |
| `auto-restart-stale-daemon` | `false` | `true` or `false` | Whether a local daemon on another protocol, or below this zz's `MIN_DAEMON_REVISION`, is terminated and replaced on connect. Off by default because it ends every running session |
| `check-for-updates` | `true` | `true` or `false` | Whether the GUI fetches the GitHub release list ten seconds after launch and daily after that, offering a newer release as a toast. Update in the toast or on the About page downloads the release in the app, shows its progress and offers Restart to update; nothing downloads before that click. The channel follows the running build (a `-beta.N` version takes prereleases). One anonymous request; nothing else is sent. Release builds only unless `ZZ_UPDATE_CHECK=1`; `ZZ_UPDATE_CHECK=0` silences every build. Surfaced in Settings under About |
| `flat-sidebar` | `false` | `true` or `false` | Whether the sidebar shows the flat window list instead of the session tree. The eye button at the bottom right of the sidebar sets it |
| `program-colors` | `true` | `true` or `false` | Whether a flat sidebar tile takes the colors of the program running in it. Off, tiles keep the theme's color and draw the program's mark in its ink. Settings › Appearance › Sidebar sets it; the web and iPad clients keep their own switch |
| `experimental-editor-pane` | `false` | `true` or `false` | Whether new Editor panes can be created at all . picker row, palette completion, and the daemon's `select-pane-kind editor` |
| `pane-gaps` | `false` | `true` or `false` | Whether panes use the gapped treatment: margin, radius, a glass frame, and no split dividers |
| `pane-glow-strength` | `1` | `0..=2` | How far the active gapped pane stands apart: the others' fills darken and their rims dim while it keeps the content plate; Panes settings display it as Selected pane contrast, 0–200%. `0` draws it like the others |
| `pane-inactive-opacity` | `0.7` | `0..=1` | Retained strength of inactive pane content and chrome; `1` disables dimming |
| `pane-margin` | `6` | `0..=32` | Inset around each pane, on every platform; applies only with `pane-gaps`. The window's top strip grows with it, and the sidebar's buttons move in with it, so they stay on the pane headers' center lines |
| `widget-corner-radius` | the platform's (`12`) | `0..=25` | The look's `radius` knob in pixels: the corner of containers, of controls unless they are pills, and the base of a gapped pane's corner. 25 is a round corner, never a pill. Settings shows it as Corner radius, a slider from 0 to 100% where the pixels are 25 × share² (12 is 69%), written with two decimals |
| `control-shape` | the platform's (`pill` on macOS, `rounded` elsewhere) | `rounded`, `pill` | Whether controls follow the corner radius or are pills (`control-radius` `full`). Settings shows it as Controls |
| `interface-glass` | `liquid` | `none`, `liquid`, or a glass material as one line of JSON | The glass popovers, menus and glass roles draw, and whether the window is see-through. While it is on, the desktop shows through the chrome, blurred, where the compositor can blur, and less of it through panes and the settings page, which sit on a plate of the background; `none` keeps the window opaque. Glass has one look for light and one for dark (penna's Glass looks): on macOS the chrome takes 0.82 in light and 0.65 in dark, and content 0.95 and 0.85; where zz can't shape the backdrop the window takes 0.97. The system's Reduce transparency (macOS, iOS), Transparency effects off (Windows) and `prefers-reduced-transparency: reduce` (the browser clients) draw it as `none` without changing the key. Pasting a theme file with its own material writes the material here, drawn over the mode's backdrop and planes. Settings shows it as Glass, a switch: on writes `liquid`, off `none`. Any other word is reported as invalid and the window draws `liquid` |
| `chrome-accent` | `theme` | `theme`, `ink`, `blue`, `purple`, `pink`, `red`, `orange`, `yellow`, `green`, `graphite`, or `#rrggbb` | The accent, which becomes the highlight: primary fills, switches, selection. `theme` (or no line) keeps the platform's (blue on macOS, ink elsewhere); `ink` draws primary fills in the text color. Each name is a pair of colors tuned for light and for dark; a hex color is used in both modes |
| `chrome-terminal-colors` | `true` | `true` or `false` | Whether the window takes its colors from the terminal theme: the background exactly, the text lifted to 7:1 when the theme's reads under that, the accent and status colors from the palette, fitted to read. Only a themed terminal counts; zz's built-in terminal colors leave the look alone. Settings › Appearance › Terminal sets it |
| `chrome-tint` | the platform's (`0.15`) | `0..=1` | The amount of the look's `neutral`: how far the window's background leans toward `chrome-tint-hue`; everything drawn from the background leans with it, and the text leans about a third as far. Settings shows it as Tint strength, 0–100% |
| `chrome-tint-hue` | the platform's (`60`) | `0..=360` | The hue of the look's `neutral`, in Oklch degrees; `60` is the warm default. Settings shows it as Tint |
| `editor-font-size` | `13` | `8..=32` | Buffer text size in editor panes, in pixels |
| `which-key-delay` | `400` | `0..=2000` | Milliseconds after the prefix, or inside a `switch-client -T` table, before the which-key sheet lists the next keys; `0` turns the sheet off. Read when the timer starts |
| `editor-line-numbers` | `true` | `true` or `false` | Whether editor panes draw the line-number rail |
| `editor-relative-line-numbers` | `true` | `true` or `false` | Number the rail by distance from the cursor line, which keeps its absolute number |
| `editor-soft-wrap` | `true` | `true` or `false` | Wrap long lines at the pane edge instead of scrolling horizontally |
| `editor-vim-mode` | `true` | `true` or `false` | Modal vim editing in editor panes: normal, insert, visual and visual-line modes |
| `browser-element-selector-hotkey` | `cmd-shift-c` on macOS; `ctrl-shift-c` elsewhere | One ala keystroke containing Control, Alt, Command/Super, or Function; Shift is optional | Toggle the element selector while a Browser pane owns the Browser key context |
| `browser-remote-debugging-port` | `0` (off) | Bare integer: `0` or `1024..=65535` | Loopback CDP endpoint; `ZZ_BROWSER_REMOTE_DEBUGGING_PORT` overrides this value at CEF startup. Restart the client after changing an initialized runtime. The Browser page's **Agents** switch writes `9222` or removes the key; other ports are file-only |
| `browser-search-provider` | `google` | `google`, `duckduckgo`, `brave` | Where a Browser pane's address bar sends an entry that is not an address |
| `browser-egress` | `true` | `true` or `false` | Whether a Browser pane opened while attached to a remote ssh host routes its traffic through that host; panes already open keep the route they were created with |
| `ui-font-family` | the platform's (the system font) | Installed font family, or `.SystemUIFont` | Interface text across desktop windows; applies live and stays separate from terminal and editor fonts. Pasting a theme file with a `face` writes it here |
| `ui-mono-font-family` | the terminal font | Installed font family | Interface's Mono font: code blocks and inline code in Markdown, the editor, tool previews and every other monospaced line outside the terminal. Its list opens on Monospaced. Terminal font removes the line, and Reset look removes it too; a theme file never sets it |
| `theme-mode` | `system` | `system`, `light`, `dark` | Follow the OS appearance, or pin one mode |
| `app-icon` | `automatic` | `automatic`, `light`, `dark` | Which render of `assets/zz.icon` the macOS Dock tile wears; `automatic` defers to the bundle's compiled icon when packaged (so tinted/clear dock styles work) and follows the OS appearance in bare builds, independently of `theme-mode` |
| `chrome-contrast` | `1.0` | `1.0`–`2.0` | Raises the chrome derivations (surface steps, fills, dividers and muted text) and adds no edges; out-of-range or invalid values are reported and fall back to `1.0`. Settings shows it as Increase contrast, a slider from 0% (1.0) to 100% (2.0), written with two decimals. |
| `pointing-hand` | `true` | `true` or `false` | The pointer over anything you can click in the chrome: a hand when on, the arrow when off. Pages in a browser pane and links in a terminal keep their own pointer. Settings shows it as Pointing hand under Interface › Behavior; the thin clients keep it in their preferences as `pointer` (`hand` or `arrow`) and leave it off the phone, and the HTML client shows the same row and sets `data-pointer` on its root |
| `chrome-density` | `regular` | `compact`, `regular`, `comfortable` | The default control and row height in the chrome: 24, 28 or 32px for buttons, fields, selects and list, menu and picker rows; text, gaps and the other control sizes stay. Invalid values are reported and fall back to `regular`. Settings shows it as Density under Interface › Text. |

`chrome-preset`, `chrome-preset-light`, `chrome-preset-dark`, `chrome-background`,
`chrome-foreground`, `chrome-success`, `chrome-warning` and `chrome-danger` are
retired (`zz_config::RETIRED_APPEARANCE_KEYS`): a line with one of them is reported with
`` `<key>` was removed: set the look in Settings > Interface `` and skipped. Chrome colors come from the look, the accent and the tint, never from
per-color keys.
`chrome-accent` was on that list from 2026-10-11 to 2026-10-15 and is live again as the accent
above, so an old `chrome-accent = #rrggbb` sets the accent again.
`interface-style`, `widget-outline-width`, `widget-shadow-strength`, `widget-row-inset`,
`widget-primary` and `widget-glass` are retired too (`zz_config::RETIRED_MATERIAL_KEYS`): a line
with one is reported with `` `<key>` was removed: set the look in Settings > Interface `` (`widget-glass`
with `` set `interface-glass`, or Glass in Settings > Interface ``) and ignored.
`window-background-blur` and `reduce-transparency` joined that list on 2026-11-09, when Glass took
over the window blur and zz began following the system's setting: the first is reported with
`` glass blurs the window now; set `interface-glass` to none for an opaque one `` and the second with
`` zz follows the system's Reduce transparency; set `interface-glass` to none to turn glass off ``.
`interface-frost` joined it on 2026-11-12, when glass took one look per mode: a line with it is
reported with `` `interface-frost` was removed: set `interface-glass` to none or liquid, or Glass in
Settings > Interface `` and ignored. zz never rewrites it; it stays, reported, until it is deleted.
`pane-glass`, `pane-corner-radius`, `pane-border-width`, `pane-background-opacity` and
`pip-background-opacity` are retired pane keys (`zz_config::RETIRED_PANE_KEYS`), each reported with
what replaced it: gapped panes always draw a glass frame, pane corners follow `widget-corner-radius`,
the pane edge follows the theme's glass, or its outline width without glass,
panes always show the window behind them, and a PiP is glass while `interface-glass` is on and its
pane's color, opaque, under the pane hairline while it is off. They are otherwise ignored.
`shadow-strength`, `widget-shadows`, `widget-selection` and `widget-field` are retired too
(`zz_config::RETIRED_LOOK_KEYS`). A `shadow-strength` line is reported with
`` `shadow-strength` was removed: set the look in Settings > Interface ``, and the
other three with `` `<key>` was removed: the kit sets this per role now ``, since which surfaces
cast shadows, how a selected row shows and how a field draws are role props now (see
[surface roles](/designs/surface-roles.md)). Each is otherwise ignored.
`interface-theme` is retired too (`zz_config::RETIRED_THEME_KEYS`); see Interface themes below.
`experimental-agent-pane` and `agent-working-directory` went with the Agent pane on 2026-10-23,
and `driver-agent`, `driver-model`, `driver-effort` and `driver-hosts` went with the Driver Agent on
2026-10-28 (`zz_config::RETIRED_AGENT_KEYS`). A line with any of them is reported with
`` `<key>` was removed: open a chat from the New agent picker, or flip a terminal running Claude
Code or Codex from its pane header `` and ignored (see [agent panes](/concepts/agent-panes.md)).
`status-show-session`, `status-agents`, `status-badges`, `status-host` and `status-update` are
retired status bar keys (`zz_config::RETIRED_STATUS_KEYS`). `status-show-session` went with the
session menu and is reported with `` `status-show-session` was removed: the status bar no longer has
a session menu; switch sessions from the sidebar ``. `status-agents` went with the agent activity
item on 2026-10-24 and is reported with `` `status-agents` was removed: the status bar no longer
shows agent activity; the sidebar shows each agent's state ``. The other three went with the Status
bar page on 2026-11-06, when the title bar dropped its window badges, host and update items:
`status-badges` points to the sidebar's bell marks, `status-host` to the sidebar, and `status-update` to
Settings > About. Each is otherwise ignored.
`animations`, `animation-speed` and `window-background-opacity` went on 2026-11-06
(`zz_config::RETIRED_MOTION_KEYS`): the first two are reported with `` `<key>` was removed: set
`motion` to off, default or fast `` and the third with `` `window-background-opacity` was removed: set
how see-through the window is with `interface-glass` ``.

The title bar holds the sidebar toggle and the window pills with their pane decks, with nothing
to set. The clock never shows seconds. `AppShell` aligns its first wake to
the next minute boundary, then requests one redraw per minute while the bar and clock are visible;
`time-date` renders `%H:%M · %b %d`.

`widget-corner-radius`, `control-shape`, `interface-glass`, `chrome-accent`, `chrome-tint`,
`chrome-tint-hue`, `ui-font-family`, `ui-mono-font-family`, `chrome-contrast`, and `theme-mode`
land on the **zz-ui theme** rather than being read per-frame by a renderer:
`zz::theme` activates `config::shown_theme`, the platform's theme file with the keys the file sets
applied over it (see Interface themes for the order), which puts its colors and knobs on the `Theme` global,
and `apply_zz_overrides` then sets the contrast, drops the glass and its look while the system reduces transparency, and sets the window opacity and the backdrop's darkest and brightest values from the mode's glass look. Every surface already resolves
its role from there, so no component is plumbed individually. Because the theme and the overrides
are reapplied on every theme rebuild, they survive a light/dark switch, the same treatment
`mono_font_family` gets. A gapped pane's corner follows the same radius: `config::pane_content_radii` starts from
`Theme::pane_radius()`, the theme radius up to 10.24 (64% on the slider), then a parabola that
leaves the radius at its slope and lands flat on 16 at 100%, whatever the control shape (11.8 at
the default 12, 0 for a square theme), and resolves it per corner.

Every rounded surface is a **squircle** . iOS-style continuous corners, set as the ala window
default (`penna::CORNER_SMOOTHING`, 4, from the kit's `foundation/theme.rs`) by `penna::Root::new`
on each window and again by zz's theme refresh, rather than plumbed per widget. Neither a key nor
a theme changes it: the retired `corner-shape` key offered a `round` circular arc alongside it and
produced two nearly identical windows, and themes lost their
`corner-smoothing` key on 2026-10-13.
Shapes whose radius reaches half their shorter side . switch tracks and thumbs,
status dots, avatars, scrollbar thumbs . stay circular regardless: a pill or a circle is a shape in
its own right, and smoothing one turns it into a lozenge. The squircle therefore reads on rounded
rectangles, and the larger `widget-corner-radius` is, the more visible it gets (the corner departs
from a circular arc by roughly a fifth of the radius, so it is sub-pixel below about 4px).

`ui-font-family` and `ui-mono-font-family` live in `UiFontConfig`, and `interface-glass` in `GlassConfig`, both apart from
`AppConfig` so it can remain `Copy`. Interface settings lists font families from ala's
cross-platform text system. Selecting System default writes `.SystemUIFont`; Reset removes the
current override. With no line, the font is `Appearance::ui_font()`: the platform theme file's
`face`, which no file sets, so the system font. The 500 ms config watcher reapplies the family
to `Theme.font_family` and redraws open windows. Quoted names preserve spaces, Unicode, quotes, and
backslashes. Pasting a theme file writes its `face` here, or removes the key when it has none.
Terminal, editor, and code-block fonts use their existing monospace settings.

`browser-element-selector-hotkey` lives in a separate `BrowserConfig` ala global so `AppConfig`
can remain `Copy`. The parser canonicalizes ala keystroke syntax and rejects keys without a
non-Shift modifier; an invalid present line keeps the built-in value but retains `Override`
provenance so Settings can still expose Reset. Browser key installation observes that global, so a
watched config edit applies without restarting.

`browser-search-provider` shares that global, and is read at submit time rather than cached per
pane, so an edit reaches every open Browser pane on the next Enter. The engine set is
`ala_cef::SearchProvider`, which owns the query endpoints; the address bar's decision between
navigating and searching is `ala_cef::resolve_address` (see
[input translation](/browser/input-translation.md)).

`browser-remote-debugging-port` shares `BrowserConfig`. The GUI passes it to CEF before the first
browser operation initializes the runtime. An invalid file value produces a config diagnostic and
keeps the preceding/default value; an invalid environment override prevents CEF startup. See
[Agent access through CDP](/browser/agent-cdp.md) for attach commands and local access risks.

`motion = off` sets ala's reduced-motion state. ala transitions, spinners, switches,
dialogs, notifications, and animated UI images settle on a static frame; the custom scrollbar holds
at full opacity and then disappears without a fade. iOS also honors UIKit Reduce Motion, even when
the file sets another motion. Cursor blinking and Chromium page animation are content behavior and
keep their own controls. Desktop ala does not currently publish the operating system's motion
preference, so macOS, Linux, and Windows follow this key directly.

## Interface themes

The look is penna's one look with knobs over it; [interface themes](/designs/interface-themes.md)
has the theme file format. It starts from the platform's theme file
(`penna::foundation::interface_theme::platform_theme`: `macos` on macOS, `default` elsewhere), and
each look key above (`chrome-accent`, `chrome-tint`, `chrome-tint-hue`, `widget-corner-radius`,
`control-shape`, `interface-glass`, `ui-font-family`; `zz_config::ConfigKey::LOOK`) sets one knob
over it. A key with no line follows the platform. `config::appearance` builds a
`penna::foundation::interface_theme::Appearance` from the file, and `config::shown_theme` is its
`shown_theme()`: `ThemeOverrides::apply_to` sets the radius, replaces the tint with the accent (a
named accent takes its light or its dark color, `zz_ui::interface_theme::accent_color`; `ink` draws
primary fills in the foreground), sets the neutral's amount and hue, makes controls pills or
rounded, and sets the glass.

There are no presets to pick and no saved themes, and no Settings row reads or writes a theme
file. The settings handler still takes penna's two theme file changes: pasting one
(`AppearanceChange::Import`: zz writes
every look key from `ThemeOverrides::from_theme` and the file's `face` in one edit,
`config::look_edits` through `zz_config::set_config_keys`) and resets the look
(`AppearanceChange::ResetLook`: every look key removed from the file, duplicates included).
`interface-theme` is retired (`zz_config::RETIRED_THEME_KEYS`): a line with it is reported with
`` `interface-theme` was removed: there are no theme presets now; set the look in Settings >
Interface `` and ignored. zz no longer reads, stamps or
watches a `themes/` folder beside the config; files left there are ignored.

Run `just storybook macos` on macOS 27 with its SDK to compare the macOS theme file against native
AppKit glass buttons and tabs on the same backdrop. The zz-ui preview's Reload theme button
reads `libs/penna/themes/macos.json` again (or the one theme file passed as the example's first argument); both previews have a light/dark switch and show their window activity.
In the native reference, Cmd+Shift+A deactivates the app and Cmd+Shift+R activates it.
Click a circular toolbar button in the zz-ui preview to activate that window.

A glass material (`interface-glass` as JSON, or a theme file's `glass`) names the
`GlassMaterial` fields it changes from liquid glass in that mode, for both modes or under `light` and `dark`.
`light_directionality`, `inactive_flattening` and `edge_spread` run from 0 to 1. `edge_spread`
evens the contour out around the edge: 0 keeps it darkest along the light axis, 1 makes it a
hairline as dark all the way around, which is what a light glass needs to stand off a light
backdrop. Directionality moves the glint toward the light and the contour shadow away from it.
Inactive flattening reduces the rim, refraction, and color boost when a native window loses focus,
keeping the tint, blur, and contour. Every material's tint rises until text on it reads at 4.5:1.

A surface whose role takes glass draws the look's glass tinted with its own fill, with the glass
rim in place of the outline. On a glass ground it paints a tint instead, and with `interface-glass
= none` a solid fill.

A test in `crates/zz-ui/src/interface_theme.rs` (and its twin in `libs/penna`) holds every theme
file to contrast floors in both modes: foreground at least 7:1 on the background and on a popover,
the accent and status colors at least 3:1, and text on the accent at least 3:1. penna's
`every_accent_reads_in_every_theme_at_any_neutral` holds each of the eight named accents to the same
floors on every file at any tint, with the tint hue every 30 degrees.

Chrome colors come from the theme and the keys above, and from the terminal's colors only through
`chrome-terminal-colors`, while the terminal's theme is light in light mode or dark in dark mode.

`chrome-contrast` adjusts derived surfaces, muted text, fills, and edges without storing colors.
The default is 1.0; desktop and browser Settings show it as 0%. Reset removes the override.
The native macOS client keeps the default derivation strengths and does not show this control.

zz has no frame-rate overlay. The `show-fps` key was removed together with ala's `profiler`
feature, which it needed; a leftover `show-fps` line, like the older `show-app-fps` and
`show-browser-fps`, only logs the normal `unsupported key` warning and changes nothing.

`quit-daemon-on-exit` changes what app quit sends: `true` issues `kill-server`, `false` (the default)
issues `detach()`. Leaving it off is what preserves the multiplexer guarantee that live sessions
outlive the window; see [session persistence](/concepts/session-persistence.md) for the daemon's own
exit rule.

The editor pane is additionally **compiled out by default**: the `editor-pane` cargo feature on the
app crate (mirrored by `editor` on `zz-ui`) gates the implementation behind a facade module
(`crates/zz/src/editor/mod.rs`) whose stub keeps every call site compiling. In a build without the
feature the matching config flag below reads false regardless of the file, the Settings switch and
Editor page disappear, and a pane handed over by a featureful daemon renders a labelled placeholder.
Dev builds opt back in with `ZZ_CARGO_FEATURES=editor-pane just run mac` (or `--features` on cargo
directly). The `agent-pane` cargo feature keeps the same facade (`crates/zz/src/agent/mod.rs`) for
the [agent panes](/concepts/agent-panes.md) and is in the default set; it has no config flag.

`experimental-editor-pane` is a **hard capability gate** with two consumers of the same config
entry. Client-side it gates the pane picker's Editor row (and the `e` hotkey) in
`pane::picker::choices` plus the palette's `select-pane-kind` completions. Daemon-side the entry is
forwarded through the config-override push (it is also a `MuxOptionKey`), and the mux engine
rejects `select-pane-kind … editor` while the flag is off. To enable it again, run
`set-option -g experimental-editor-pane on` in `mux.conf` or at runtime. This changes the daemon
gate directly; the picker row follows the `zz/config` entry, not the mux option. Panes that already
exist keep rendering on reattach when the switch is off; flipping it never destroys pane state.

The main window always asks for client-side decorations: zz draws its own minimize, maximize and
close on Linux and Windows. `use-system-titlebar` went on 2026-11-08
(`zz_config::RETIRED_WINDOW_KEYS`); a line with it is reported with
`` `use-system-titlebar` was removed: zz draws its own window controls `` and skipped. A
compositor that still insists on server-side decorations gets its own titlebar, ala reports
`Decorations::Server`, and zz draws no controls.

While glass is on, zz requests `WindowBackgroundAppearance::Blurred` for both newly created and
already-open windows (`theme::chrome_blur`). zz reads the system's reduced transparency at start and again whenever its window becomes
active (`config::follow_system_transparency`): `NSWorkspace`'s `accessibilityDisplayShouldReduceTransparency` on macOS and
`EnableTransparency` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` on Windows; Linux has none. `window/background.rs` maps that request to a transparent ala surface plus
a backdrop layer under ala's view on macOS (the glass look's blur, saturation, curve and brightness
cap; the WindowServer's background-blur radius, 160, where that layer is missing), retains ala's optional KDE
`org_kde_kwin_blur_manager` protocol on Wayland, and sets KDE's
`_KDE_NET_WM_BLUR_BEHIND_REGION` property on X11. The X11 region follows window size, scale, and
exposed rounded corners so resize and tiling do not leave a stale rectangular effect.

**The blur is gated on the compositor actually blurring.** A translucent chrome over an unblurred
desktop is just a hole, so `window/background.rs` probes once at startup
(`detect_compositor_support`): macOS and Windows always pass; a Wayland session passes when the
registry advertises `org_kde_kwin_blur_manager` (KWin ≤ 6.3) or its staging successor
`ext_background_effect_manager_v1` (Hyprland, KWin master, niri); an X11 session passes when the
root window's property list announces `_KDE_NET_WM_BLUR_BEHIND_REGION` . KWin's blur effect posts
and retracts that atom with the effect itself, the same signal
`KWindowEffects::isEffectAvailable` reads, so it means "blur is on", not just "KWin is running".
Everywhere else `theme::chrome_blur` stays false and the toggle is an honest no-op . opaque
chrome, `Opaque`/`Transparent` native request . logged at startup. picom ignores the KDE blur
property entirely (open upstream feature request), so picom setups stay in the opaque fallback by
design.

A PiP follows Glass like a menu: with glass on it is the theme's material tinted in its pane's
color at the material's own opacity (`zz_ui::pip::pip_material`), and with glass off it is that
color, opaque, under the `divider` hairline gapped panes take. Panes themselves paint no
fill: the window shows through them, and a browser page, blank, loading and error backgrounds
stay opaque.

With gaps the pane frame owns the single fill (`PaneLook::frames` in the
`zz_ui::pane_look::PaneLook` global, set from `pane-gaps`). Content roots paint nothing under the
frame, and `PaneLook::frame(theme, plane, active)` gives the frame the pane's own color over the
window plane. Under a theme with glass, a pane whose role takes glass draws the frame in the theme's
glass with blur, refraction and dispersion off: tiled panes sit on the flat app background, so
there is nothing to blur. On an opaque window the tint is solid and the glass draws in place with
no backdrop copy, costing about what a bordered fill does. The glass edge (glint, fresnel, thin
contour) is the frame, drawn as the theme's glass defines it: its own glint width, specular,
fresnel, edge and noise, so a glass theme with no outline keeps its rim light on panes. No color marks
the active pane: its tint is washed toward white and the other panes' tints are veiled toward
black, their glint dimmed to 70%, all scaled by `pane-glow-strength` (see UI conventions). Under a
theme without glass (Default) the frame is a
plain fill at the pane opacity with an even line over the content, the outline width wide, in the
divider color, dimmer on the other panes (`PaneLook::edge_color`), because a lit glass edge there drew a bright top
and a dark band along the bottom that differed from pane to pane; with no outlines there is no
line. A line the glass glint's width stands in for the glass edge where the window cannot sample
its backdrop (WebGL, DirectX), since glass paints only its fill there. A content root that paints
an opaque fill under a frame hides the frame's color and the inside of its edge, which is why a browser shows its opaque page fill only behind a live page.
The iPad browser's page is a native web view laid over everything ala draws, so it is mounted
inside the frame by the pane edge width on its sides and bottom, with its corners eased by the same
amount, and the pane's root paints the pane background like every other pane.
Floating panes take the theme's full glass like every popover. Settings previews that are not
inside a frame use `zz_ui::pane_look::pane_color`.

An agent pane's chat uses the chosen opacity. The composer is a field shell (`field_chrome` drawn in
`FieldLook::Composer`), so at rest it draws what the theme gives that look, a soft wash
by default, rather than an opaque raised card.
The footer inherits the main pane background and adds no fill of its own.
The composer overlaps the transcript by 12 px.
The transcript clips before the footer. Prefix cards disable the overlap. Text keeps its own opacity.

A terminal first blends its Ghostty background color with the theme base using
`background-opacity`, then applies the shared pane factor to that result. Ghostty's
`background-blur` stays ignored; Glass owns the native compositor request.

`app_shell_surface` paints one continuous app background beneath the sidebar, titlebar,
Settings, and pane layout. It uses `theme::chrome_background`: the theme background at `Theme::window_opacity`, which
`apply_zz_overrides` sets from the mode's glass look (on macOS its chrome opacity, 0.82 in light and 0.65 in dark; elsewhere 0.97) while the window is blurred and to 1 otherwise. Margins, split gaps, and rounded pane corners reveal
this same root surface, or with pane gaps the sidebar-colored frame over it. Panes and Settings
sit on the content plate (`Theme::content_plate()`): with gaps each is a rounded plate in the frame
(`zz_ui::pane::content_pane` for Settings, the New chat and new-session screens and the connection state),
and without gaps the pane column paints the plate on a see-through window and nothing on an opaque
one, where the plate is the root fill itself. The fixed sidebar inherits the root fill; the titlebar
sits in the pane column and takes what the column paints.
The slideover sidebar keeps its own background because it overlays pane content.

Linux window geometry starts from `window-corner-radius` (default 13.5px, targeting the macOS 27
window radius . ~13.5pt tangent-circle equivalent, measured from a macOS 27 screenshot against the
12pt traffic lights) and `frame_content_corner_radius(cx)` derives the inner curve by subtracting
the frame's shared 1px border width (12.5px at the default). The key exists because Linux desktops
agree on no radius and expose no protocol to query one: users match their compositor's rounding
(Hyprland `decoration:rounding`, KWin effects) or set `0` for square, by hand. It only draws where
the frame is app-owned . tiled/maximized Linux edges remain square, and `WindowCorners::for_window`
returns no app-owned corners under server-side decorations or on macOS and Windows, where native
window shaping remains authoritative. `frame-content-corner-radius` and `pane-content-corner-radius`
are still derived, not configuration keys; leaving one in the file produces the normal
unsupported-key diagnostic.

On Wayland compositors using `ext-background-effect-v1`, ala sends that same exposed-corner mask as
the effect region. KWin changes pixels even where the client surface is transparent, so the region
does not extend into the client-side shadow inset. Transparent shadow and corner pixels retain the
compositor's unmodified background; tiled and square corners remain square.

`pane-gaps = false` keeps the regular workspace flush: margin, corner radius, and edge width
resolve to `0`, and split nodes retain their neutral 1px hairline. The active pane colors only its
half of each adjacent separator with a washed foreground accent; nested T-junctions limit the
segment to the edge the pane touches. With `pane-gaps = true` the margin applies exactly as
configured, defaults included, and the corners and edge come from the theme: a 6px margin and,
under Default, a radius of about 13.6px, concentric with the controls in the pane header, so a maximized
grid reads as one rounded surface. The strip along the window's top beside the panes (the macOS
traffic lights and the sidebar's buttons) is a pane header tall (34px) plus the margin above and
below it (`Theme::title_bar_height()`), so its buttons sit on the pane headers' center line; flush
panes leave it one header tall. With the sidebar hidden the status bar lies across the top of the
panes instead, and it is a pane header plus one margin (`Theme::title_bar_over_panes_height()`, 40px
at the default 6px, as tall as with flush panes), since the panes' own top margin finishes the gap
under it; the traffic lights glide to its center line as the sidebar slides away and back. The sidebar's footer mirrors it: settings and the view switch are
centered the margin plus half a header in from the window's bottom and leading edges, like the
traffic lights from its top. The toggle and the view switch center on the column the sidebar rows' actions
(+, …, ×) center on, past the scrollbar's gutter (see [UI conventions](/configuration/ui-conventions.md)). There is
no second tier of gapped defaults: the number Settings shows is the number the panes use, which is
what lets the disabled Frame row stay honest. Gapped panes draw their frame in glass, as above,
and keep the contact shadow. This treatment is built in; the retired `pane-shadow` key produces the
normal unsupported-key diagnostic.
Split divider visuals disappear in this mode because the gap is the separator, but the unchanged
16px drag target still resizes the split.

`pane-inactive-opacity` is independent of pane gaps. The default `0.7` preserves the previous 30%
fade. Terminal panes apply it to glyphs and decorations while keeping their background unchanged.
Browser panes apply it to the toolbar while keeping Chromium page pixels unchanged. Other native
pane surfaces fade toward the opaque window background. Setting it to `1` removes every inactive
pane dimming treatment.

The pane frame renders app-side and therefore applies on every platform. While the effective margin
is zero, window-exposed pane corners keep the larger of the pane radius and derived frame-content
curve; a nonzero margin detaches panes from the frame, so the pane radius alone shapes all four
corners. The margin is single-counted everywhere: the layout root insets the window edge by one
margin and each split's slot *is* the one inter-pane gap, so `pane-margin = 2` yields exactly 2px
between panes and 2px at the frame; gaps never sum to double between neighbors. The gap paints the
window's base plane (the theme's `background`) from three places: the frame inset is a border band on the
layout root rather than padding, the split slot is the divider's own fill, and each rounded pane
fills the four wedges its arc leaves bare inside its own box with a band clipped to that box. Pane
roots cover the native backdrop inside those bounds. In browser panes the toolbar owns the pane's
top arc: the Chromium surface and the overlays that replace it (error panel, empty state) keep
square top corners and only follow the pane curve at the bottom.

## Agent keys

No agent key is app-owned any more: `agent-working-directory` went with the Agent pane (see the
retired keys above). The three agent keys are [mux options](#daemon-owned-mux-option-keys). They
configure the daemon's native Claude Code and Codex drivers and its ACP runtime. Since the Agent
pane was removed on 2026-10-23 nothing opens those outside tests. The
[agent panes](/concepts/agent-panes.md) reads the files the CLI writes and ignores them.

The command values may be unquoted, wrapped in matching single/double quotes, or supplied as
raw JSON when explicit arguments/environment variables are needed. zz does not log the configured
command, because a JSON environment map may contain credentials. None of the three has a row in
the Settings dialog.

The daemon captures and caches the user's login-shell `PATH` before the first driver launch,
merges in inherited entries, and injects the result only when the parsed command has no explicit
`PATH` environment entry. The capture is bounded and falls back to `/usr/libexec/path_helper` on
macOS, so a launch-agent or Finder-started daemon can resolve Homebrew and user-local executables
while raw JSON or a leading
`PATH=...` assignment remains authoritative.

## Daemon-owned appearance keys

The following keys use the same spelling and value grammar as zz-terminal's supported Ghostty
subset. The client only recognizes and transports them; it does not validate their values.

The set is `AppearanceConfigKey::ALL` in `crates/zz-terminal/src/appearance.rs`; read it there
rather than trusting a count here.

| Group | Keys |
| --- | --- |
| Theme | `theme` |
| Colors | `background`, `foreground`, `cursor-color`, `selection-foreground`, `selection-background`, `palette` |
| Fonts and geometry | `font-family` plus `-bold` / `-italic` / `-bold-italic` stacks, `font-size`, `font-feature`, `font-synthetic-style`, `font-thicken`, `font-thicken-strength` (macOS only), `adjust-cell-height`, `window-padding-x`, `window-padding-y` |
| Policy | `minimum-contrast`, `background-opacity`, `cursor-style`, `cursor-style-blink` |
| zz extensions | `zz-font-weight`, `zz-cursor-blink-interval-ms`, `zz-search-match-color`, `zz-search-current-color`, `zz-link-color`, `zz-copy-cursor-color`, `zz-rounded-selection` |

On the iPhone the terminal draws in the interface theme's colors instead of these: the theme's
text on its background, a block cursor in the text color, and a 16-color palette built from the
theme (`zz_ui::terminal::theme_terminal_colors`: black and white between the background and the
text, red and green from the theme's danger and success, blue from the accent when it is blue,
yellow, magenta and cyan at their lightness, brights a step further from the background). The
daemon still resolves cells with the host's colors but tags where each came from (`ColourClass`),
so the client repaints default and palette 0-15 colors (`LocalColors::recolor`) and leaves 256-color
and RGB picks alone; an inverted cell, whose source isn't tagged, is matched by value.

`minimum-contrast` (a WCAG ratio from 1 to 21, default 4.5; Settings › Appearance's Terminal text
contrast writes 4.5 or 1) keeps terminal text readable whatever the theme or the application picked: any text color below the floor against its cell background gets
lighter or darker until it reaches it, keeping its hue. Faint text is dimmed against its cell background
before correction, so dim placeholders and input text retain the same contrast floor. Lower values
such as 2 preserve more application colors; 1 turns correction off. Box drawing, block and Powerline
glyphs skip correction.

File order is preserved, including repeated `palette`, face-specific font-family, and `font-feature`
entries. `config-file` is not part of the native override surface.

`theme` is transported like every other daemon-owned key. The daemon resolves it first, ahead
of the rest of the override set: it looks each name up in `themes/` beside `zz/config`, then in the
standard Ghostty theme directories (`$XDG_CONFIG_HOME/ghostty/themes`, `~/.config/ghostty/themes`,
the macOS Application Support directory, `$XDG_DATA_DIRS/ghostty/themes`, and the installed Ghostty
resources), loads that file, and tags every key it sets `theme-file`. The remaining override entries
then apply on top, so an explicit `background = …` beside a `theme` line wins. A name that cannot be
resolved produces a daemon diagnostic and leaves the rest of the set applied.

zz's mode picks the theme (`theme-mode`, else the system's). `light:A,dark:B` names one for each
mode, and either side may be left out (`dark:B`), so that mode draws zz's built-in colors. A lone
`theme = NAME` counts as light or dark by its background (lighter than its foreground is light,
`TerminalAppearance::background_scheme`) and paints only that mode; the other one draws the built-in
colors. Settings › Appearance writes the key with Light theme and Dark theme, two searchable lists of
the installed themes that open on themes of their own kind (Light or Dark, with All themes beside
it); picking either one writes the pair and removes custom terminal colors, and Built-in leaves
that side out.

The `zz-*` extension keys are **zz/config-native** daemon-owned settings. The Ghostty parser also
accepts their spellings, which is what makes a Ghostty file importable, but they are zz's keys.

## Daemon-owned mux option keys

The seventeen mux keys are also transported raw and in file order. The daemon turns each entry into
a global `set-option`, so repeated keys retain normal last-writer behavior and invalid entries
produce a daemon diagnostic without blocking later entries. `mouse` and `escape-time` joined the
config-writable roster on 2026-08-21 when Wave B2/B3 took ownership of their behavior, and
`prefix2` — the last publication-only key — joined the same day when Wave C run 2 took ownership of
second-prefix arming and `send-prefix -2`; a reload reapplies them exactly like the existing keys
(`from_config_key` routes the entry, the daemon replays it as a global `set-option` with
`Override` provenance, and unset restores the underlay).

| Key | Built-in default | Accepted value / effect |
| --- | --- | --- |
| `prefix` | `C-b` | tmux-style prefix key name; canonicalization is owned by `KeyTables::set_prefix` |
| `prefix2` | `None` | optional second prefix key (or `None`); arms the prefix table beside `prefix` and feeds `send-prefix -2`; canonicalization is owned by `KeyTables::set_prefix2` |
| `mode-keys` | `emacs` | `vi` or `emacs`; retargets native copy-mode tables |
| `history-limit` | `10000` | integer `0..=1000000`; inherited by newly created panes |
| `word-separators` | tmux punctuation set | one-line string up to 8 KiB; updates live terminal word selection |
| `copy-command` | empty | one-line command up to 8 KiB used by copy-pipe fallbacks |
| `set-clipboard` | `external` | `on`, `external`, or `off` |
| `buffer-limit` | `50` | integer `1..=2147483647`; updates automatic paste-buffer eviction |
| `synchronize-panes` | `off` | `on` or `off`; controls global synchronized input inheritance |
| `experimental-editor-pane` | `off` | flag value; gates `select-pane-kind editor` in the engine |
| `history-trickle` | `2000` | integer `0..=10000`; background scrollback backfill budget. `0` disables trickle and leaves scroll-driven prefetch intact |
| `agent-command` | `codex` | Nonempty command string or an `AcpAgentConfig` JSON object (`{"command", "args", "env"}`), up to 4 KiB; what the daemon's Codex driver runs |
| `agent-claude-code-command` | `claude` | Same, for the Claude Code driver |
| `agent-auto-approve` | `reads` | `off`, `reads` or `all` (the flag spellings `on`/`yes`/`true`/`1` still parse as `all`, `off`/`no`/`false`/`0` as `off`); `reads` answers read-only tool kinds (`read`, `search`, `fetch`, `think`) and, in the Claude Code and Codex drivers, shell commands made of `zz` calls daemon-side, and sends `execute`, `edit`, `delete`, `move`, an absent kind and any unrecognised kind to the permission wizard; when the tier answers, a kinded `session/request_permission` is answered daemon-side with the agent's preferred allow option (`allow_always`, else `allow_once`) and the tool call is still published to the stream. A request with no allow option always falls through to the permission wizard |
| `mouse` | `on` (the pin builds with `-DTMUX_MOUSE=1`) | flag value; session-effective per client on the wire. zz-tui gates its outer-terminal mouse modes on it and the daemon rejects mouse input from terminal-surface clients when off; the GUI's native mouse is ungated (decision 6) |
| `escape-time` | `10` | integer milliseconds; zz-tui's escape-sequence fold timeout (`0` clamps to 1 like the pin's `tty_keys_next`) |

The three agent keys became mux options when the Agent pane's ACP runtime moved into the daemon at
wire v53: the daemon spawns the driver, so the daemon owns what it spawns. They apply to the native
drivers only, not to the [agent panes](/concepts/agent-panes.md).
They are still written in `zz/config` like any other key; the client's parser recognizes them and
partitions them into the ordered daemon set, exactly as it does for the appearance and other mux
keys. Changing one reconfigures the runtime without restarting the children already running.

# Parsing contract

- Each nonblank entry is `key = value`; surrounding whitespace is ignored.
- Full-line `#` comments are accepted. Outside single/double quotes, an inline `#` starts a comment
  only after a nonempty value and when immediately preceded by whitespace. Leading and adjacent
  hashes remain data, so `background = #112233`, the normal tmux punctuation set, and quoted hashes
  in copy commands survive transport unchanged. This bounded rule intentionally leaves an unquoted
  whitespace-prefixed hash ambiguous.
- Client-local booleans and pane geometry are validated in the client; later valid entries
  replace earlier values and invalid entries warn while retaining the preceding/default value.
- Daemon-owned values are retained as trimmed raw strings in file order and parsed only by the
  daemon-side appearance loader or mux command engine. Supported appearance and mux keys do not
  produce client warnings.
- `experimental-editor-pane` is the one dual-consumer exception: it parses as a client-local
  boolean **and** forwards raw to the daemon, so the picker gate and the engine gate read the same
  entry.
- Unknown keys and malformed lines warn and are ignored.

# Provenance

Every client-local knob resolves to `(effective value, provenance)`. Provenance is `Default` when the
loaded file contains no assignment for that key and `Override` when the key is present. Presence is
recorded even when its value is invalid; the diagnostic still follows the warn-and-keep rule, so an
invalid override keeps the preceding/default effective value while still counting as an override.
The look keys follow the same rule: `Default` provenance means the platform's knob, so their
default values only show through an invalid line. `chrome-accent`'s diagnostic lists `theme`,
`ink`, the eight names and `#rrggbb`; `interface-glass` keeps its value as written, and a material
penna cannot read is logged and draws the platform's glass.

Appearance provenance is resolved by the daemon and arrives with `ServerHello` and
`AppearanceChanged` as a complete per-key map. `AppearanceSource` has four tiers . `default`,
`theme-file`, `ghostty`, and `override`. Only three of them can reach a client: the daemon resolves
appearance from built-in defaults plus the override set, so it emits `default`, `theme-file` (keys
a `theme = NAME` override pulled in), and `override`. The `ghostty` tier belongs to the client-side
import loader, which reads a donor Ghostty config in-process and never publishes a provenance map.
Palette provenance is one entry for the whole palette, not 256 entries. Settings no longer renders
per-palette-entry provenance; Terminal renders the palette as one inherited/overridden group and
shows per-key provenance for the rest of its structured appearance controls.

Mux option state is also daemon-resolved. `ServerHello.mux_options` and `MuxOptionsChanged` carry a
complete ordered map of the ten effective display strings. Each value has the last-writer tier
`default`, `tmux-config`, `override`, or `runtime-command`. The `tmux-config` wire tier kept its name
for compatibility and means "set by sourced tmux or zz mux configuration". Multiplexer reads the saved `zz/mux.conf` for its option controls.

# Comment-preserving writer

`config/mod.rs` exposes typed set/remove operations for the client-local Settings controls through
the same bounded writer. They use parser-compatible key matching and edit the **last** occurrence,
matching later-entry precedence:

- set replaces only the value bytes, preserving the assignment's key spelling, whitespace, inline
  `#` comment, line ending, and every other file byte;
- remove deletes that one complete line, so an earlier duplicate becomes effective;
- set appends a canonical `key = value` line when the key is absent;
- reads and the resulting edited file retain the same UTF-8 and 64 KiB bounds as normal loading;
- writes use a uniquely created temporary file in the config file's directory, flush it, and rename
  it over the destination atomically; a symlinked config (dotfiles) is resolved first, so the
  rename lands on the link's target and the link survives.

Key matching and value replacement use the same quote-aware, whitespace-prefixed `#` boundary as
the parser. A hash inside a color, palette assignment, word-separator set, or quoted command is never
mistaken for the line's preserved trailing comment.

# Import

`crates/zz-config/src/import.rs` owns the one-shot import. `import_ghostty_config_from(path, scheme)`
reads a chosen donor (`import_ghostty_config` delegates with the discovered default), parses it client-side with the zz-terminal loader, and serializes every key the donor set, directly
(`Ghostty` provenance), into concrete `zz/config` values. A `theme` directive comes across as the
same `theme` line, with both modes, and the theme files it names are copied into `themes/` beside
`zz/config` (`copy_theme_files`), so the import keeps working once Ghostty is gone; the colors the
theme set are cleared from `zz/config` instead of copied. Everything else is a snapshot.

`config/mod.rs` `import_appearance_values_at` applies those values with **donor-wins replace
semantics**: scalar keys edit the last occurrence in place (preserving spelling, spacing, inline
comments, and every unrelated byte); the cumulative `palette`/`font-family*`/`font-feature` groups
remove every prior occurrence and re-append the group at end of file, led by an empty-value reset so
the result is donor-independent. Palette writes only indices that differ from the built-in palette.
Both the input and result honor the 64 KiB bound, and the write is the normal atomic writer.

The daemon owns `import-tmux-config [path]`. Without a path it uses `discover_tmux_config`.
It copies the donor into a marked import block, comments unsupported source constructs with
`# zz-unsupported:`, and reloads its selected files. The first import prepends the block;
re-import replaces it in place while preserving everything outside it. User-written `source-file`
lines remain intact. `zz/config` mux overrides still apply above the loaded mux values.

# Settings view

`crates/zz/src/config/settings.rs` renders the `WorkspaceRoute::Settings` route in the main window.
`Cmd+,` on macOS or `Ctrl+,` elsewhere opens it. `SettingsSection::ALL` is **eleven pages**, titled
Interface, Status bar, Editor, Panes, Multiplexer, Browser, Terminal, Hosts, Providers, System, About. The sidebar lists
them in that order, in three unlabeled groups (Appearance, Tools, Advanced) set apart by a gap. There is deliberately no "General": the page that name described
had accumulated window chrome, a global widget metric, daemon lifecycle and a debug overlay. The
device/pairing page that sat before Terminal was deleted on 2026-08-01 along with pairing itself;
fleet hosts live in `zz/config` and can be managed from the sidebar or **Settings › Hosts**.

Pages use `SettingsStack` groups of `SettingEntry` rows. `SettingEntry::disabled` dims a row and
blocks interaction, which is how the Frame group shows that `pane-margin` is inert while
`pane-gaps` is off (`config` forces it to `0` there). Pane corners and the pane edge have no rows:
they follow the theme's radius and its glass, or its outline when it has no glass. Layout holds the `pane-gaps` switch alone. Focus holds the
always-live inactive-opacity factor and the selected pane's edge strength, which needs gaps.

| Page | Groups |
| --- | --- |
| Appearance | **Theme** (`theme-mode` as Mode, a System, Light and Dark segmented control, `chrome-accent` as Accent swatches with a custom color picker, `chrome-tint-hue` as Tint and `chrome-tint` as Tint strength) · **Terminal** (a live preview, Light theme and Dark theme, searchable lists of the installed themes that open on their own kind and write `theme = light:A,dark:B`, `chrome-terminal-colors` as Window follows the theme, and `background-opacity` as Background opacity) · **Sidebar** (`program-colors`) · **Contrast** (`chrome-contrast` as Increase contrast, a slider, `minimum-contrast` as Terminal text contrast, a switch for 4.5 or 1) |
| Interface | **Material** (`interface-glass` as Glass, a switch, `control-shape` as Controls, a segmented control, `widget-corner-radius` as Corner radius in percent) · **Text** (`UI font`, Mono font, transient `UI zoom` as Text size, `chrome-density` as Density) · **Behavior** (`pointing-hand` as Pointing hand, a switch, `motion` as Motion, an Off, Default and Fast segmented control) · **Window**, where it has rows (macOS `app-icon` as three icon tiles, Linux `window-corner-radius`) |
| Browser | **Network** (`browser-egress`) · **Agents** (`browser-remote-debugging-port` as an on/off switch that writes `9222`) · **Search** (`browser-search-provider`) · **Shortcuts** (`browser-element-selector-hotkey`) |
| Editor | **Typography** (`editor-font-size`) · **Display** (`editor-line-numbers`, `editor-relative-line-numbers`, `editor-soft-wrap`, `editor-vim-mode`) |
| Panes | **Layout** (`pane-gaps`) · **Focus** (`pane-inactive-opacity`, `pane-glow-strength` as Selected pane edge) · **Frame** (`pane-margin`, disabled without gaps) |
| Hosts | **Machines** (configured hosts, live connection state, Remove) · **Add host** (an inline ssh destination field) |
| Agents | **Coding agents** (Set up, which installs the startup hook `zz tools --install-hooks` writes for Claude Code and Codex) · one group per host (This Mac or the host's name): Claude Code and Codex with the path the host's daemon found, the installed version, the newest release, and Update or Install, which runs the vendor's updater on that host (`claude update`, `codex update`, `brew upgrade --cask` for Homebrew installs, the vendor install script when missing) with its latest output line and any error under the row; an install something else owns (a root-owned npm prefix, mise, Nix, Snap, winget, a system package) gets no Update button but says who owns it and offers its update command to copy; a disconnected host says so; **Check again** asks every connected host. Nothing is written to `zz/config`; the daemon answers the internal `providers` command ([wire protocol](/protocol/wire-protocol.md)) |
| System | **Command palette** (`palette-window-layout` as Window chooser, Tree or Flat, `palette-show-keys` as Show shortcuts) · **Tray** (`tray`, only where the profile has one) · **Notifications** (`notifications`) · **Daemon** (`quit-daemon-on-exit`) · **Experimental** (`palette-vim-mode` as Command palette Vim mode, and `experimental-editor-pane`, present only with its cargo feature). `auto-restart-stale-daemon` is a file key with no Settings row |
| Multiplexer | **Navigation** (`which-key-delay`), **Split panes** (a key field per split), **Options** (Prefix as a key field, Mode keys, Mouse, History limit, Clipboard, Escape time), **Import** (Open, which opens `zz/mux.conf` in the default text editor, and Import from tmux). Edits made outside zz apply when the zz window is focused again |
| Terminal | **Preview** · **Display** (Font family as a searchable list of installed fonts that opens on Monospaced, with All fonts one Tab or click away, Font size, macOS Font smoothing and Smoothing strength, Cursor style, Cursor blink, Padding X/Y); how the terminal looks (themes, window colors, background opacity, text contrast) is on Interface, **Import** (Open, which opens `zz/config` in the default text editor, and Import from Ghostty) |
| About | Centered mark (the Dock render at 88pt), name, tagline and version badge · **Updates** (`check-for-updates`, plus a Latest release row whose description is the update state, such as "zz 0.23.0 is out, you have 0.22.0" or "The check failed: …", with Check now, or Update to X / What's new once a newer release is known; while the app installs it the row says Downloading zz X, 84 of 186 MB (then Verifying, Preparing, Installing) over a progress bar with Cancel during the download, then "zz X is ready to install" with Restart to update, or "The update failed: …" with Try again; when the release states its wire line and the local daemon is attached, the row and the update toast add "The update keeps your sessions running." or "The update needs a daemon restart, which ends 3 sessions."; a Daemon row with Restart daemon appears only while the local daemon runs an older revision than this zz; desktop only) · **Build** (`CARGO_PKG_VERSION`, OS · arch, the short `ZZ_ALA_SOURCE` revision, with a copy button on Version that puts all three on one line) · **Project** (repository, releases, new issue, license) |

A row is its title and its control, with no provenance badge and no Reset button. A short
description sits under the title only where it says something the title and control do not: live
state (a host's address, an agent's install path, the update check), a unit or range, why the row is
off ("Needs pane gaps"), or what a terse title means. A stack's note says why all its rows are off.
Group titles are muted and semibold, and page headings carry only the page title. Pages are at most
760px wide. All chrome colors come from `cx.theme()`
semantic values.

Accent is a row of 18px swatches: Ink, in the text color, the eight named accents in the current mode's color, each named in a tooltip, and a
custom color picker drawn as a bare swatch (`ColorPicker::swatch`). Swatches round with
`Theme::swatch_radius`, half the theme radius, so they are square at radius 0 and rounder as the
radius grows, short of a circle (the adaptive corner cap). The selected swatch wears a
foreground ring concentric with it. The picker opens a saturation and brightness field with a
handle to drag, a hue slider over a rainbow and a hex field with a clear button, in a popover, or
a bottom sheet on touch screens; it keeps the last hue picked while the color is gray or black. Ink or a
name writes `chrome-accent`, clearing the picker removes it, and a color from the picker writes it as `#rrggbb`. Tint is a hue slider over a rainbow track with a dot in the picked hue, writing
`chrome-tint-hue` in whole degrees, and Tint strength a 0 to 1 slider with its percentage beside
it, writing `chrome-tint` to two decimals. Both are the kit's continuous `Slider`
(`zz_ui::slider::Slider`, beside `DiscreteSlider`): drag the thumb, click the track, or step it
with the arrow keys (Shift steps ten, Home and End jump to the ends), and each new value is written
as the thumb moves.

`app-icon`, under Advanced, offers three tiles of the real artwork through `crate::app_icon::icon_preview` . macOS's
own renders of `assets/zz.icon`, committed as `assets/zz-{light,dark}-512.png` . where Automatic
renders the variant `cx.window_appearance()` resolves to right now. The preview is Lanczos-filtered
once from 512px to a cached 96px raster before ala paints it into the 48pt tile, avoiding the
rounded-edge aliasing caused by a large atlas downscale without mipmaps. The card is macOS-only,
since X11 takes its icon from the window options and Wayland from the desktop file. Every tile
picker uses the same selection treatment: a plain foreground border around the selected preview
(transparent border otherwise, so nothing shifts), no fill and no hover state, keeping the artwork
unchanged.

Advanced also carries one non-persisted row: **UI zoom**. It is the same bounded `NumberInput`
the geometry rows use, over the root rem as a percentage of the 16px default. Because it writes nothing to disk it is also the one
field here that applies as it changes . a stepper press zooms on the press, and a typed value zooms
once it is a complete number in range, so a prefix such as `1` of `150` never drags the whole UI to
the minimum. Leaving the field clamps and normalizes what it holds. This setting intentionally does
not enter `zz/config`;
it follows Zed's per-run UI font-size adjustment and leaves browser page zoom and terminal font size
under their existing pane-local controls.

Terminal reads saved appearance entries into its controls and writes edits back through the
bounded, comment-preserving `zz/config` writer. Neither page shows the raw file: each reads it
again when entered and lists its path under the controls, with Reload on Multiplexer. Anything
the rows do not cover is edited in the file itself.

The **Split panes** rows in `crates/zz/src/config/settings/multiplexer.rs` read
`MuxClient::prefix_bindings`, the daemon's effective table after its selected mux files, zz overrides, and runtime
commands. Each direction selects one shortcut, preferring `-` / `|`, then `"` / `%`, then another
binding that splits in that direction. A renamed shortcut stays selected while Settings remains open.
Additional shortcuts stay in the file. Choosing a pane type or pressing Enter in the shortcut
field saves an ordinary `bind-key` override through the same atomic writer and reload request.
Renaming also unbinds the previous key; an occupied destination key is rejected.

`crates/zz/src/config/mux_bindings.rs` preserves unrelated source text and replaces matching overrides
in the trailing generated split-binding group when users change selections again. Simple direction
flags and `-c "#{pane_current_path}"` map to the dropdown only for the matching direction; opposite-direction bindings, other arguments, or command chains show
**Custom** and stay editable in the file. Shortcut changes retain command arguments,
repeat settings, and notes. Pane-type changes use `split-window --kind terminal|picker|browser`; terminal and picker
commands inherit the source terminal directory.
The controls require a connected local session. They check for external file edits
before saving and wait for the daemon to publish the changed binding before accepting another edit.
If the daemon does not confirm within five seconds, Settings refreshes the rows from the effective
bindings, permits another edit, and offers a reload retry. Requesting a reload cancels the earlier
pending shortcut expectation.

Multiplexer's Options rows read the last saved global `set-option` line for each option, or
show its built-in default. Writes replace that line or append one; Reset removes it. The daemon
owns value grammar. Controls require a local connection, and wait for daemon
confirmation with a five-second retry fallback. The Reload button uses the daemon's retained
file selection, including explicit `-f` files.

Terminal's Appearance rows read the saved parsed `zz/config` entries. Scalars replace the last
line; Font family replaces its whole stack with one line. The existing watcher applies these
edits. macOS exposes `font-thicken` as Font smoothing and `font-thicken-strength` as Smoothing
strength (0–255); the strength control is disabled while smoothing is off. Changes update the
terminal preview and live panes through the existing reload path. Both pages offer one
**Import…** button that opens the system file picker and imports the picked file right away, with
no confirmation: tmux through `import-tmux-config`, Ghostty through
`config::import::import_ghostty_config_from`. tmux import requires a local connection.

The first-run prompt (`crates/zz/src/config/import_prompt.rs`) uses marker
`<data-dir>/zz/import-prompted-v2`. It names every discovered donor and explains that zz does not
read those files on its own. Accepting imports the found donors; Cancel imports nothing. Settings
accepts other paths. Existing installs receive this offer once more after the ownership change.

The desktop and the thin clients build the Interface page from the same item list
(`zz_ui::settings::appearance::appearance_page_items`: penna's list with zz's Sidebar group, App
Icon row and window rows added as `ZzRow`s), laid out by
`penna::blocks::appearance::appearance_page`. Both draw penna's rows with `kit_row` from a
`penna::foundation::interface_theme::Appearance`, with the row titles from `AppearancePageItem::title`, and
draw only their `ZzRow`s themselves; the desktop also keeps its own UI font select and UI zoom
field. Each edit comes back as an `AppearanceChange`: the desktop builds its `Appearance` from
`zz/config` and writes each change to its key, and the thin clients keep the `Appearance`
flattened into their preferences, so the file keeps the same keys (see
[interface themes](/designs/interface-themes.md)). Corner radius is a slider like the tint's,
with its value beside it, from 0 to 25 in whole pixels ("10px"); Controls writes `control-shape`
and Glass `interface-glass` (`spelled_glass`: the name, or the material as one line of JSON).
Density writes `chrome-density` and reloads the config at once, so the chrome resizes as you pick.
Each new value is written as the thumb moves, so the look previews live, and each knob follows the
platform until it moves. The Theme file menu's Reset removes every look key and shows the
platform's look again through the normal config watcher.

Structured control callbacks, including Terminal, only write `zz/config`; they never mutate the
ala config global directly. Multiplexer writes `zz/mux.conf` and requests a daemon reload. The
existing 500 ms watcher remains the single `zz/config` apply path, updates effective values and
provenance, and refreshes the open Settings route.

# Examples

```ini
# Blend the terminal color over its opaque pane base.
background-opacity = 0.85

# Set every interface transition to its static state.
motion = off

# The toggle uses an 8px margin, corners concentric with the pane header's controls
# (about 14px under the default look), and a glass frame lit like the look's glass, or with a line as
# wide as its outlines when it has no glass.
pane-gaps = true

# Keep inactive pane content at 70% strength. Set 1 to disable dimming.
pane-inactive-opacity = 0.7

# Optional explicit chrome overrides
pane-margin = 6

# Pill controls and no glass, with a green accent and a slight teal tint.
control-shape = pill
interface-glass = none
chrome-accent = green
chrome-tint = 0.4
chrome-tint-hue = 190

# Off by default . the daemon outliving the app is what preserves sessions.
quit-daemon-on-exit = false

# Opt in to the experimental editor pane; off, no surface (picker, palette,
# CLI) can create one.
experimental-editor-pane = true
```

```ini
# Native appearance override; repeated keys retain order.
background = #1e1e2e
font-family = JetBrains Mono
font-family = Symbols Nerd Font Mono
palette = 4=#89b4fa
```

```ini
# Native mux overrides; the daemon applies these above zz/mux.conf.
prefix = C-a
mode-keys = vi
history-limit = 50000
set-clipboard = external
synchronize-panes = off
```

# Related

- [`zz`](/crates/zz.md) owns configuration loading and all consumers.
- [End-to-end data flow](/architecture/data-flow.md) distinguishes client-local and daemon-owned
  configuration.
- [Split-pane layout](/concepts/split-pane-layout.md) propagates exposed window corners into panes.

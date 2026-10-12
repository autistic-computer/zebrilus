---
type: Configuration
title: Application configuration search paths
description: Platform-aware discovery, bounded parsing, ACP launch settings, settings editing, and one-shot donor import for client-local knobs plus ordered daemon-owned overrides.
resource: crates/zz/src/config/mod.rs
tags:
- configuration
- gpui
- window
- appearance
- mux
timestamp: 2026-10-12T00:00:00Z
---

# Overview

The GUI process loads the first existing `zz/config` file from the user's platform configuration
roots. `crates/zz/src/config/mod.rs` resolves the candidates when `run_app` enters the GPUI application
closure, parses named client-local behavior/layout/theme/browser knobs plus
chrome-color entries into typed
`AppConfig`, `BrowserConfig`, and `UiFontConfig` values, parses the one app-owned ACP key into `AgentConfig`,
collects repeatable `chrome-keybind`/`chrome-unbind` entries for the client-local keymap, and collects the supported
daemon-owned appearance and mux entries as ordered raw `(key, value)` pairs. Client-reserved
`host-<name> = <uri>` entries ([fleet attach](/designs/fleet-attach.md)) are matched before any
of that: validated via `zz_daemon_client::Endpoint::parse`, published through a dedicated `FleetHosts`
global (not `AppConfig`, which stays `Copy`), and never forwarded to any daemon.
Three surfaces write these lines and nothing else: `zz fleet add <name> <ssh-destination>` from the
CLI, the sidebar's final **Add host** row, and the inline form in **Settings › Hosts**. The
two GUI fields share the same `parse_add_host` validation and name the entry after the destination's
host component. The underlying writer replaces an existing `host-<name>` value in place while
preserving every other byte, comments included; the GUI fields reject a duplicate name before they
call it. `zz fleet list` prints name and endpoint; `zz fleet remove <name>` deletes every matching
host line, as does a remote row's **Close host** . which additionally republishes `FleetHosts`
(`config::remove_fleet_host_live`) so the running fleet drops the machine immediately. There is no bootstrap step, no key pinning, and no daemon-side setup . ssh already owns
identity. These local schema entries retain `Default`/`Override` provenance. Daemon-owned value
grammar deliberately stays in the zz-terminal appearance loader or
mux `set-option` engine.

The GUI polls candidate paths every 500ms (path, mtime, and size) and live-applies edits, file
appearance, precedence changes, and deletion. On connect and every poll change it sends the complete
ordered daemon-owned set through `SetConfigOverrides`, including an empty set after the
last override is removed. The daemon never reads `zz/config`; it stores the latest appearance and
mux subsets, layers appearance above built-in defaults (it reads no donor appearance config of its
own; the one external file it opens is the Ghostty theme a `theme` override names, see
[Terminal appearance](/terminal/appearance.md)), and
dispatches mux values through the existing global `set-option` grammar. Appearance overrides replay
after `reload-config` and system color-scheme changes; mux overrides replay after every
`zz/mux.conf` load so a reload cannot revert a GUI choice.

The daemon loads only `zz/mux.conf`, or explicit `-f` files that replace it in argument order.
Reload repeats that selection. It never reads tmux donors implicitly. The first-run offer imports
Ghostty appearance into `zz/config` and tmux commands into a marked `zz/mux.conf` block.
Settings accepts any donor path and keeps structured rows above the editor for the same file.

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
`chrome-unbind = <table>:<key>` removes one. The allowed tables are `ui`, `sidebar`, `browser`, and
`terminal`; action names come from `zz_client::ChromeAction`. Keys use the chrome grammar from
`zz-client`, including `D-` for Command/Super and `S-` for Shift. These directives may appear more
than once, preserve file order, and stay client-side. They are separate from the scalar
knobs below and from the daemon-owned tmux tables in `zz/mux.conf`.

The desktop binds `Cmd+1` through `Cmd+9` on macOS and `Ctrl+1` through `Ctrl+9`
on Linux to `ui:select-window-1` through `ui:select-window-9`. Each number selects
that position in the attached session's displayed window order, including windows
beyond the visible titlebar range. Missing positions do nothing. The workspace
intercepts these shortcuts before pane shortcuts, including browser tab selection.
Dialogs, popups, menus, local palettes, and Settings retain their keyboard handling.
For example, `chrome-keybind = ui:D-2=select-window-4` selects the fourth window
with Cmd+2; `chrome-unbind = ui:D-2` restores the pane's handling of that chord.

## Client-local keys

The client-local schema includes these scalar settings and chrome colors.

| Key | Default | Valid range | Consumer |
| --- | ---: | --- | --- |
| `use-system-titlebar` | `false` | `true` or `false` | On Linux, request a desktop-owned titlebar and window border instead of client-side window decorations |
| `window-corner-radius` | `13.5` | `0..=32` | The app-drawn window frame's corner, visible only under Linux client-side decorations |
| `window-background-blur` | `false` | `true` or `false` | Whether the desktop shows through the window chrome, blurred |
| `animations` | `true` | `true` or `false` | Whether interface transitions, loading indicators, scrollbar fades, and animated UI images move |
| `tray` | `true` | `true` or `false` | Shows a native tray icon while the GUI runs. Closing the window or quitting hides zz when the icon is available, `quit-daemon-on-exit` is false, and the local daemon has sessions; otherwise the usual close or quit behavior applies. Applies live from Settings → System or file edits. Quit and Stop Sessions in the tray menu stops the daemon |
| `quit-daemon-on-exit` | `false` | `true` or `false` | Whether quitting the app stops the daemon even while sessions are live |
| `auto-restart-stale-daemon` | `false` | `true` or `false` | Whether a protocol-mismatched local daemon is terminated and replaced on connect. Off by default because it ends every running session |
| `check-for-updates` | `true` | `true` or `false` | Whether the GUI fetches the GitHub release list ten seconds after launch and daily after that, offering a newer release as a toast. The channel follows the running build (a `-beta.N` version takes prereleases). One anonymous request; nothing else is sent. Release builds only unless `ZZ_UPDATE_CHECK=1`; `ZZ_UPDATE_CHECK=0` silences every build. Surfaced in Settings under About |
| `status-show-session` | `true` | `true` or `false` | Whether the native desktop status bar shows the attached session menu |
| `status-badges` | `true` | `true` or `false` | Whether window entries show bell and activity markers. Window names and pane icon decks remain visible when this is off |
| `status-agents` | `true` | `true` or `false` | Whether the bar shows Agent activity in the attached session |
| `status-host` | `true` | `true` or `false` | Whether the bar shows the host name while attached to a remote host. Local attachment has no host item |
| `status-update` | `true` | `true` or `false` | Whether an available release appears in the bar with its version and an install action. `check-for-updates` separately controls the release check |
| `experimental-agent-pane` | `true` | `true` or `false` | Whether new Agent panes can be created at all . picker row, palette completion, and the daemon's `split-window --kind agent` and `select-pane-kind agent` |
| `experimental-editor-pane` | `false` | `true` or `false` | Whether new Editor panes can be created at all . picker row, palette completion, and the daemon's `select-pane-kind editor` |
| `pane-gaps` | `false` | `true` or `false` | Whether panes use the gapped treatment: margin, radius, a glass frame, and no split dividers |
| `pane-background-opacity` | `0.5` | `0..=1` | Pane and Agent composer background strength; Panes settings display 0–100%. Browser panes apply it only to the toolbar |
| `pane-glow-strength` | `1` | `0..=2` | How brightly the active gapped pane's frame lights its edge in the accent; Panes settings display it as Selected pane edge, 0–200%. `0` lights it like the others |
| `pane-inactive-opacity` | `0.7` | `0..=1` | Retained strength of inactive pane content and chrome; `1` disables dimming |
| `pane-margin` | `6` | `0..=32` | Inset around each pane, on every platform; applies only with `pane-gaps` |
| `pane-corner-radius` | `13.5` | `0..=32` | All four corners of every pane, on every platform; applies only with `pane-gaps` |
| `pane-border-width` | `0.5` | `0..=8` | Width of the glint, the bright line along a gapped pane's glass edge; `0` turns it off. Settings calls it Pane edge width |
| `interface-theme` | `default` | `default`, `macos`, `linear`, or a saved theme's name | The interface theme: a look (corners, outlines, shadows, glass, how rows and primary buttons are filled) and its colors for light and for dark, picked together (see Interface themes). Any other name selects `themes/<name>.json` beside this file. It decides the two keys below unless they are set; picking a theme in Settings removes them |
| `widget-corner-radius` | from the theme (`24`, `10`, `8`) | `0..=25` | Widget corners in pixels through 24; values above 24 enable Full mode for buttons, fields, navigation chips, and picker rows. At 0, menu and tree rows become full-width bands with no gaps |
| `shadow-strength` | from the theme (`1`) | `0..=1` | Multiplier for every shadow; `0` turns them and the control rings off. The theme's `control-shadow` says how much of it controls cast |
| `editor-font-size` | `13` | `8..=32` | Buffer text size in editor panes, in pixels |
| `which-key-delay` | `400` | `0..=2000` | Milliseconds after the prefix, or inside a `switch-client -T` table, before the which-key sheet lists the next keys; `0` turns the sheet off. Read when the timer starts |
| `editor-line-numbers` | `true` | `true` or `false` | Whether editor panes draw the line-number rail |
| `editor-relative-line-numbers` | `true` | `true` or `false` | Number the rail by distance from the cursor line, which keeps its absolute number |
| `editor-soft-wrap` | `true` | `true` or `false` | Wrap long lines at the pane edge instead of scrolling horizontally |
| `editor-vim-mode` | `true` | `true` or `false` | Modal vim editing in editor panes: normal, insert, visual and visual-line modes |
| `browser-element-selector-hotkey` | `cmd-shift-c` on macOS; `ctrl-shift-c` elsewhere | One GPUI keystroke containing Control, Alt, Command/Super, or Function; Shift is optional | Toggle the element selector while a Browser pane owns the Browser key context |
| `browser-remote-debugging-port` | `0` (off) | Bare integer: `0` or `1024..=65535` | Loopback CDP endpoint; `ZZ_BROWSER_REMOTE_DEBUGGING_PORT` overrides this value at CEF startup. Restart the client after changing an initialized runtime. The Browser page's **Agents** switch writes `9222` or removes the key; other ports are file-only |
| `browser-search-provider` | `google` | `google`, `duckduckgo`, `brave` | Where a Browser pane's address bar sends an entry that is not an address |
| `browser-egress` | `true` | `true` or `false` | Whether a Browser pane opened while attached to a remote ssh host routes its traffic through that host; panes already open keep the route they were created with |
| `ui-font-family` | System default | Installed font family, or `.SystemUIFont` | Interface text across desktop windows; applies live and stays separate from terminal and editor fonts |
| `theme-mode` | `system` | `system`, `light`, `dark` | Follow the OS appearance, or pin one mode |
| `app-icon` | `automatic` | `automatic`, `light`, `dark` | Which render of `assets/zz.icon` the macOS Dock tile wears; `automatic` defers to the bundle's compiled icon when packaged (so tinted/clear dock styles work) and follows the OS appearance in bare builds, independently of `theme-mode` |
| `chrome-contrast` | `1.0` | `0.5`–`2.0` | Scales the chrome derivations; out-of-range or invalid values are reported and fall back to `1.0`. Settings shows it as 50–200% in steps of 5. |

`interface-style`, `chrome-preset`, `chrome-preset-light`, `chrome-preset-dark`, `chrome-background`,
`chrome-foreground`, `chrome-accent`, `chrome-success`, `chrome-warning` and `chrome-danger` are
retired (`zz_config::RETIRED_APPEARANCE_KEYS`): a line with one of them is reported, skipped and
hidden from the Terminal page's editor, and picking a theme in Settings removes it. Colors now
come only from the theme. `pane-glass` is retired the same way (`zz_config::RETIRED_PANE_KEYS`):
gapped panes always draw a glass frame, so the line is reported and otherwise ignored.

The status settings project directly into `zz_client::StatusBarSettings` and apply through the
normal watched-config refresh. The clock never shows seconds. `AppShell` aligns its first wake to
the next minute boundary, then requests one redraw per minute while the bar and clock are visible;
`time-date` renders `%H:%M · %b %d`.

`interface-theme`, `widget-corner-radius`, `chrome-contrast`, `shadow-strength`, `pane-background-opacity`, and `theme-mode` land on the **zz-ui theme** rather than being read
per-frame by a renderer: `zz::theme::apply_zz_overrides` pushes them onto the `Theme` global, and
every widget already reads from there, so no component is plumbed individually. Because the theme
and the overrides are reapplied on every theme rebuild, they survive a light/dark switch, the same
treatment `mono_font_family` gets. `widget-corner-radius` is distinct
from `pane-corner-radius`, which is pane *geometry* and is resolved per corner in
`config::pane_content_radii`.

Every rounded surface is a **squircle** . iOS-style continuous corners, set once as the GPUI window
default (`theme::CORNER_SMOOTHING`) rather than plumbed per widget. This is not configurable: the
retired `corner-shape` key offered a `round` circular arc alongside it and produced two nearly
identical windows. Shapes whose radius reaches half their shorter side . switch tracks and thumbs,
status dots, avatars, scrollbar thumbs . stay circular regardless: a pill or a circle is a shape in
its own right, and smoothing one turns it into a lozenge. The squircle therefore reads on rounded
rectangles, and the larger `widget-corner-radius` is, the more visible it gets (the corner departs
from a circular arc by roughly a fifth of the radius, so it is sub-pixel below about 4px).

`ui-font-family` lives in `UiFontConfig`. Interface settings lists font families from GPUI's
cross-platform text system. Selecting System default writes `.SystemUIFont`, which keeps the system
font even under a theme that names its own; Reset removes the current override. With no line, the
select shows the active theme's font when it sets one. The 500 ms config watcher reapplies the family
to `Theme.font_family` and redraws open windows. Quoted names preserve spaces, Unicode, quotes, and
backslashes. Picking or importing a theme that sets `look.font` drops `ui-font-family`, like the
radius and shadow it decides; other theme changes keep the chosen font. Terminal, editor, and
code-block fonts use their existing monospace settings.

`browser-element-selector-hotkey` lives in a separate `BrowserConfig` GPUI global so `AppConfig`
can remain `Copy`. The parser canonicalizes GPUI keystroke syntax and rejects keys without a
non-Shift modifier; an invalid present line keeps the built-in value but retains `Override`
provenance so Settings can still expose Reset. Browser key installation observes that global, so a
watched config edit applies without restarting.

`browser-search-provider` shares that global, and is read at submit time rather than cached per
pane, so an edit reaches every open Browser pane on the next Enter. The engine set is
`zz_browser::SearchProvider`, which owns the query endpoints; the address bar's decision between
navigating and searching is `zz_browser::resolve_address` (see
[input translation](/browser/input-translation.md)).

`browser-remote-debugging-port` shares `BrowserConfig`. The GUI passes it to CEF before the first
browser operation initializes the runtime. An invalid file value produces a config diagnostic and
keeps the preceding/default value; an invalid environment override prevents CEF startup. See
[Agent access through CDP](/browser/agent-cdp.md) for attach commands and local access risks.

`animations = false` sets GPUI's reduced-motion state. GPUI transitions, spinners, switches,
dialogs, notifications, and animated UI images settle on a static frame; the custom scrollbar holds
at full opacity and then disappears without a fade. iOS also honors UIKit Reduce Motion, even when
the file enables animations. Cursor blinking and Chromium page animation are content behavior and
keep their own controls. Desktop GPUI does not currently publish the operating system's motion
preference, so macOS, Linux, and Windows follow this key directly.

## Interface themes

An interface theme is one JSON object (`zz_ui::interface_theme::InterfaceTheme`): a `name`, a `look`
and a `light` and a `dark` color set. The mode (`theme-mode`, or the OS appearance on System) only
picks which color set applies; the look is the same in both.

- `look` is `zz_ui::interface_theme::Look` in kebab-case: `radius`, `corner-smoothing`, `shadow`,
  `control-shadow` (how much of the shadow controls cast; `0` keeps buttons, fields and rows flat
  while popovers keep theirs), `elevation`, `outline`, `outline-width`, `row-inset`, `selection`
  (`accent` or `wash`), `primary` (`foreground` or `accent`), `field` (`soft` or `outline`),
  `density`, `control-fill`, `divider`, `animation-speed`, `font` and `glass`. `glass` is `null`
  for plain surfaces, a preset name (`regular`, `clear`, `frosted`, `bubble`, `smoked`), or an
  object with a `preset` and the fields it changes.
- Each color set holds the six roots (`background`, `foreground`, `accent`, `success`, `warning`,
  `danger`) and two optional colors: `on-accent`, the text on an accent fill (a solid primary
  button in the accent, an accent selection), and `shadow-color`, the ink of every drop shadow,
  whose alpha is a floating surface's shadow at full strength. A color left out falls back to the
  built-in palette for the mode; `on-accent` falls back to black or white by the accent's
  lightness.

Keys a file leaves out take `Look::default()` (the default theme's shape without glass) and the
built-in palette. Every other color the UI paints (panels, hover and pressed fills, muted text,
edges, washes) is derived from the roots at paint time, as `design-system` describes. A key the
file does not know, at the top level, in `look`, in a color set or in a `glass` object, is an
error rather than ignored: the message names it and, when it differs from a real key only in
case, dashes or underscores, names the key that was meant
(`` unknown field `corner_smoothing`, did you mean `corner-smoothing`? ``). Look and color keys are
kebab-case while glass fields keep `GlassMaterial`'s snake_case (`light_color`, `edge_shadow`),
which is the usual slip. Settings Import, the thin clients' Import and the storybook's import show
the same message.

Three themes are compiled in from `crates/zz-ui/themes/*.json`:

- `default`: the zz look (24px corners, opaque glass edge, foreground primary, accent selection)
  on the macOS system colors: window `#ffffff` / `#1e1e1e`, label ink, system blue, and the
  system status colors (the higher-contrast variants in light).
- `macos`: 10px corners, flat controls (`control-shadow` 0) under shadowed frosted popovers,
  inset rows, accent selection and accent primary with white text on it.
- `linear`: 8px plain corners, opaque surfaces with ring-shadow outlines, gray (wash) selection,
  indigo primary, and a cool tinted shadow in light.

A test in `crates/zz-ui/src/interface_theme.rs` holds every built-in to contrast floors in both
modes: foreground at least 7:1 on the background and on a popover, the accent and status colors
at least 3:1, and text on the accent at least 3:1.

Saved themes live one per file in a `themes` folder beside the configuration file
(`~/.config/zz/themes/<name>.json`; zz Dev keeps its own), named by letters, digits, `-` and `_`;
a file named after a built-in id is ignored. `interface-theme = <name>` selects one. The config
poll stamps the folder, so adding, editing or removing a theme applies within half a second. A
selected theme whose file is gone logs a warning and falls back to `default`. A selected theme
whose file stops loading (bad JSON, an unknown key, a bad value) leaves the screen alone: the
reload keeps the theme as it last loaded, or the theme that was active before when the file never
loaded (pointing `interface-theme` at a broken file), and shows an error toast with the file's
path and the parse error. The toast shows once per new error, and the next save that parses
applies as usual. A broken file that is not selected keeps its last good version as its tile and
only logs a warning. When zz starts with a broken selected file there is nothing to keep, so it
draws `default` and says so in the same toast once the window opens.

Settings > Interface > Interface theme shows the built-ins and then the saved themes as tiles.
Duplicate writes the active theme to the first free `themes/theme-N.json` and selects it, so a
theme is customized by editing that file. The copy takes the effective widget corner radius and
shadow strength and an explicit UI font into its look first, so nothing on screen changes. A
duplicated or imported theme whose name is empty or a built-in's is saved under its file name
(`theme-N`), so Settings never shows two tiles called Default. Import reads a theme from the clipboard (the storybook's
theme creator copies one with Copy JSON), saves it the same way and selects it. Thin clients (web,
iOS) never read this folder: they keep their own saved themes in their preferences, with the same
Duplicate and Import (Import uses the browser's async clipboard read on web) and a Delete for the
selected saved theme. A stored theme the build cannot read, one with an unknown key included, is
skipped without touching the other preferences.

Chrome colors come from the theme alone, never from the terminal's palette: there is no key that
makes application chrome follow terminal colors.

`chrome-contrast` adjusts derived surfaces, muted text, fills, and edges without storing colors.
The default is 1.0; desktop and browser Settings show it as 100%. Reset removes the override.
The native macOS client keeps the default derivation strengths and does not show this control.

zz has no frame-rate overlay. The `show-fps` key was removed together with GPUI's `profiler`
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
directly). The `agent-pane` feature works the same way and keeps the same facade
(`crates/zz/src/agent/mod.rs`), but it is **in the default set** since 0.2.0-beta.2, so
`experimental-agent-pane` defaults on and is the only gate a stock build has.

`experimental-agent-pane` and `experimental-editor-pane` are **hard capability gates** with two
consumers of the same config entry. Client-side they gate the pane picker rows (and the `a`/`e`
hotkeys) in `pane::picker::choices` plus the palette's `select-pane-kind` completions.
Daemon-side the entries are forwarded through the config-override push (they are also
`MuxOptionKey`s), and the mux engine rejects `select-pane-kind … agent|editor` while the flag is
off. Turning the agent flag off also blocks `split-window --kind agent`. To enable it again, run
`set-option -g experimental-agent-pane on` in `mux.conf` or at runtime. This changes the daemon
gate directly; the picker row follows the `zz/config` entry, not the mux option. Panes that already exist keep rendering on reattach when the switch is off;
flipping it never destroys pane state.

`use-system-titlebar` maps to GPUI server-side decorations for the main window on
Linux. It is applied at window creation and re-requested for every open window after a watched
configuration change, so switching modes does not require a restart. KDE supports the request on
both Wayland and X11 in the pinned GPUI backend. A Wayland compositor without server-decoration
support keeps client-side decorations; GPUI reports the negotiated mode and the app-owned rounded
frame follows that result. Returning to server decorations also clears the client frame inset.
The in-app navigation/status strip remains available, but its Linux window controls disappear when
the desktop owns them.

`window-background-blur` requests `WindowBackgroundAppearance::Blurred` for both newly created and
already-open windows. `window/background.rs` maps that request to a transparent GPUI surface plus
WindowServer's background-blur radius on macOS, retains GPUI's optional KDE
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

Panes → Appearance → Background opacity controls `pane-background-opacity`, stored as 0–1
and displayed as 0–100%. Valid edits apply live, and Reset restores 50%. The control works
with or without pane gaps. The value reaches paints through `Theme::pane_background_opacity`.
Without gaps each pane content root owns its fill. Terminal, Agent, Editor, picker, and waiting
surfaces use this factor. Browser panes apply it only to the toolbar; page, blank, loading, and
error backgrounds stay opaque.

With gaps the pane frame owns the single fill and draws it in glass (`Theme::pane_frames`, set
from `pane-gaps`). `Theme::pane_background` returns zero alpha, so content roots paint nothing
under the frame, and `Theme::pane_frame(fill, plane, active)` tints the glass with the pane's own
color over the window plane. The edge comes from the interface theme's glass when it has one, and
from `GlassMaterial::edge` otherwise (`linear`), always with blur and refraction off: tiled panes
sit on the flat app background, so there is nothing to frost. On an opaque window the tint is
solid and the glass draws in place with no backdrop copy, costing about what a bordered fill does.
There is no border and no focus glow: the glass edge (glint, fresnel, thin contour) is the frame.
`pane-border-width` sets the glint's width. The active pane lights its glint, fresnel and contour
in the accent, by `pane-glow-strength`; the colored contour is what shows on a light pane, where
added light cannot. Where the window cannot sample its backdrop (WebGL, DirectX) glass paints only
its fill, so `pane_surface` draws a hairline at the glint's width instead: the divider color, the
accent on the active pane. Floating panes take the theme's full glass like every popover. Settings
previews that are not inside a glass frame use `Theme::pane_color`.

The Agent main surface uses the chosen opacity; the raised composer card stays opaque.
The footer inherits the main pane background and adds no fill of its own.
Shadow strength remains independent.
The input card overlaps the transcript by 12 px, hiding content as it scrolls behind the top edge.
The transcript clips before the footer. Prefix cards disable the overlap. Text keeps its own opacity.

A terminal first blends its Ghostty background color with the theme base using
`background-opacity`, then applies the shared pane factor to that result. Ghostty's
`background-blur` stays ignored; `window-background-blur` owns the native compositor request.

`app_shell_surface` paints one continuous app background beneath the sidebar, titlebar,
Settings, and pane layout. It uses `theme::chrome_background`: 93% alpha with native blur,
and the theme background otherwise. Margins, split gaps, and rounded pane corners reveal
this same root surface. They do not paint separate fills. Pane surfaces and their shadows
sit above it; the fixed sidebar, titlebar, workspace, and Settings inherit the root fill.
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

On Wayland compositors using `ext-background-effect-v1`, GPUI sends that same exposed-corner mask as
the effect region. KWin changes pixels even where the client surface is transparent, so the region
does not extend into the client-side shadow inset. Transparent shadow and corner pixels retain the
compositor's unmodified background; tiled and square corners remain square.

`pane-gaps = false` keeps the regular workspace flush: margin, corner radius, and border width
resolve to `0`, and split nodes retain their neutral 1px hairline. The active pane colors only its
half of each adjacent separator with a washed foreground accent; nested T-junctions limit the
segment to the edge the pane touches. With `pane-gaps = true` every value applies exactly as
configured, defaults included . a 6px margin and a 13.5px radius, the window's own corner, so a
maximized grid reads as one rounded surface. There is
no second tier of gapped defaults: the number Settings shows is the number the panes use, which is
what lets the disabled Frame rows stay honest. Gapped panes draw their frame in glass, as above,
and keep the contact shadow. This treatment is built in; the retired `pane-shadow` key produces the
normal unsupported-key diagnostic.
Split divider visuals disappear in this mode because the gap is the separator, but the unchanged
16px drag target still resizes the split.

`pane-inactive-opacity` is independent of pane gaps. The default `0.7` preserves the previous 30%
fade. Terminal panes apply it to glyphs and decorations while keeping their background unchanged.
Browser panes apply it to the toolbar while keeping Chromium page pixels unchanged. Other native
pane surfaces fade toward the opaque window background. Setting it to `1` removes every inactive
pane dimming treatment.

All four frame keys render app-side and therefore apply on every platform. While the effective margin
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

## App-owned Agent keys

Exactly one is left. The three adapter keys are [mux options](#daemon-owned-mux-option-keys) now,
because the daemon spawns the adapter; this one feeds pane creation, which is a client concern.

| Key | Default | Validation / effect |
| --- | --- | --- |
| `agent-working-directory` | unset | Absolute path only; overrides the donor cwd for brand-new Agent sessions |

The adapter command values may be unquoted, wrapped in matching single/double quotes, or supplied as
raw JSON when explicit arguments/environment variables are needed. zz does not log the configured
command, because a JSON environment map may contain credentials. A configured working directory takes
precedence when creating a new pane; a daemon-persisted descriptor cwd takes precedence when loading
an existing ACP session. None of the four has a row in the Settings dialog.

The daemon captures and caches the user's login-shell `PATH` before the first ACP launch,
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

`minimum-contrast` (a WCAG ratio from 1 to 21, default 2) keeps terminal text readable whatever the
theme or the application picked: any text color below the floor against its cell background gets
lighter or darker until it reaches it, keeping its hue. 2 only rescues text that is hard to read, such
as a dark-theme application's yellow on a white background or palette black on a dark one, and leaves
designed palettes almost untouched; 3 to 4.5 is stricter; 1 turns it off. Box drawing, block and
Powerline glyphs are never changed.

File order is preserved, including repeated `palette`, face-specific font-family, and `font-feature`
entries. `config-file` is not part of the native override surface.

`theme = NAME` is transported like every other daemon-owned key. The daemon resolves it first, ahead
of the rest of the override set: it looks `NAME` up in the standard Ghostty theme directories
(`$XDG_CONFIG_HOME/ghostty/themes`, `~/.config/ghostty/themes`, the macOS Application Support
directory, `$XDG_DATA_DIRS/ghostty/themes`, and the installed Ghostty resources), loads that file,
and tags every key it sets `theme-file`. The remaining override entries then apply on top, so an
explicit `background = …` beside a `theme` line wins. Adaptive `light:name,dark:name` values select
by the current color scheme. A name that cannot be resolved produces a daemon diagnostic and leaves
the rest of the set applied. Settings has no theme picker . `theme` is a file-only key.

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
| `experimental-agent-pane` | `on` | flag value (`on`/`off`/`true`/`false`/…); gates `split-window --kind agent` and `select-pane-kind agent` in the engine |
| `experimental-editor-pane` | `off` | flag value; gates `select-pane-kind editor` in the engine |
| `history-trickle` | `2000` | integer `0..=10000`; background scrollback backfill budget. `0` disables trickle and leaves scroll-driven prefetch intact |
| `agent-command` | `npx -y @agentclientprotocol/codex-acp@1.11.0` | Nonempty command string or an `AcpAgentConfig` JSON object (`{"command", "args", "env"}`), up to 4 KiB; what the daemon spawns for a Codex pane |
| `agent-claude-code-command` | `npx -y @agentclientprotocol/claude-agent-acp@0.76.0` | Same, for Claude Code panes |
| `agent-auto-approve` | `reads` | `off`, `reads` or `all` (the flag spellings `on`/`yes`/`true`/`1` still parse as `all`, `off`/`no`/`false`/`0` as `off`); `reads` answers only read-only tool kinds (`read`, `search`, `fetch`, `think`) daemon-side and sends `execute`, `edit`, `delete`, `move`, an absent kind and any unrecognised kind to the permission wizard; when the tier answers, a kinded `session/request_permission` is answered daemon-side with the agent's preferred allow option (`allow_always`, else `allow_once`) and the tool call is still published to the stream. A request with no allow option always falls through to the permission wizard |
| `mouse` | `on` (the pin builds with `-DTMUX_MOUSE=1`) | flag value; session-effective per client on the wire. zz-tui gates its outer-terminal mouse modes on it and the daemon rejects mouse input from terminal-surface clients when off; the GUI's native mouse is ungated (decision 6) |
| `escape-time` | `10` | integer milliseconds; zz-tui's escape-sequence fold timeout (`0` clamps to 1 like the pin's `tty_keys_next`) |

The three agent keys became mux options when the [Agent pane](/concepts/agent-pane.md)'s ACP runtime
moved into the daemon at wire v53 . the daemon spawns the adapter, so the daemon owns what it spawns.
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
- Client-local booleans, pane geometry, and Agent values are validated in the client; later valid entries
  replace earlier values and invalid entries warn while retaining the preceding/default value.
- Daemon-owned values are retained as trimmed raw strings in file order and parsed only by the
  daemon-side appearance loader or mux command engine. Supported appearance and mux keys do not
  produce client warnings.
- The two `experimental-*-pane` flags are the one dual-consumer exception: they parse as
  client-local booleans **and** forward raw to the daemon, so the picker gate and the engine gate
  read the same entry.
- Unknown keys and malformed lines warn and are ignored.

# Provenance

Every client-local knob resolves to `(effective value, provenance)`. Provenance is `Default` when the
loaded file contains no assignment for that key and `Override` when the key is present. Presence is
recorded even when its value is invalid; the diagnostic still follows the warn-and-keep rule, so an
invalid override can display the preceding/default effective value with an `Overridden` badge. This
definition lets Reset remove a stale invalid line instead of incorrectly presenting it as absent.
`widget-corner-radius` and `shadow-strength` are the exception: their default is the active theme's,
so only a line that parses marks them `Override`, and an invalid one leaves the theme's value in place.

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
  it over the destination atomically.

Key matching and value replacement use the same quote-aware, whitespace-prefixed `#` boundary as
the parser. A hash inside a color, palette assignment, word-separator set, or quoted command is never
mistaken for the line's preserved trailing comment.

# Import

`crates/zz/src/config/import.rs` owns the one-shot import. `import_ghostty_config_from(path, scheme)`
reads a chosen donor (`import_ghostty_config` delegates with the discovered default), parses it client-side with the zz-terminal loader, and serializes every key the donor set, directly
(`Ghostty` provenance) or through its `theme` directive (`ThemeFile`), into concrete
`zz/config` values. Theme-derived values are flattened for the current color scheme; an import is a
snapshot.

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
`Cmd+,` on macOS or `Ctrl+,` elsewhere opens it. `SettingsSection::ALL` is **ten pages**, titled
Interface, Status bar, Editor, Panes, Multiplexer, Browser, Terminal, Hosts, System, About. The sidebar labels
them as Appearance, Tools, and Advanced groups. There is deliberately no "General": the page that name described
had accumulated window chrome, a global widget metric, daemon lifecycle and a debug overlay. The
device/pairing page that sat before Terminal was deleted on 2026-08-01 along with pairing itself;
fleet hosts live in `zz/config` and can be managed from the sidebar or **Settings › Hosts**.

Pages use `SettingsStack` groups of `SettingEntry` rows. `SettingEntry::disabled` dims a row and
blocks interaction, which is how the Frame group shows that `pane-margin`,
`pane-corner-radius`, and `pane-border-width` are all inert while `pane-gaps` is off
(`config` forces them to `0` there). Layout holds the `pane-gaps` switch alone. Focus holds the
always-live inactive-opacity factor and the selected pane's edge strength, which needs gaps.

| Page | Groups |
| --- | --- |
| Interface | **Theme** (`theme-mode` as three drawn window previews, `interface-theme` as a sideways-scrolling tile strip with Duplicate and Import) · **Advanced**, collapsed until opened (`UI font`, transient `UI zoom`, macOS `app-icon` as three icon tiles, `chrome-contrast`, `animations`, `widget-corner-radius`, `shadow-strength`, `window-background-blur` as "Window blur", Linux `window-corner-radius` and `use-system-titlebar`) |
| Status bar | Title-bar items shown when the sidebar is retracted (`status-show-session`, `status-badges`, `status-agents`, `status-host`, `status-update`) |
| Browser | **Network** (`browser-egress`) · **Agents** (`browser-remote-debugging-port` as an on/off switch that writes `9222`) · **Search** (`browser-search-provider`) · **Shortcuts** (`browser-element-selector-hotkey`) |
| Editor | **Typography** (`editor-font-size`) · **Display** (`editor-line-numbers`, `editor-relative-line-numbers`, `editor-soft-wrap`, `editor-vim-mode`) |
| Panes | **Layout** (`pane-gaps`) · **Appearance** (`pane-background-opacity`) · **Focus** (`pane-inactive-opacity`, `pane-glow-strength` as Selected pane edge) · **Frame** (`pane-margin`, `pane-corner-radius`, `pane-border-width` as Pane edge width . all disabled without gaps) |
| Hosts | **Machines** (configured hosts, live connection state, Remove) · **Add host** (an inline ssh destination field) |
| System | **Tray** (`tray`, only where the profile has one) · **Daemon** (`quit-daemon-on-exit`) · **Experimental** (`experimental-editor-pane`, `experimental-agent-pane`, each row present only with its cargo feature). `auto-restart-stale-daemon` is a file key with no Settings row |
| Multiplexer | **Navigation** (`picker-focus-sidebar`, `which-key-delay`), **Split panes**, **Options** (Prefix, Mode keys, Mouse, History limit, Clipboard, Escape time), **Import** (path, Choose, Import), and `zz/mux.conf` editor with Reload and Save |
| Terminal | **Appearance** (Font family, Font size, Theme, Cursor style, Cursor blink, Background opacity, Padding X/Y), **Import** (path, Choose, Import), and the `zz/config` appearance editor |
| About | Centered mark (the Dock render at 88pt), name, tagline and version badge · **Updates** (`check-for-updates`, plus a Latest-release row that reads the update state: Check now, or Update / What's new once a newer release is known; desktop only) · **Build** (`CARGO_PKG_VERSION`, OS · arch, the short `ZZ_GPUI_SOURCE` revision, with a copy button on Version that puts all three on one line) · **Project** (repository, releases, new issue, license) |

Structured `zz/config` rows show their effective client-local or daemon-resolved appearance provenance and
a Reset button that removes the corresponding key. Multiplexer uses effective prefix bindings
without per-key provenance badges or Reset controls. All chrome colors come from `cx.theme()`
semantic values.

The Theme group is two picker cards rather than rows of buttons, because both choices are about
appearance: `theme-mode` offers three tiles carrying a drawn 84×56 window mockup . sidebar strip,
title-bar lights, two lines of body text . painted from the active theme's colors for that mode,
with System showing one window in both color sets (the dark copy is drawn full width and clipped
to the right half). The interface theme strip draws each theme as `interface_theme_preview`: the
same window in the theme's colors for the current mode, a menu drawn the way its look draws one
(corners, outline, shadow in its shadow color, frosted or plain, accent or wash selection) and a
primary button in its primary fill and text.
`app-icon` offers three tiles of the real artwork through `crate::app_icon::icon_preview` . macOS's
own renders of `assets/zz.icon`, committed as `assets/zz-{light,dark}-512.png` . where Automatic
renders the variant `cx.window_appearance()` resolves to right now. The preview is Lanczos-filtered
once from 512px to a cached 96px raster before GPUI paints it into the 48pt tile, avoiding the
rounded-edge aliasing caused by a large atlas downscale without mipmaps. The card is macOS-only,
since X11 takes its icon from the window options and Wayland from the desktop file. Both pickers use
the same selection treatment: a plain foreground border around the selected preview (transparent
border otherwise, so nothing shifts), no fill and no hover state, keeping the artwork unchanged.

The Theme group also carries one non-persisted row: **UI zoom**. It is the same bounded `NumberInput`
the geometry rows use, over the root rem as a percentage of the 16px default, with the reset control
in the title where every other row keeps it. Because it writes nothing to disk it is also the one
field here that applies as it changes . a stepper press zooms on the press, and a typed value zooms
once it is a complete number in range, so a prefix such as `1` of `150` never drags the whole UI to
the minimum. Leaving the field clamps and normalizes what it holds. This setting intentionally does
not enter `zz/config`;
it follows Zed's per-run UI font-size adjustment and leaves browser page zoom and terminal font size
under their existing pane-local controls.

The interface theme strip is a `PickerStrip`: a sideways-scrolling row of tiles that bleeds to the
row's edges, keeps its own scroll offset and focus handle keyed on its id in window state, and
moves the selection with the left and right arrows once focused. Picking a tile writes
`interface-theme` through `zz_config::set_interface_theme`, which also drops
`widget-corner-radius`, `shadow-strength` and every retired appearance key so the theme shows as
designed.

Terminal reads saved appearance entries into its controls and writes edits back through the
bounded, comment-preserving `zz/config` writer. Multiplexer mounts the native rope-backed
`CodeEditor` for `zz/mux.conf` with line numbers disabled, 12px monospace text, tmux-grammar
highlighting (`tree-sitter-tmux`, upstream's own `highlights.scm`), and a deliberately square 2px-
inset frame . a file surface, not a control. Save uses the
1 MiB bounded atomic writer; a clean editor reloads when entered, while unsaved text is retained. A
successful mux save asks the daemon to `reload-config`.

The **Split panes** rows in `crates/zz/src/config/settings/multiplexer.rs` read
`MuxClient::prefix_bindings`, the daemon's effective table after its selected mux files, zz overrides, and runtime
commands. Each direction selects one shortcut, preferring `-` / `|`, then `"` / `%`, then another
binding that splits in that direction. A renamed shortcut stays selected while Settings remains open.
Additional shortcuts stay in the text editor. Choosing a pane type or pressing Enter in the shortcut
field saves an ordinary `bind-key` override through the same atomic writer and reload request.
Renaming also unbinds the previous key; an occupied destination key is rejected.

`crates/zz/src/config/mux_bindings.rs` preserves unrelated source text and replaces matching overrides
in the trailing generated split-binding group when users change selections again. Simple direction
flags and `-c "#{pane_current_path}"` map to the dropdown only for the matching direction; opposite-direction bindings, other arguments, or command chains show
**Custom** and remain editable in the text editor. Shortcut changes retain command arguments,
repeat settings, and notes. Pane-type changes use `split-window --kind terminal|picker|browser`; terminal and picker
commands inherit the source terminal directory.
The controls require a connected local session and a clean editor. They check for external file edits
before saving and wait for the daemon to publish the changed binding before accepting another edit.
If the daemon does not confirm within five seconds, Settings refreshes the rows from the effective
bindings, permits another edit, and offers a reload retry. Saving a manual editor change or requesting
a reload cancels the earlier pending shortcut expectation.

Multiplexer's Options rows read the last saved global `set-option` line for each option, or
show its built-in default. Writes replace that line or append one; Reset removes it. The daemon
owns value grammar. Controls require a clean editor and local connection, and wait for daemon
confirmation with a five-second retry fallback. The Reload button uses the daemon's retained
file selection, including explicit `-f` files.

Terminal's Appearance rows read the saved parsed `zz/config` entries. Scalars replace the last
line; Font family replaces its whole stack with one line. The existing watcher applies these
edits. Both pages offer a donor path prefilled from discovery, Choose, and a confirmed Import.
Import requires a clean editor; tmux import also requires a local connection.

The first-run prompt (`crates/zz/src/config/import_prompt.rs`) uses marker
`<data-dir>/zz/import-prompted-v2`. It names every discovered donor and explains that zz does not
read those files on its own. Accepting imports the found donors; Cancel imports nothing. Settings
accepts other paths. Existing installs receive this offer once more after the ownership change.

Shadow strength appears in Interface's Tweaks group beside Widget corner radius. The numeric control
shows 0–100%, steps by five percentage points, and stores a 0–1 factor. Valid edits save as they are
typed; partial or invalid values stay in the field until Enter or blur restores the effective value.
Enter or blur on a numeric field with no override writes nothing while its value is unchanged, so
visiting Widget corner radius or Shadow strength does not pin the theme's value.
Reset removes the override and restores 100% through the normal config watcher.

Structured control callbacks, including Terminal, only write `zz/config`; they never mutate the
GPUI config global directly. Multiplexer writes `zz/mux.conf` and requests a daemon reload. The
existing 500 ms watcher remains the single `zz/config` apply path, updates effective values and
provenance, and refreshes the open Settings route.

# Examples

```ini
# Let KDE (or another capable Linux desktop) own the titlebar and window border.
use-system-titlebar = true

# Blend the terminal color over its opaque pane base.
background-opacity = 0.85

# Ask GPUI and the platform compositor to blur app chrome.
window-background-blur = true

# Set every interface transition to its static state.
animations = false

# The toggle uses a 6px margin, a 13.5px radius, and a glass frame with a 0.5px glint.
pane-gaps = true

# Keep inactive pane content at 70% strength. Set 1 to disable dimming.
pane-inactive-opacity = 0.7

# Optional explicit chrome overrides
pane-margin = 8
pane-corner-radius = 6
pane-border-width = 0.5

# Off by default . the daemon outliving the app is what preserves sessions.
quit-daemon-on-exit = false

# Opt in to the experimental panes; off, no surface (picker, palette, CLI)
# can create them.
experimental-agent-pane = true
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

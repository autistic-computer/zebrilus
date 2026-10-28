# zz-dom

zz in the browser as plain HTML, kept in step with the gpui web client.

```bash
just design client                  # the zz Dev daemon, on http://127.0.0.1:8092
just design compare                 # both clients on a scratch daemon: HTML 8092, gpui 8093
just design parity                  # diff the two
```

## How it is built

- `src/` is a wasm-bindgen wrapper around `zz-client`'s `ClientCore`. `lib.rs` holds the `Client`,
  the socket plumbing and commands; `events.rs` turns every `CoreEvent` into a JSON event with a
  `type`; `tree.rs`, `terminal.rs`, `appearance.rs`, `overlays.rs`, `palette.rs`, `agent.rs` each add
  their own `#[wasm_bindgen] impl Client` block. Heavy payloads (scrollback chunks, kitty image
  bytes, agent updates) stay in Rust and are read back through those modules.
- `web/js/core.js` owns the client, the socket, the event bus (`on(type, handler)`), and the render
  phases (`phase("tree" | "panes" | "after", handler)`). Each feature is one module in `web/js/` with
  its stylesheet in `web/css/`, imported by `main.js`.
- Pane bodies come from `paneRenderer(kind, factory)` in `workspace.js`; a factory returns a view
  with `update()`, `layout()` and `destroy()`.
- Terminal cells are measured the way zz-ui does it: the width is the advance of `m` at the
  terminal font size, snapped to device pixels; the height is the font's natural line box
  (`line-height: normal`) with the appearance's `cell_height_adjustment` applied and snapped
  (`Client::terminal_metrics`). The default appearance gives 8x17 css px. Text is pinned to that
  grid with `letter-spacing`, and wide cells get a two-cell box. `state.cell` holds the base size.
- Everything is painted with `design/kit`: elements name their role (`data-zz="menu-item"`) and
  never hard-code colors, so every theme, mode and role change reaches the page through
  `just design tokens`.

## Settings and preferences

`web/js/settings.js` is the settings route: `#open-settings`, or `emit("open-settings", { section })`
from another module, swaps the sidebar tree for Back and the section list and draws the page over
the workspace (Escape or Back returns). It emits `settings` with `{ open, section }`.

`web/js/preferences.js` holds the local preferences, the same object and `localStorage` key as the
gpui web client (`zz-dev-web-preferences` in dev builds, `zz-web-preferences` in release), read,
migrated and clamped by zz-app's `Preferences` in Rust. Every change is also mirrored to
`zz-dom-preferences`, which seeds the gpui key on a browser that has only that one:

```js
import { preferences, setPreferences, subscribe } from "./preferences.js";

preferences.gaps;                            // one live object, gpui's serialized field names
setPreferences({ pane_margin: 8 });          // sanitized and saved; `set` is the same function
const stop = subscribe((now, before) => {}); // after every change, and when the system mode flips
on("preferences", ({ preferences, previous }) => {}); // the same, on the core bus
```

Fields other lanes read: `sidebar`, `flat_sidebar`, `sidebar_width`, `status_badges`,
`program_colors`, `gaps`, `pane_margin`, `pane_inactive_opacity`, `pane_glow_strength`,
`pip_background_opacity`, `terminal_font_family`, `terminal_font_scale`, `palette_grouped`,
`palette_show_keys`, `animations`, `animation_speed`. `dark()` is the resolved mode.

preferences.js applies the look itself: it writes the theme tokens and icons as `<style>` elements
in place of kit.js's token files, sets `data-zz-mode` and `data-zz-theme`, UI zoom and
`data-zz-still` when animations are off. The CSS comes from `theme/` (`zz-dom-theme`), a release
wasm build of zz-app's `Preferences` and zz-ui's `theme_css`, so accent, tint, radius, contrast,
font and saved or imported themes all come out of the same Rust as the gpui client. With no UI font
picked the tokens name Inter Variable, as the gpui web client does.

## Keeping in sync

- Logic comes from Rust. Anything the gpui client decides outside of drawing (labels, ordering,
  completion, menus, key routing, layout math) should be a `zz-client` function both clients call.
  If it only exists in `clients/app`, move it to `zz-client` rather than rewriting it in JS.
- `scripts/dom-parity.mjs` loads both clients on one daemon and matches every named node in the
  gpui accessibility tree with this page. Buttons carry the gpui accessible name as `title` or
  `aria-label`; text that gpui exposes carries `data-text`. A feature is done when its nodes match
  within 2px.

## Not yet

Open work and known differences are listed in `HANDOFF.md`.

## Workspace and sidebar contracts

- `paneRenderer(kind, factory)` in `workspace.js`: a view has `update()`, `layout()` and `destroy()`;
  `pane.info` is the pane's entry in `client.tree()`. `pane.element` (`section[data-pane]`) is
  `position: relative` and workspace only touches its own children, so another lane can append an
  absolutely placed overlay there (the display-panes chips).
- `paneHeaderButton(factory)`: buttons between a terminal header's title and its split actions, for
  the agent lens. `factory(pane, info)` runs on every tree render and returns the same element each
  time, or `null`. Set `data-lens` on `pane.element` while the agent view covers the terminal: the
  inactive pane then dims under a scrim and the "Waiting for" badge stays off.
- Inactive terminal panes dim through `opacity` on `.pane-body` (`--pane-dim`), so `terminal.js` does
  not dim its own text. While a split divider drags, workspace skips `view.layout()`, so no PTY
  resizes until the drop lands. Floating panes carry `data-floating`; gpui draws their terminal with
  no padding. Float frames use `state.cell`, so they line up with gpui only while `terminal.js`
  measures cells as gpui does.
- Preferences: `preferences.js` owns the live `preferences` object, `setPreferences(patch)` and
  `subscribe(fn)`, stored under gpui's own key (`zz-web-preferences`, `zz-dev-web-preferences` in
  dev builds) with the field names of gpui's `Preferences`, and emits `preferences` after every
  change. `workspace.js` re-exports `preferences` and `setPreferences`.
- Events: `toggleSidebar` and `focusSidebar` open or close the sidebar; the sidebar emits
  `openPalette` (`{ mode: "command" }`) for its `:` key and `focusTerminal` when it lets go of focus.
  `contextMenu(x, y, items)` in `workspace.js` draws a kit popover menu.
- `client.workspace_key(table, key, shift, control, alt, meta)` resolves a key against the `ui` or
  `sidebar` chrome table and returns the action name, or `"prefix"` when the prefix claims the key.
  The sidebar acts only on `toggle-sidebar` from the `ui` table.

# design

HTML mockups that paint with zz's real theme, through penna's HTML kit. Use them to try three or four
takes on a screen side by side, pick one, then build that one in zz-ui once.

```bash
just design serve          # writes the tokens, then serves zz's page on http://127.0.0.1:8090/ and penna's on /penna/
just design tokens         # rewrite zz's tokens while serving, after editing a theme, a role or the kit's surface code
just design penna-tokens   # the same for penna's own page, which shows the kit's roles only
```

## What is where

| Path | What it is |
|---|---|
| `libs/penna/web/penna.css` | The kit: every element with `data-penna="<role id>"` paints with that role's tokens, plus the layout of the kit's roles (buttons, fields, menus, dialogs, toasts, the window, sidebar and settings) |
| `libs/penna/web/kit.js` | Loads the tokens for the chosen theme and mode, rounds corners the way ala does, adds the glass lens in Chromium |
| `libs/penna/web/index.html` | penna's kit page, served at `/penna/` |
| `libs/penna/src/theme_css.rs` | `penna::theme_css`: writes every registered role's look in every built-in theme as CSS tokens. penna's `theme_css` example writes `libs/penna/web/tokens/` |
| `design/zz.css` | The layout of zz's own roles: pane headers, window tabs, the status chip, the agent composer, the palette, the terminal |
| `design/index.html` | zz's page, served at `/` |
| `design/tokens/` | zz's tokens: penna's roles and zz's, with zz's patches, written by `crates/zz-ui/examples/theme_css.rs` |

Both token folders are build output and stay out of git. The HTML client (`clients/dom`) copies the
kit, `zz.css` and zz's tokens into its `dist/kit/`.

## A page

```html
<link rel="stylesheet" href="/penna/penna.css">
<link rel="stylesheet" href="/zz.css">
<script src="/penna/kit.js" data-tokens="/tokens/"></script>

<div data-penna="popover" class="penna-menu">
  <div data-penna="menu-item">Split right</div>
  <div data-penna="menu-item" data-state="selected">Close pane</div>
</div>
<button data-penna="button" data-prominent>Save</button>
```

`data-tokens` names the folder the tokens come from, relative to the page. Without it, `kit.js` reads
`tokens/` next to itself, which is how penna's page gets penna's tokens. `data-themes` lists the theme
ids the switcher offers, separated by spaces or commas (`default macos yaru` without it; add any id an
app registers as a whole theme), and `data-theme` picks the one a first visit opens on (the first
listed without it). zz's page and the HTML client set `data-themes="macos default yaru"` and
`data-theme="macos"`. A theme in the URL or saved from an earlier visit wins, and the switcher lists it
even when `data-themes` does not, as long as it is a plain token file name (lowercase letters, digits
and dashes); any other value, and a mode other than `dark` or `light`, is ignored.

`kit.js` loads the theme in the corner switcher (or `?theme=macos&mode=light`), sets
`data-penna-theme` and `data-penna-mode` on `<html>`, and exposes `window.pennaKit` (`choice`,
`apply`, `roundAll`). `?bare` hides the switcher, `?flat` turns the lens off.

The kit's names are `data-penna`, `--penna-*` properties and `.penna-*` classes. zz's own classes and
properties keep `zz-` (`.zz-terminal`, `--zz-pane-header-height`).

## What maps to what

| HTML | zz-ui |
|---|---|
| `data-penna="sidebar.row"` | `.surface(roles::SIDEBAR_ROW, cx)`, any id from `SurfaceRole::KIT`, penna's `roles.rs` or zz-ui's `roles.rs` |
| `:hover`, `:active`, `:focus-visible` or `data-state="hover"` | `PaintState::Hover`, `Active`, `Focused` |
| `data-state="selected"`, `"current"`, `"disabled"` | `PaintState::Selected`, `Current`, `Disabled` |
| `data-tone="danger"`, `data-prominent` | `Surface::new(role).tone(Tone::Danger).prominent(true)` (kit buttons and labels, and the roles in zz-ui's `roles::TONED`) |
| `data-ground` on a container, kit roles inside it | `Surface::new(role).on(theme.ground_of(container))` |
| `color: var(--penna-muted)` | `muted_on(&layer, cx)` |
| `display: flex; gap: 8px; padding: 0 12px` | `h_flex().gap(px(8.)).px(px(12.))` |

Colors, edges, shadows, radius and smoothing come out of `Theme::layer` for every role, state, tone and
theme, through `penna::theme_css`. Mockups name roles, not colors, same rule as zz-ui.

## How close it is

- Exact: fills, ink, edges, shadows, radius, per theme, mode and state.
- Close: squircle corners need Chrome 139+ (`corner-shape`); other browsers draw round ones.
- Glass: the blur, tint and color boost match; the rim (glint, contour, fresnel) is box-shadows. Edge
  refraction is an SVG lens that only Chromium draws.
- Left out: chroma presets, tint, contrast, accent and zoom settings, and user theme files.
- Fonts: `system-ui`, which is SF on macOS like desktop zz. Thin clients use Inter.

## Mockups

Put each question in its own folder under `mockups/`, one page with the variants next to each other
(`.penna-variants` lays them out as figures). Mockups change layout and structure. To try a different
look for a role (a tint instead of an accent, another radius), edit its theme file in
`libs/penna/themes/` (zz's own roles: `crates/zz-ui/themes/`) and rerun `just design tokens`.

---
type: Configuration
title: zz icon set
description: zz's icons, adapted from Solar and drawn from skeletons so their corners follow the theme radius and superellipse smoothing, how the app and the site get them, and how to draw, preview and review a new one.
resource: libs/penna/src/icon/solar.json
tags: [ui, icons, penna, storybook, site, theme]
timestamp: 2026-10-24T00:00:00Z
---

# Overview

zz's icon set has 95 glyphs: every `IconName` in penna except its three brand marks (`claude`,
`openai`, `brand-chrome`), which keep their own artwork as files in `libs/penna/assets/icons`. The
zz logo is not in the kit: its artwork is `crates/zz-ui/assets/icons/zz.svg`, which `zz_ui::Assets`
serves at the path `zz_ui::ZZ_LOGO` (`icons/zz.svg`) before falling back to penna's assets. The
set lives as skeletons in `libs/penna/src/icon/solar.json`, not as SVG files, and is drawn at
paint time for the current theme, so radius 0 gives flat icons and higher radii round them the way zz
rounds panes. The desktop app, the web and iOS clients and the storybook all draw it.

The glyphs are adapted from the Linear style of the [Solar Icons Set](https://www.figma.com/community/file/1166831539721848736)
by 480 Design, CC BY 4.0. Each icon taken from Solar names its source in a `solar` field; icons
without one are zz's own, drawn in Solar's style. `libs/penna/src/icon/LICENSE-SOLAR` holds the
credit and the list of changes the license asks for, and `THIRD_PARTY_NOTICE.md` lists the set.

Program tiles in the flat sidebar draw the generic programs (a terminal, `log`, `sleep`, `top`,
`caffeinate`, lazygit, an inspector, a browser) from this set too, through the `icon` field of
`zz_client::program::Program`; brand programs use their own marks from `crates/zz-ui/assets/programs`.

History: Lucide, Phosphor, Iconoir and Tabler files came first; on 2026-10-10 zz drew its own
Mac-school set from skeletons, and on 2026-10-24 it moved to Solar, kept on the same skeleton
renderer.

# How an icon is drawn

Each icon is a list of parts on a 24-unit grid with one stroke width for the whole set (`stroke` in the
JSON, 1.5, Solar's own weight; 1 read too thin and 2 too heavy). Corners are the only thing the theme changes:

- **Corner radius** = theme radius × 0.5 grid units × the vertex's corner scale, capped at 0.45 of the
  shorter adjacent edge. That cap is the same rule `Window::set_adaptive_corner_fraction` applies to
  chrome, so no icon turns into a pill.
- **Corner shape** is a superellipse with the exponent `CORNER_SMOOTHING` (4, a squircle; 2 would be
  a circular arc), the default corner every window draws, sampled 16 steps per corner. No theme
  changes it.
- **Radius 0 is flat**: sharp corners, square caps, mitered joins. Above 0, caps and joins are round.

Two renderers implement the same math and must stay in step when a part kind changes:
`libs/penna/src/icon/glyphs.rs` (Rust, what the app draws) and
`clients/storybook/icons/glyph.js` (the review page).

# How it ships

`IconName::path` stays `icons/<name>.svg`. When it renders, `Icon` rewrites a path the set draws to
`icons/<radius>-<smoothing>/<name>.svg`, with the radius from `cx.theme()` and the smoothing from
`CORNER_SMOOTHING`, and the kit's `Assets` draws that path on load. The radius and smoothing travel
in the path because ala's sprite atlas caches rasters by path and size; a new radius has to be a new
path or the old raster stays on screen. Changing the radius or the interface theme refreshes the
windows, so every icon redraws. A plain
`icons/<name>.svg` load, outside any theme, draws at radius 6 and smoothing 4. Nothing has to be wired
per client: every app that uses the kit's `Assets` and `Icon` gets the set.

The bijection tests in `libs/penna/src/icon/mod.rs` hold `IconName` to the set plus the brand
files: every variant loads, every file in `assets/icons` has a variant, and every icon in `solar.json`
names its variant and has no file shadowing it.

## Part kinds

Each part is an array whose first item is its kind. Points are `[x, y]` or `[x, y, cornerScale]`;
a missing scale means a sharp vertex.

| Kind | Arguments | Notes |
|---|---|---|
| `line` | `x0, y0, x1, y1` | |
| `path` | points | Open polyline; inner vertices round by their scale |
| `shape` | points | Closed polygon; every vertex rounds by its scale |
| `rect` | `x0, y0, x1, y1[, scale]` | Scale defaults to 1; boxes in the set use 1.3 |
| `circle`, `ellipse` | `cx, cy, r` / `cx, cy, rx, ry` | Never change with the radius |
| `arc` | `cx, cy, r, from, to` | Degrees, clockwise from east, drawn from `from` to `to` |
| `dot` | `cx, cy[, size]` | Filled; square at radius 0, round from radius 2 up |
| `gear` | `teeth, root, tip, tipWidth, rootWidth[, scale]` | Centered on 12, 12 |
| `curve`, `cubic` | quadratic / cubic Bézier points | Fixed curves |
| `solid` | `[part]` | Fills the inner part, no stroke |
| `fade` | `opacity, [part]` | Draws the inner part at that opacity |
| `d` | SVG path data | Escape hatch for organic outlines (palette, moon, the undo and redo loops) |

# Solar rules

- The live area is 2 to 22. Window and pane glyphs share one frame, `["rect", 2, 3, 22, 21, 0.67]`
  (Solar's panel frame), with title bars, splits and cards drawn inside it. Square boxes fill 2..22.
- Corner scales come from Solar: a scale is Solar's corner size divided by 12, so the default theme
  radius (24, which is 12 grid units) draws Solar's corners and smaller radii sharpen them in step.
  Big boxes land on the 0.45 cap the way Solar's near-squircle boxes do.
- Organic outlines Solar draws with curves (bell, moon, braces, the chat bubble, the palette, the
  steam over the cup) stay fixed `d` paths; they have no corners for the radius to change.
- Dots are filled `dot` parts, never smaller than the stroke: 1.75 for the small accents (window lights, the `i` and `!` dots, the
  keyboard's keys, the drive's lights), 3 for grips, ellipses and palette wells. Round things stay circles.
- Arrow shafts stop about 1.25 units short of the tip, so at radius 0 the shaft's square cap does
  not stick out of the mitered head. Solar runs its shafts to the tip.
- Points must stay inside the grid at radius 0: Solar's soft triangle and star have their sharp
  corners outside it, so ours put the vertices inside and round them back toward Solar's look.
- Keys are drawn the way keyboards print them, in two sets: Apple's (`key-control` ⌃, `key-option` ⌥,
  `key-tab` ⇥) and ISO 9995-7's for every other keyboard (`key-control-iso` ⎈, `key-alt-iso` ⎇,
  `key-tab-iso` ↹), with `key-escape` ⎋ shared. The phone key row picks the set at build time, the way
  `Kbd` picks ⌃⌥ or Ctrl Alt. The prefix key has no glyph of its own: it shows the prefix it sends,
  the control mark and the letter (⌃B, ⎈B), so it follows a rebinding.
- Meanings follow use, not names: `panel-left` is Toggle sidebar; `list` and `list-tree` are the
  sidebar's flat window list (tiles with a title and a shorter detail line) and session tree (the same
  tiles in three rows, the last two hanging off the first by elbow guides), and the footer's view
  button shows the one on screen; `panel-right` and `panel-bottom` are the split right and split down
  actions (two equal panes); `layout-columns` is a float's Tile back (one tall pane and two stacked);
  `layers` is a session; `layers-plus` and `window-plus` are New session and New window on the host
  and session headers of both sidebars, the flat list and the session tree: the session's layers
  with a plus at their lower right, and a stack of tiles (how the flat sidebar draws a window, so not
  a pane frame) with the plus on the front tile; `server-plus` is Add host; `picture-in-picture` is Float (Solar's PiP, the card
  in the frame's corner); `loader` is Solar's open ring, which the app spins; `hard-drive` is a host.
- Each icon may carry a `note` that explains a choice; the review page shows it on the card next to
  its Solar source.

# Importing another Solar icon

Solar's Linear SVGs use only absolute `M L H V C Z`, and every rounded corner is a pair of cubics
between two straight edges. To bring one in, read its file from
[saoudi-h/solar-icons](https://github.com/saoudi-h/solar-icons) (`packages/core/svgs/<category>/Linear/<name>.svg`)
and turn each corner into a `shape` or `path` vertex where the two edges meet, with Solar's corner
size ÷ 12 as its scale. Straight subpaths become `line` parts, circles `circle`, zero-length round-cap
strokes `dot` parts, and anything else stays a `d` path. Record the source name in `solar`.

# Adding or changing an icon

1. If zz needs a new `IconName`, add the variant and its `icons/<name>.svg` path in
   `libs/penna/src/icon/mod.rs`. Only brand marks get a file in `assets/icons`.
2. Add one line to `solar.json` with `name` equal to the path's stem, its `variant`, a `group`, its
   `solar` source if it has one, and its `parts`. Draw it with the rules above.
3. Preview it with `just storybook run`: Foundation › Icon set opens on a catalog of every icon by
   group with its name and Solar source, then shows the set at the current Radius knob and the fixed
   smoothing, a radius ramp, chrome sizes and named 48px construction cards. Check radius 0, 6 and 25,
   and 12 to 16px. `include_str!` reads the JSON, so touch a file under `libs/penna/src` if the
   watcher does not rebuild after a JSON edit.
4. `cargo test -p penna --lib icon` draws every icon at radius 0, 6 and 25 and checks the
   bijection.
5. Re-export the site copies (see [How it ships](#how-it-ships)).
6. For review, `node clients/storybook/icons/build-sheet.mjs` writes `target/icon-sheet/zz-icons.html`:
   every icon with radius and smoothing sliders, the 24 grid, its Solar source and how many Rust files
   use it. The review artifact is `https://claude.ai/artifact/A4dypJQa9Nqs8puPXPiBjS`; republish to
   that URL so comments stay with it.

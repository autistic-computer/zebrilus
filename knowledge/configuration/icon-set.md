---
type: Configuration
title: zz icon set
description: How penna draws themed icons and where zz keeps application and program marks.
resource: libs/penna/src/components/icon/glyphs.rs
tags: [ui, icons, penna, theme]
timestamp: 2026-11-24T00:00:00Z
---

Kit icons are 24-unit glyphs in `libs/penna/src/components/icon/solar.json`.
`IconName` selects a glyph. At render time `glyphs::path` includes radius and
smoothing in the asset path so ala's cache distinguishes different shapes.
`glyphs::load` draws the requested SVG. Theme-aware corners, fixed curves
and current-color strokes let the same glyph work at different UI sizes.
The Solar source and adaptations are credited in the kit's `LICENSE-SOLAR` and
the repository's `NOTICE.md`.

zz-owned logos, provider marks and program art live in `crates/zz-ui/assets`.
`Assets` in `crates/zz-ui/src/program.rs` serves these and falls back to penna.
An `IconName::Asset` selects the application's own file.

# Part kinds

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
| `cut` | `[part]` | Strokes the inner part out of every other part of the glyph, as negative space |
| `d` | SVG path data | Escape hatch for organic outlines (palette, moon, the undo and redo loops) |

# Adding a glyph

1. Add the variant and path to `libs/penna/src/components/icon/mod.rs`. Give
   its entry in `solar.json` the matching path stem, variant, group, parts and
   Solar source when applicable. Keep brand artwork in the owning app.
2. Check the glyph on the 24-unit grid and at the small sizes used in chrome.
   Compare flat and rounded settings. `just storybook penna` opens the kit's
   Foundation icon catalog. The local review sheet is generated with
   `node libs/penna/storybook/icons/build-sheet.mjs target/icon-sheet/zz-icons.html crates clients`.
3. The icon tests in `mod.rs` check that each variant loads and each glyph names
   its variant. The glyph tests cover several radii. The library check is
   `just libs check penna`. [Storybook](/playbooks/storybook.md)

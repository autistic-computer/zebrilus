---
type: Reference
title: Ghostty X11 color reference
description: Provenance of crates/zz-terminal/src/x11-rgb.txt, copied verbatim from Ghostty's rgb.txt for X11 named-color lookups.
resource: third_party/ghostty-reference/UPSTREAM.md
tags: [ghostty, x11, color, reference, license]
timestamp: 2026-11-24T00:00:00Z
---

# Overview

`x11_colors` in `crates/zz-terminal/src/appearance.rs` embeds the named-color table
for [terminal appearance](/terminal/appearance.md#color-parsing-and-the-x11-table).
Its provenance is separate from the native [VT engine](/terminal/libghostty-vt.md).

# Schema

| Field | Value |
| --- | --- |
| Local file | `crates/zz-terminal/src/x11-rgb.txt` |
| Upstream file | `src/terminal/res/rgb.txt` (Ghostty) |
| Pinned commit | `cf60af281bd7559a819aa25372cef01d623b8c5a` |
| Original data source | X.Org `rgb` project |
| License | MIT/X11 |

# Citations

- `third_party/ghostty-reference/UPSTREAM.md` records the source and revision.
- `third_party/ghostty-reference/LICENSE.txt` carries the Ghostty license.

# Related

- [Terminal appearance concept](/terminal/appearance.md) . where this named-color table is consumed (Ghostty config palette/color parsing)
- [`zz-terminal` crate](/crates/zz-terminal.md) . owns the copied file

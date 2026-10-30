---
name: ala-interface
description: >-
  ala layout, hover, and optical alignment rules for zz chrome. Use when
  building or tweaking ala UI: buttons, rows, labels, icons, chevrons,
  hover/fg highlight, padding, composer chrome, agent timeline rows, or
  anything that "looks a few pixels off". Reach for it before putting
  `.hover()` on a nested div or trusting `items_center` to line up an icon
  with text.
---

# ala interface

Theme tokens, clippy's ban on raw `rgb`/`hsla`, and widget conventions live in
`knowledge/configuration/ui-conventions.md`. This skill is the ala mechanics
that file does not cover. Source of the worked example:
`crates/zz-ui/src/agent.rs` (`activity_row_marked`, `activity_row_glyph`).

## Hover lives on the id'd row

ala only tracks hover on a **stateful** element, one with `.id()`. A nested
`div()` with `.hover()` or `group_hover` never sees the pointer.

Put color and hover on the same element that already has the id (usually the
clickable row). Children **inherit** `text_color` if they do not set their own.

```rust
let foreground = cx.theme().foreground;
h_flex()
    .id(("some-toggle", id))
    .text_color(cx.theme().foreground.muted())
    .cursor_pointer()
    .hover(move |this| this.text_color(foreground))
    .child(Icon::new(icon).size(px(13.0))) // inherits, no own color
    .child(div().child(label))             // inherits
```

Do not:

- `.hover()` on a child without `.id()`
- `group` / `group_hover` as a workaround when the row is already stateful
- set `text_color` on the label and then hope parent hover overrides it

If icons must stay muted while text highlights, set color on the icon
explicitly. If the whole row should go to `foreground` (text, icon, chevron),
set color only on the row.

## `items_center` centres boxes, not ink

ala centres a glyph on its **ascent/descent box**, and `items_center` centres
the icon's box on the same line. Neither is where the letters are. The rule is
Apple's for SF Symbols: an icon next to text is **centred on the cap height**.

`penna::icon::cap_center_drop(font_size, icon_size, cx)` reads the drop from
the theme font's metrics (the numbers are in `ui-conventions.md`), so the same
code is right on every font.

Do this:

1. One shared builder for every row of the same shape (font, metrics, hover
   cannot drift across callers).
2. Explicit row height, font size, and line-height in pixels.
3. Line-height **taller** than ascent+descent, or `overflow_hidden` clips
   descenders. At 13px the system font is 15.31px tall; use 16px.
4. Size icons to the font (`.size(px(13.0))`), not `.medium()` (that is 14px,
   `size_3p5`) or `.small()` (12px, `size_3`).
5. Centre the icon on the line, then move it down by `cap_center_drop` for the
   label's font size and the icon's size. Never a fixed `0.5px`: that was
   0.17px too low on SF Pro and 0.5px too low on Inter.
6. The glyph's ink has to be centred in its 24-unit box, or no layout fixes it.
   `triangle-alert` sat 0.86 to 1 unit low (its round apex trims the top and
   the base does not move) and is now drawn a unit higher. Check a glyph with
   `cargo run -p penna --example export_icons` and `resvg`.

```rust
const ROW_HEIGHT: f32 = 28.0;
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 16.0; // > 15.31px ascent+descent

fn row_glyph(icon: IconName, size: f32, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(FONT_SIZE), px(size), cx))
        .child(Icon::new(icon).size(px(size)))
}
```

`line_height(relative(1.0))` is not enough: it still centres on the em box,
and a 13px line box clips the 15.31px font.

Wrapping rows (markdown task lists, alerts, card titles, disclosure headings)
cannot `items_center` the marker on the whole block. Give the marker a
**first-line strut**: a box as tall as the first line's pixel line height, the
icon sized to that line's font and centred in it, then the same
`cap_center_drop`. A heading with no explicit line height gets the 1.618 default,
and an `items_start` row then parks a 14px chevron near the top of a 19px line,
almost 3pt above the caps. Worked examples: `render_list_item_row` in
`libs/penna/src/text/node.rs`, `settings_list_disclosure_header` in
`libs/penna/src/settings.rs`.

What is left is pixel snapping: `cap_center_drop` already nudges the icon by up
to a quarter point so it and the baseline round to the same device pixels. The
storybook draws Inter through skrifa's autohinter, which grows 13px capitals by
a device pixel, so 13px icons read about 0.45pt low there while the desktop is
right. Measure alignment in the desktop app, not only in the storybook.

## One row, three callers

If group headers, tool rows, and reasoning rows must look identical, they
share one function. Copy-pasting the same `h_flex` three times is how font
size, chevron placement, and hover diverge.

Worked example: `activity_row_marked` in `crates/zz-ui/src/agent.rs`, which
draws the agent timeline's group headers, tool rows and reasoning rows. The
chevron sits **after the label** (same flex children), not `justify_between`.

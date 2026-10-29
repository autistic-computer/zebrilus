---
name: gpui-interface
description: >-
  GPUI layout, hover, and optical alignment rules for zz chrome. Use when
  building or tweaking GPUI UI — buttons, rows, labels, icons, chevrons,
  hover/fg highlight, padding, composer chrome, agent timeline rows, or
  anything that "looks a few pixels off". Reach for it before putting
  `.hover()` on a nested div or trusting `items_center` to line up an icon
  with text.
---

# GPUI interface

Theme tokens, clippy's ban on raw `rgb`/`hsla`, and widget conventions live in
`knowledge/configuration/ui-conventions.md`. This skill is the GPUI mechanics
that file does not cover. Source of the worked example:
`crates/zz-ui/src/agent.rs` (`activity_row`, `activity_row_glyph`).

## Hover lives on the id'd row

GPUI only tracks hover on a **stateful** element — one with `.id()`. A nested
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

GPUI centres a glyph on its **ascent/descent box**, and `items_center` centres
the icon's box on the same line. Neither is where the letters are. The rule is
Apple's for SF Symbols: an icon next to text is **centred on the cap height**.

The middle of the capitals sits `(ascent - descent - cap height) / 2` below the
middle of the line box, whatever the line height. For SF Pro that is 0.28px at
11px, 0.31px at 12px and 0.33px at 13px; for Inter it is 0.
`zz_gpui_kit::icon::cap_center_drop(font_size, icon_size, cx)` reads it from
the theme font's metrics, so the same code is right on every font.

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
   `cargo run -p zz-gpui-kit --example export_icons` and `resvg`.

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
`libs/kit/src/text/node.rs`, `settings_list_disclosure_header` in
`crates/zz-ui/src/settings.rs`.

What is left is pixel snapping. GPUI rounds the text baseline and the icon's
box to device pixels separately, so an icon left at the exact cap centre jumps
a pixel against the letters depending on where the row lands (a scrolled
transcript). `cap_center_drop` moves the icon by up to a quarter point so its
top sits a whole number of half points above the baseline; then both round
together and the error is fixed, at most 0.25pt by the metrics. CoreText also
inks caps about 0.1pt above the cap height. Measured on the macOS text system
(2026-10-12): 13px rows land 0.13 to 0.19pt low, 12px rows 0.32 to 0.36pt low
(SF Pro's 12px cap half-height sits between two pixel steps), where the old
0.5px rule left triangles 0.66 to 0.88pt low and the settings chevron 3.1pt
high. The storybook draws Inter through
skrifa's autohinter, which grows 13px capitals by a device pixel, so there
13px icons read about 0.45pt low while the desktop is right; measure alignment
in the desktop app, not only in the storybook.

## One row, three callers

If group headers, tool rows, and reasoning rows must look identical, they
share one function. Copy-pasting the same `h_flex` three times is how font
size, chevron placement, and hover diverge.

Worked example: `activity_row` in `crates/zz-ui/src/agent.rs`. Chevron sits
**after the label** (same flex children), not `justify_between`.

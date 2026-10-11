---
type: Design Plan
title: "Design system: tone, emphasis and ground"
description: "How zz chrome picks colors: every filled, tinted or outlined widget names a tone (what it means) and an emphasis (how loud it is), the theme turns the pair into one Paint on the ground it sits on, and the old hand-picked washes, opacities and per-widget variant enums move onto it one component family at a time."
status: In progress (foundation, fields, agent cards, badges and buttons built)
resource: crates/zz-gpui-kit/src/foundation/variant.rs
tags:
- ui
- theme
- design-system
- zz-gpui-kit
- design-plan
timestamp: 2026-10-11T00:00:00Z
---

# Why

Before this, call sites picked their own colors. An audit on 2026-10-11 found:
- 4 keycap recipes;
- about 14 hand-built chips, pills and badges;
- 5 ways to draw a selected row;
- 6 ways to write quieter text;
- 4 divider recipes;
- 7 different border alphas.

`ButtonVariant` mixed what a button means (Primary, Danger) with how loud it is (Ghost, Secondary) in one enum, and had an `outline` flag on top. `TagVariant` was a second, smaller system whose "Primary" meant something else.

The palette itself was fine: six roots and a scrim (see
[ui-conventions](../configuration/ui-conventions.md)). The layer between the roots and the widgets was missing.

# The model

A widget picks two things and the theme does the rest.

- **`Tone`**, what it means: `Neutral`, `Primary`, `Danger`, `Warning`, `Success`.
  - Primary's color comes from the look: `primary = foreground | accent` (`PrimaryStyle`). It works like `selection = accent | wash` and defaults to foreground, so the main action is white on dark and black on light.
  - Code says "this is the main action" and never picks the color itself.
- **`Emphasis`**, how loud it is: `Solid`, `Soft`, `Outline`, `Ghost`.
- **`theme.paint(tone, emphasis)`** returns a `Paint`:
  - fills for rest, hover, pressed and selected;
  - a border and its width;
  - text and selected text;
  - the `Ground` it sits on.

  `fill_at`, `border_at`, `text_at`, `muted_at` and `faint_at` read any `PaintState`: rest, hover, active (pressed), selected, focused, disabled.
- **`painted(paint, state)`** draws a paint on any element.
- **`painted_interactive(paint, selected, enabled)`** draws one on an element with an id and adds live hover and press. Hover only changes the fill, which GPUI can repaint; text color cannot follow a hover style.

## Recipes

| Emphasis | fill | text | border | hover / pressed |
|---|---|---|---|---|
| solid | the tone | readable on the fill | none | the fill's `hover()` / `active()` |
| soft | neutral: one wash step on the ground; other tones: the tone at 12% | neutral: the ground's ink; other tones: the tone mixed 30% toward the ink | the look's outline hairline, if it has one | the next wash or alpha step |
| outline | none | as soft | neutral: ink at 30%; other tones: the tone at 55%; 1px | soft's rest / hover |
| ghost | none | as soft | none | soft's hover / pressed |

**Selected:**
- Neutral soft, outline and ghost on the window follow the look's `selection`: accent fill or a wash. On any other ground, and for coloured tones, selected is the pressed fill.
- Text on an opaque selected fill is picked by perceptual lightness (`readable_on`), so a light accent gets dark text and a dark accent gets white.

**Focused:** the border turns to the focus color. A neutral or primary paint focuses in the primary tone (the ground's ink when primary is the foreground, the accent otherwise); danger, warning and success focus in their own tone.

**Disabled:** fill, border and text all at half strength.

## Ground

A `Ground` is what a widget sits on: a fill, and the ink that reads on it.
- `theme.ground()` is the window.
- `paint.ground_at(state)` is the ground a paint makes for what sits inside it: its fill, composited over its own ground, and its text color in that state.
- `theme.paint_on(ground, tone, emphasis)` paints relative to a ground: neutral takes the ground's ink, and washes tint the ground's fill.

This is how a keycap inside a red Submit, or on a row selected with the accent, stays legible:

```rust
Keycap::new("↵").on(theme.paint(Tone::Primary, Emphasis::Solid).ground_at(PaintState::Rest))
```

Without it, the keycap would compute muted gray against the window.

## Text

| Level | Use | From |
|---|---|---|
| default | labels, body | the ground's ink (`foreground` on the window) |
| muted | descriptions, secondary labels | `paint.muted_at(state)`, or `.muted()` on the window |
| faint | placeholders, hints | `.faint()` |
| on a fill | text on a solid or selected fill | `paint.text_at(state)` |

`muted_at` pulls the text toward the fill it actually sits on, so it reads on the window, a solid fill and an accent selection alike.

# Fields

Text, number and select fields are neutral paints at the look's field emphasis, `field = soft | outline` (`FieldStyle`, soft by default). Call sites never pick it.
- `Input::tone(Tone::Danger)` marks an invalid value: a danger fill or border, and a danger focus ring.
- `Input::emphasis(Emphasis::Ghost)` is for a field that shows its edge only on hover or focus.
- `Input::on(ground)` puts a field inside a tile or card on that surface's ground.
- `NumberInput` takes the same three.

The shared chrome, `field_chrome` in `input/field.rs`, does three things:
- it always draws at least a 1px border, clear when the paint has none, so focusing never moves the text;
- it hovers to the soft hover fill;
- it focuses to the focus color.

**Text inside a field:**
- placeholders are `faint_at`;
- the caret is the field's text color;
- the selection is a wash of it.

**Select** draws its trigger as the same field with a caret. Open or focused counts as focused, and a missing value shows its placeholder faint.

**Switch:**
- track: the primary solid fill when on, the neutral soft pressed fill when off;
- thumb: when on, the text color that reads on the track; when off, the lighter of background and foreground, so it stays a light knob in both modes.

**Discrete slider:**
- filled pills: the primary solid fill;
- the rest: the neutral soft pressed fill;
- focus: the paint's focus color.

# Widgets

Kit widgets on the model:
- **`Badge`**: tone, emphasis, size, optional icon, any children after the label. `.opaque()` for a badge floating over content. One widget for chips, pills, status and counts.
- **`Keycap`**: a key as a cap. Neutral, outline by default, `.on(ground)` inside another paint. `Kbd` still formats keystrokes; it moves onto `Keycap` during migration.
- **`Check`**: `Check::checkbox(checked)` and `Check::radio(checked)`.
  - Off: a neutral outline.
  - On: the tone solid (primary by default), with `.on(ground)`.
  - Corners: a checkbox at half the theme radius, a radio round.
- **`Button::tone(..)` / `.emphasis(..)`**: `ButtonVariant` is `Paint(Tone, Emphasis)`, `Link` or `Text`. The shorthands name the common pairs: `primary` (primary solid), `secondary` (neutral soft, the default), `ghost` (neutral ghost), `danger`, `warning` and `success` (soft). A dialog's destructive OK is `Paint(Danger, Solid)`.
- **Tiles and rows** are not widgets. A tile is `painted_interactive(theme.paint(Neutral, Soft), ..)`, a row the same with `Ghost`.

**Corners:**
- Badges and keycaps round with `control_radius()`, like buttons: pills in full, squircles in modern, square in flat.
- Checkboxes round at half the theme radius; radios are round.

# Rules

- Name a tone and an emphasis. Do not pick a fill with `washed()`, `raised()` or `opacity()` at a call site outside the kit.
- A widget nested in a painted surface takes that surface's `ground_at(state)`.
- Secondary text comes from `muted_at`, not from `opacity(..)` on the foreground.
- A variant that cannot be expressed as a tone and an emphasis is a missing recipe in the kit, not a local one-off.

# Component map

| Component | Tone · emphasis |
|---|---|
| Submit, Send, the main action of a surface | primary · solid |
| Skip, Cancel, dismiss | neutral · ghost |
| A secondary action beside the main one | neutral · soft |
| Option tile in a question or permission card | neutral · soft, selectable |
| Menu, list and chooser rows | neutral · ghost, selectable |
| Shortcut keycap | neutral · outline (ghost inside rows) |
| Count or kind chip (`2/3`, `Base`) | neutral · soft |
| Reject, delete | danger · soft, or danger · solid when it is the main action |
| Failing or blocked status | danger · soft |
| Waiting status | warning · soft |
| Done or healthy status | success · soft |
| Multi-select mark | `Check::checkbox`, primary |
| Single-select mark | `Check::radio`, primary |
| Text, number, select field | neutral at the look's field emphasis |
| Invalid field | danger at the look's field emphasis |
| Inline field (rename in place) | neutral · ghost |
| Switch, slider | primary · solid when on, neutral soft when off |

# Where to look

- `crates/zz-gpui-kit/src/foundation/variant.rs`: `Tone`, `Emphasis`, `Paint`, `Ground`, the recipes.
- `crates/zz-gpui-kit/src/{badge,check,kbd}.rs`: `Badge`, `Check`, `Keycap`.
- The storybook's Foundation → Design system story (`#/design-system`) draws every recipe from the same code. Its sections are:
  - tones;
  - a tone × emphasis × state matrix;
  - buttons;
  - badges, keycaps and checks;
  - tiles and rows;
  - text levels;
  - the same paints on the window, a raised panel and a popover.

  Its Primary and Selection knobs switch the look settings live.

# Migration

One component family at a time, each checked in the storybook and in the app:

1. **Agent cards** (done): `CardTile` paints tiles, keycaps and checks on its ground; `card_chip` is a `Badge`; Skip is neutral ghost and Submit primary solid. The retry and auth buttons in the zz and thin-client error cards wait for the button step.
2. **Fields** (done): `Input`, `NumberInput`, the `Select` trigger, `Switch` and `DiscreteSlider` draw from paints. The zz-ui fields that wrap a bare `Input` in their own chrome (the composer, the palette, the browser address bar, the agent title) still pick their colors and move with the clean-up.
3. **Badges** (done): `Tag` is deleted. `Badge` carries the editor's vim mode, settings provenance, the palette's mode, host and command pills, chooser and picker chips and the command kind; the palette shortcut is a `Keycap`. The agent status pill, the trace count, the plan chip and the pane's status badge, drag chip and overlay lines paint from tones too. `Badge::opaque()` and `Paint::opaque()` fill over the window for anything floating over content. The compact bar pill, browser tab pill and queue chip are controls and move with buttons and rows.
4. **Buttons** (done): the old variants, the `outline` flag and `ButtonCustomVariant` are deleted, and `.accent()` callers use `.primary()`. The neutral soft button paints opaque over the window so its resting shadow does not show through.
5. **Rows and selection:** `ListItem`, menus and the chooser move onto `Paint`, collapsing the selected-row recipes.
6. **Clean-up:**
   - the remaining keycaps;
   - one divider recipe;
   - faint text in place of opacity text;
   - a check that rejects `washed()`/`opacity()` fills outside the kit.

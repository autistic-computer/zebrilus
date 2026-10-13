---
type: Design Plan
title: "Design system: tone, emphasis and ground"
description: "How zz chrome picks colors and shapes: every filled, tinted or outlined widget names a tone (what it means) and an emphasis (how loud it is), the theme turns the pair into one Paint on the ground it sits on, planes name an elevation, edges and shadows come from kit recipes that follow the look, and clippy rejects hand-mixed washes, opacities, borders and shadows outside the kit."
status: Built
resource: crates/zz-gpui-kit/src/foundation/variant.rs
tags:
- ui
- theme
- design-system
- zz-gpui-kit
- design-plan
timestamp: 2026-10-13T00:00:00Z
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
  - Primary's color comes from the theme's look: `primary = foreground | accent` (`PrimaryStyle`). It works like `selection = accent | wash` and defaults to foreground, so the main action is white on dark and black on light. The theme picks both per look (see [interface themes](/designs/interface-themes.md)).
  - Code says "this is the main action" and never picks the color itself.
- **`Emphasis`**, how loud it is: `Solid`, `Soft`, `Outline`, `Ghost`.
- **`theme.paint(tone, emphasis)`** returns a `Paint`:
  - fills for rest, hover, pressed, selected and current;
  - a border and its width;
  - text and selected text;
  - the `Ground` it sits on.

  `fill_at`, `border_at`, `text_at`, `muted_at` and `faint_at` read any `PaintState`: rest, hover, active (pressed), selected, current, focused, disabled.
- **`painted(paint, state)`** draws a paint on any element.
- **`painted_interactive(paint, selected, enabled)`** draws one on an element with an id and adds live hover and press. Hover only changes the fill, which GPUI can repaint; text color cannot follow a hover style.

## Recipes

| Emphasis | fill | text | border | hover / pressed |
|---|---|---|---|---|
| solid | the tone | readable on the fill | none | the fill's `hover()` / `active()` |
| soft | neutral: one wash step on the ground; other tones: the tone at 12% | neutral: the ground's ink; other tones: the tone mixed 30% toward the ink | the look's outline hairline, if it has one | the next wash or alpha step |
| outline | none | as soft | neutral: ink at 30%; other tones: the tone at 55%; 1px | soft's rest / hover |
| ghost | none | as soft | none | soft's hover / pressed |

**Selected and current** are two different meanings, the same split as HTML's `aria-selected` and `aria-current`:
- **Selected** is the row the cursor is on in a list you pick from: a menu, the palette, a picker, a select's options. It moves with the arrow keys and Enter acts on it. Neutral soft, outline and ghost on the window take `selection_background()`, so they follow the look's `selection`: accent fill or a wash. On any other ground, and for coloured tones, selected is the pressed fill.
- **Current** is where you are or what is switched on: the open settings page, a toggle button that is on, the active session row, a latched phone modifier, an open tray panel. It is always the pressed wash (the tone's `active()` for solid), whatever the look's `selection`, so chrome that stays in this state never carries the accent. `Button::selected` draws it.
- Text on an accent fill (a solid primary in the accent, an accent selection) is the theme's `on_accent` when it names one. Otherwise, and on any other opaque fill, it is picked by perceptual lightness (`readable_on`), so a light accent gets dark text and a dark accent gets white.

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

## Surfaces

`theme.surface(elevation)` is the ground of a plane at an `Elevation`:
- `Window`: the window itself, the same as `theme.ground()`;
- `Panel`: the window's fill raised one step, for a bar, a card or an inset box;
- `Popover`: two steps, for a menu, a popover or a sheet.

A surface floating over content takes its fill `.opaque()` (`Ground::opaque` keeps the elevation). `popover_style`, tooltips, sheets and notifications fill from it.

Surfaces take their shape from the kit's recipes in `foundation/styled.rs`, which read the look, so a theme's outline, corners and shadows reach every one of them:
- `card_style`: a raised panel that sits in the window: the agent's user message bubble, attachment tiles and Mermaid diagrams. The opaque panel fill, `surface_edge`, the theme radius and `contact_shadow`. Settings cards build the same edge themselves, `surface_edge` on the card's ground with `contact_shadow` behind the group.
- `floating_surface`: what everything floating over the window shares, without a shadow. The popover fill or the look's glass, `surface_edge`, the theme radius and the foreground ink.
- `popover_style`: `floating_surface` with `floating_shadow`, for menus, popovers, dialogs and sheets.
- `lifted_style`: `floating_surface` with `lifted_shadow`, for tooltips and toasts.
- `control_surface` and `control_surface_on(ground)`: a control's hairline at the outline width, and its shadow.

A ground remembers its `elevation`, and `paint.ground_at` passes it on. Controls cast their shadow only on the window, and only when the look's `shadows` is `all`: on a panel or a popover they keep the hairline and drop the shadow, so a stepper in a menu or a field on a settings card does not stack a third layer on a surface that already stands off the window. `control_highlight_on(ground)` and `control_surface_on(ground)` apply the rule; `Button` reads its `.on` ground.

GPUI draws a shadow under the whole box, not only outside it, so a translucent fill over a shadow turns muddy (a 12% wash over a 20% shadow reads as 30%). A control that casts a shadow composites its fill over its ground first.

A pane fills with `theme.pane_background(fill)`: its own fill (the window color, or a terminal's background) over the window, at the look's pane opacity.

## Lines and backdrops

- A rule between sections is `rule_top`, `rule_bottom`, `rule_left` or `rule_right`: `RULE_WIDTH` (1px) in `theme.divider_color()`, which is now just `theme.border()`. It replaces the separate rule recipes the audit found.
- The edge of a card or a floating surface is `surface_edge`: `theme.border()` at the look's outline width, and no edge at all when the outline is 0. `surface_outline(color)` is the same edge in another color. Glass draws its own rim, so a floating surface on glass hides the hairline and keeps its width.
- A focus ring is `focus_ring(&paint)`: `FOCUS_RING_WIDTH` (1.5px) in the paint's focus color. The chosen tile in a picker takes `chosen_ring`, 2px in the accent.
- Anything that dims the window behind a modal, a sheet or a slide-over fills with `theme.scrim`.
- Every drop shadow is drawn in `theme.shadow_color`, which the theme sets per mode, so a light theme on a tinted background can tint its shadows instead of graying them.
- The look's `shadows` (`zz_gpui_kit::Shadows`) says which surfaces cast one. `None` casts nothing. `Surfaces` gives cards, popovers, dialogs and floating panes theirs and keeps controls flat. `All` adds a small one under controls on the window. `Theme::surface_shadows` and `Theme::control_shadows` read it.
- The shadow recipes check it themselves and return no shadow when the look turns theirs off: `contact_shadow` is the tight shadow a pane or a card sits on, `control_shadow` the same under a control, `floating_shadow` the contact shadow with a deep, soft one under it for anything floating over the window, and `lifted_shadow` the small one under tooltips, toasts and switch thumbs.

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
- `NumberInput` takes the same three, and `Select` and `Button` take `.on(ground)` too.
- Settings controls sit on `settings_card_ground(cx)`, the opaque panel a settings card fills with, so their fills step up from the card instead of a hand-picked control fill.

The shared chrome, `field_chrome` in `input/field.rs`, does three things:
- it always draws at least a 1px border, clear when the paint has none, so focusing never moves the text;
- it hovers to the soft hover fill;
- it focuses to the focus color.

`input::field_chrome` and `input::field_paint` are public, for a field that holds more than its text. The agent composer is a field shell built this way, not a card: the neutral paint at the look's field emphasis around its attachments, input and controls, the theme radius, and the focus border while its input has focus.

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
- **`Keycap`**: a key as a cap. Neutral, outline by default, `.on(ground)` inside another paint.
- **`Kbd`**: a `Keystroke` formatted for the platform and drawn as a `Keycap`, neutral soft by default, with the same `.emphasis()` and `.on(ground)`. Menus and tooltips show a binding as text (`Kbd::text`) in the row's ink instead of a cap.
- **`Check`**: `Check::checkbox(checked)` and `Check::radio(checked)`.
  - Off: a neutral outline.
  - On: the tone solid (primary by default), with `.on(ground)`.
  - Corners: a checkbox at half the theme radius, a radio round.
- **`Button::tone(..)` / `.emphasis(..)`**: `ButtonVariant` is `Paint(Tone, Emphasis)`, `Link` or `Text`. The shorthands name the common pairs: `primary` (primary solid), `secondary` (neutral soft, the default), `ghost` (neutral ghost), `danger`, `warning` and `success` (soft). A dialog's destructive OK is `Paint(Danger, Solid)`.
- **Tiles and rows** are not widgets. A tile is `painted_interactive(theme.paint(Neutral, Soft), ..)`, a row the same with `Ghost`; `zz_ui::picker::list_row` builds a row at a given height. A list of rows takes `list_inset`: padded in by the look's row inset, with a hair of space between rows while they float. Rows round with `menu_item_corners(height)`, which reads `Theme::row_radius(height)` and goes square when the row inset is 0.
- **`touch::press_highlight`**: the press layer under a control on a touch screen. Without a color it paints the neutral ghost paint's pressed fill, and its corners clamp to its size.

**Corners:**
- Badges and keycaps round with `control_radius()`, like buttons: pills when the radius is above 24 (Full), the theme radius below that, square at 0.
- Checkboxes round at half the theme radius; radios are round.

# Rules

- Name a tone and an emphasis. Do not pick a fill with `washed()`, `raised()` or `opacity()` at a call site outside the kit. Clippy enforces it: the root `clippy.toml` disallows `Hsla::opacity` and the mixing transforms of `Colorize` (`raised`, `washed`, `wash`, `fill`, `outline`, `subtle`, `glow`, `hover`, `active`, `on`, `floating`). The kit's own `clippy.toml` leaves them out, since the kit is where the recipes are mixed. `muted()`, `faint()` and `opaque()` stay open.
- Draw edges and shadows with the kit's recipes. The root `clippy.toml` also disallows, outside the kit, `zz_gpui::Styled::border` and its sized and per-side forms (`border_1` to `border_4`, and `border_t`, `border_b`, `border_l`, `border_r`, `border_x` and `border_y` with their sizes), `shadow_2xs` through `shadow_2xl`, and the type `zz_gpui::BoxShadow`. The recipes (`control_surface`, `surface_edge`, `surface_outline`, `card_style`, `rule_top` and the other rules, `focus_ring`, `chosen_ring`, and the shadow functions) read the look, so a theme's outline and shadows reach every edge. The kit's `clippy.toml` keeps only the raw-color ban, since the recipes are built there.
- A color, edge or shadow that is deliberately not a recipe takes `#[allow(clippy::disallowed_methods, reason = ..)]` on its one function, or `clippy::disallowed_types` where it names `BoxShadow`. In chrome, today that is:
  - the theme previews in Appearance, and the focus halo and selection frame around its tiles;
  - the element picker overlay (page CSS) and the blurred chrome background;
  - the pane inactive dimming and drag veil, which scale by their own settings;
  - a floating pane's own border colour (`forced_edge` in `pane.rs`: the active accent or a tmux `popup-border-style`) and a tmux menu's literal one (`literal_edge` in `command/floating.rs`);
  - `pane_drop_preview`, which traces the target pane's own border width;
  - the window's drop shadow under client-side decorations (`window_shadow` in `crates/zz/src/window/frame.rs`);
  - the ring that cuts the agent trace count badge out of the icon it overlaps.

  The storybook adds `story::Framed::framed`, the edge of a frame that stands in for a window.
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
| Menu, list and chooser rows | neutral · ghost, selectable (`picker::list_row`) |
| Digit keycap on a tile | neutral · outline, on the tile's ground |
| Shortcut hint (`Kbd`) | neutral · soft; text in menus and tooltips |
| Phone key | neutral · soft opaque; current when latched; neutral · solid when held |
| Count or kind chip (`2/3`, `Base`) | neutral · soft |
| Reject, delete | danger · soft, or danger · solid when it is the main action |
| Failing or blocked status | danger · soft |
| Waiting status | warning · soft |
| Done or healthy status | success · soft |
| Multi-select mark | `Check::checkbox`, primary |
| Single-select mark | `Check::radio`, primary |
| Text, number, select field | neutral at the look's field emphasis |
| Invalid field | danger at the look's field emphasis |
| Inline field (rename in place) | the look's field emphasis, no focus ring |
| Agent composer | a field shell: `field_chrome` with the neutral field paint, not a card |
| User message bubble, attachment tile, Mermaid diagram | a card (`card_style`) |
| Browser address bar | neutral · ghost field |
| Browser tab | neutral · soft when active, ghost otherwise |
| Settings control | its own recipe, on `settings_card_ground` |
| Switch, slider | primary · solid when on, neutral soft when off |
| tmux display-menu | a popover menu; neutral · ghost rows; literal tmux colours override |
| Display-panes chip | popover surface; accent ring and ink when active |
| Pane status badge, pane overlay lines | `pane_chip`: neutral · soft opaque at the control radius with the control edge, and the focus ring when focused |
| Status dot (bell, activity, agent state, host) | `navigation::status_dot`: one 6px dot in the state's color; a working agent is the muted ink |
| A pane's failure (browser, editor, agent) | `inline_alert`: popover surface in the alert layout; primary · solid Try again last |

# Where to look

- `crates/zz-gpui-kit/src/foundation/variant.rs`: `Tone`, `Emphasis`, `Paint`, `Ground`, the recipes.
- `crates/zz-gpui-kit/src/foundation/styled.rs`: the edge, surface, rule, row, ring and shadow recipes.
- `crates/zz-gpui-kit/src/foundation/theme.rs`: `Shadows`, `CORNER_SMOOTHING`, and the look helpers (`outlined`, `surface_shadows`, `control_shadows`, `row_radius`, `inner_radius`).
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

1. **Agent cards** (done): `CardTile` paints tiles, keycaps and checks on its ground; `card_chip` is a `Badge`; Skip is neutral ghost and Submit primary solid. The error card is an `inline_alert` with its retry and sign-in buttons right-aligned.
2. **Fields** (done): `Input`, `NumberInput`, the `Select` trigger, `Switch` and `DiscreteSlider` draw from paints. The zz-ui fields that wrap a bare `Input` in their own chrome (the composer, the palette, the browser address bar, the agent title) still pick their colors and move with the clean-up.
3. **Badges** (done): `Tag` is deleted. `Badge` carries the editor's vim mode, settings provenance, the palette's mode, host and command pills, chooser and picker chips and the command kind; the palette shortcut is a `Keycap`. The agent status pill, the trace count, the plan chip and the pane's status badge, drag chip and overlay lines paint from tones too. `Badge::opaque()` and `Paint::opaque()` fill over the window for anything floating over content. The compact bar pill, browser tab pill and queue chip are controls and move with buttons and rows.
4. **Buttons** (done): the old variants, the `outline` flag and `ButtonCustomVariant` are deleted, and `.accent()` callers use `.primary()`. The neutral soft button paints opaque over the window so its resting shadow does not show through.
5. **Rows and selection** (done): a selected row is the neutral ghost paint's selected fill with its selected text. `selection_highlight` reads it, so `ListItem`, menus, the chooser, the palette, the picker and the slash menu all agree, and an accent selection gets text that reads on the accent. A hovered row that is not selected takes the ghost hover wash instead of the selection fill: GPUI repaints a hover's fill but not its text, so an accent hover would keep text picked for the window. Menus already move their selection with the pointer, so their hover becomes a real selection. The submenu parent and the agent picker's open provider row use the ghost hover, and `workspace_row_highlight` reads it too.
6. **Clean-up** (done):
   - `Kbd` draws through `Keycap`; menus and tooltips show bindings as text. The phone key row, popover keys, arrow pad, bar pill, bindings list and overview cards paint from neutral soft, selected and solid.
   - `Elevation` and `theme.surface()` name panels and popovers; `theme.pane_background()` names the pane fill.
   - Rules read `divider_color()`, edges `border()`, quiet text `faint()`, hover and press washes the ghost paint, danger callouts the danger paints, and backdrops the scrim.
   - The browser address bar is a ghost field and its tabs soft or ghost; `Button` and `Select` take a ground, and settings controls paint on the card's.
   - Clippy rejects hand-mixed fills outside the kit (see Rules).
7. **Shapes** (done, 2026-10-13):
   - Edges, rules, rings and shadows come from the kit's recipes, which read the look: `surface_edge`, `card_style`, `floating_surface`, `popover_style`, `lifted_style`, the `rule_*` family, `list_inset`, `menu_item_corners`, `focus_ring`, `chosen_ring`, and `contact_shadow`, `control_shadow`, `floating_shadow` and `lifted_shadow`.
   - Clippy rejects hand-drawn borders, the `shadow_*` presets and `BoxShadow` outside the kit (see Rules).
   - The agent composer is a field shell; the user message bubble, attachment tiles and Mermaid diagrams are cards.
   - Rows share `picker::list_row`, dots `navigation::status_dot`, and pane badges and overlay lines `pane_chip`. Menu separators are 1px rules.
   - The look shrank to nine keys to match (see [interface themes](/designs/interface-themes.md)).

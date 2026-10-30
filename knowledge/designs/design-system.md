---
type: Design Plan
title: "Design system: tone, emphasis and ground"
description: "How zz chrome picks colors and shapes: every filled, tinted or outlined widget names a tone (what it means) and an emphasis (how loud it is), the theme turns the pair into one Paint on the ground it sits on, planes name an elevation, every surface takes its fill, edge, corners and shadow from its surface role, and clippy rejects hand-mixed washes, opacities, borders and shadows outside the kit."
status: Built
resource: libs/penna/src/foundation/variant.rs
tags:
- ui
- theme
- design-system
- penna
- design-plan
timestamp: 2026-10-30T00:00:00Z
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

- **`Tone`**, what it means: `Neutral`, `Primary`, `Danger`, `Warning`, `Success`, `Accent`.
  - Primary's color comes from the theme's `primary` material: `foreground | accent` (`PrimaryStyle`). It defaults to foreground, so the main action is white on dark and black on light (see [interface themes](/designs/interface-themes.md)).
  - Code says "this is the main action" and never picks the color itself.
  - Accent is the theme's accent whatever `primary` says, for what marks your own things rather than the main action. The Driver Agent's chat washed your messages in it until it was removed on 2026-10-28.
- **`Emphasis`**, how loud it is: `Solid`, `Soft`, `Outline`, `Ghost`.
- **`theme.paint(tone, emphasis)`** returns a `Paint`:
  - fills for rest, hover, pressed, selected and current;
  - a border and its width;
  - text and selected text;
  - the `Ground` it sits on.

  `fill_at`, `border_at`, `text_at`, `muted_at` and `faint_at` read any `PaintState`: rest, hover, active (pressed), selected, current, focused, disabled.
- **`painted(paint, state)`** draws a paint on any element.
- **`painted_interactive(paint, selected, enabled)`** draws one on an element with an id and adds live hover and press. Hover only changes the fill, which ala can repaint; text color cannot follow a hover style.

## Recipes

| Emphasis | fill | text | border | hover / pressed |
|---|---|---|---|---|
| solid | the tone | readable on the fill | none | the fill's `hover()` / `active()` |
| soft | neutral: one wash step on the ground; other tones: the tone at 12% | neutral: the ground's ink; other tones: the tone mixed 30% toward the ink | the theme's outline, if it has one | the next wash or alpha step |
| outline | none | as soft | neutral: ink at 30%; other tones: the tone at 55%; 1px | soft's rest / hover |
| ghost | none | as soft | none | soft's hover / pressed |

**Selected and current** are two different meanings, the same split as HTML's `aria-selected` and `aria-current`:
- **Selected** is the row the cursor is on in a list you pick from: a menu, the palette, a picker, a select's options. It moves with the arrow keys and Enter acts on it. Neutral soft, outline and ghost on the window take `selection_background()`, the opaque accent. On any other ground, and for coloured tones, selected is the pressed fill.
- **Current** is where you are or what is switched on: the open settings page, a toggle button that is on, the active session row, a latched phone modifier, an open tray panel. As a paint it is always the pressed wash (the tone's `active()` for solid), so chrome that stays in this state never carries the accent. `Button::selected` draws it.
- A surface drawn through a role shows both states with its role's `selected` highlight (see [surface roles](/designs/surface-roles.md)). Menu items (`menu-item`, a button group) select with the accent and the places you stand in pick a lift or a tint (`row` and `tab`, item groups, and `button`), so the split holds in the Default theme, and a theme moves it per role, such as rows that select with a tint or the accent on the selected tab.
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

A surface floating over content takes its fill `.opaque()` (`Ground::opaque` keeps the elevation).

Surfaces take their fill, edge, corners and shadow from their **surface role**, described in [surface roles](/designs/surface-roles.md): `theme.layer(surface, state)` resolves the role's style against the theme's materials and returns one `Layer`, and `.surface(role)`, `.surface_at(role, state)`, `.surface_hover` and `.surface_interactive` paint it. Kit widgets take `.surface_role(role)` and draw as their kind without one. A `Surface` is a role with a tone, a `prominent` flag (the solid main action) and the ground it sits on (`.on(ground)`).

A ground remembers its `elevation` and whether it is glass, and `paint.ground_at` and `Layer::ground` pass them on. Controls (rows, tabs, chips, buttons, icon buttons, fields, badges, keycaps) cast their shadow only on the window: on a panel or a popover they keep the outline and drop the shadow, so a stepper in a menu or a field on a settings card does not stack a third layer on a surface that already stands off the window. Glass on a glass ground paints a tint instead.

ala draws a shadow under the whole box, not only outside it, so a translucent fill over a shadow turns muddy (a 12% wash over a 20% shadow reads as 30%). A surface that casts a shadow or shows a ring composites its fill over its ground first.

A pane paints no fill of its own: the window shows through it. `theme.pane_color(fill)` is the solid color of a pane's fill (the window color, or a terminal's background) over the window, which a PiP lays at `pip-background-opacity` (the `zz_ui::pip::PipOpacity` global).

## Lines and backdrops

- A rule between sections is `.rule_on(role, edge)`: `RULE_WIDTH` (1px) in `theme.divider_color()`, the foreground at the edge weight, translucent so a rule lightens glass in a dark theme the way it does a solid fill, drawn when the rule role's outline is on (`rule.bar`, `rule.sidebar`, `rule.split`, `rule.list`, `rule.tree`). It replaces the separate rule recipes the audit found.
- The edge of a surface is its role's `outline`: the theme's outline width, the border color on surfaces and 10% of the ground's ink on controls, and no edge at all when the outline is 0. Glass draws its own rim, so a surface in glass hides the hairline and keeps its width. Highlights recolor the edge and never change its width.
- A focus ring is `focus_ring(&paint)`: `FOCUS_RING_WIDTH` (1.5px) in the paint's focus color. A chosen tile is its role's `selected: ring`, 2px in the accent.
- Anything that dims the window behind a modal, a sheet or a slide-over fills with `theme.scrim`.
- Every drop shadow is drawn in `theme.shadow_color`, which the theme sets per mode, so a light theme on a tinted background can tint its shadows instead of graying them.
- The tint (`chrome-tint` and `chrome-tint-hue`, see [interface themes](/designs/interface-themes.md)) changes only the background root (`ThemeColor::tinted`). Planes, washes, edges and glass tints derive from the background, so they all lean with it and no widget has to know.
- Which surfaces cast a shadow is each role's `shadow` (`none`, `contact` or `float`); what the two shadows look like is the theme's `contact` and `float` materials, which the theme sets with the other materials. `Theme::shadows_of`, `contact_shadows`, `float_shadows` and `casts_shadow(role)` read them. `lifted_shadow` is the small shadow under a switch thumb, and it follows the contact strength.

## Text

| Level | Use | From |
|---|---|---|
| default | labels, body, and chrome while hovered, selected or current | the ground's ink (`foreground` on the window) |
| rest | chrome at rest: title bar buttons, window tabs, sidebar rows, host names | a role with `ink: rest` (`layer(.., Rest).text`), or `.rest()` on the window |
| muted | descriptions, secondary labels | `paint.muted_at(state)`, or `.muted()` on the window |
| faint | placeholders, hints | `.faint()` |
| on a fill | text on a solid or selected fill | `paint.text_at(state)` |

`muted_at` pulls the text toward the fill it actually sits on, so it reads on the window, a solid fill and an accent selection alike.

# Fields

Text, number and select fields are neutral paints at their field role's emphasis (`Theme::field_emphasis(role)`): soft while the role has a fill, outline when it has no fill but an outline, ghost when it has neither. The `field` kind is soft. Call sites never pick it; a theme does, per role.
- `Input::tone(Tone::Danger)` marks an invalid value: a danger fill or border, and a danger focus ring.
- `Input::emphasis(Emphasis::Ghost)` is for a field that shows its edge only on hover or focus.
- `Input::on(ground)` puts a field inside a tile or card on that surface's ground, and `Input::surface_role(role)` draws it as another field role.
- `NumberInput` takes the same three, and `Select` and `Button` take `.on(ground)` too.
- Settings controls sit on `settings_card_ground(cx)`, the opaque panel a settings card fills with, so their fills step up from the card instead of a hand-picked control fill.

The shared chrome, `field_chrome` in `input/field.rs`, takes a `Surface` or a role and does three things:
- it always draws at least a 1px border, clear unless the role draws one, so focusing never moves the text (a role with no fill and no outline draws no box);
- it hovers to the soft hover fill;
- it focuses to the focus color when the theme draws outlines (`Theme::outlined()`) and the field is not glass, whose rim is its edge; a bare field (no fill, no outline) still shows the focus frame then.

`input::field_chrome` and `input::field_paint` are public, for a field that holds more than its text. The agent composer is a field shell built this way, not a card: the `agent.composer` role around its attachments, input and controls, and the focus border while its input has focus.

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
- **`Badge`**: tone, emphasis, size, optional icon, any children after the label, and `.surface_role(role)`. `.opaque()` for a badge floating over content. One widget for chips, pills, status and counts.
- **`Keycap`**: a key as a cap, drawn as the `keycap` kind (a tint with the outline) or `.surface_role(role)`, with `.on(ground)` inside another paint and its label muted on the cap, or at full ink on an accent or other colored fill, where muting would wash it into the color.
- **`Kbd`**: a `Keystroke` drawn as that same `Keycap`, with the same `.surface_role()` and `.on(ground)`, so every key hint is one cap. Inside the cap the keys are `KeyGlyphs`: each key's icon where the platform has one (`key_icon`: Apple's ⌃ ⌥ ⇧ ⌘ ⎋ ⇥ on macOS and iOS, ISO 9995's ✲ and ↹ elsewhere, ↵ ⌫ and the arrows on both) and its name otherwise. The palette's footer keys are bare `KeyGlyphs` without a cap, and its rows' tmux shortcuts (`C-b c`) one small cap per key (`tmux_shortcut`). Menus and tooltips show a binding as text (`Kbd::text`) in the row's ink instead of a cap.
- **`Check`**: `Check::checkbox(checked)` and `Check::radio(checked)`.
  - Off: a neutral outline.
  - On: the tone solid (primary by default), with `.on(ground)`.
  - Corners: a checkbox at half the theme radius, a radio round.
- **`Button::tone(..)` / `.emphasis(..)`**: `ButtonVariant` is `Paint(Tone, Emphasis)`, `Link` or `Text`. The shorthands name the common pairs: `primary` (primary solid), `secondary` (neutral soft, the default), `ghost` (neutral ghost), `danger`, `warning` and `success` (soft). A dialog's destructive OK is `Paint(Danger, Solid)`.
- **Tiles and rows** are surfaces of the `tile` and `row` item groups, or the `menu-item` button group for rows you press, or a role in one of them. `penna::row_surface` draws a row of a given height as a role with its hover and press, and `penna::list_row` builds a list row on it. A list of rows takes `list_inset`: padded in by the theme's row inset, with a hair of space between rows while they float. Rows round with `row_corners(element, layer, height)`: the layer's radius held to 40% of the row's height, square when the row inset is 0, a pill when the role asks for one.
- **`ColorPicker`**: a swatch that opens a saturation and brightness field over a hue `Slider` and a hex field with a clear button, in a popover or, on a touch screen, a bottom sheet. It keeps the last hue picked while the color is gray or black, and `.swatch(size)` draws the trigger as a bare swatch to sit in a row of swatches.
- **`touch::press_highlight`**: the press layer under a control on a touch screen. Without a color it paints the neutral ghost paint's pressed fill, and its corners resolve through `Window::resolve_corner_radii`, the same curve the control's own corners take. Add it before the control's content: a role's pressed fill is often opaque, and painted last it hides the label.

**Corners:**
- Badges and keycaps round with their role's shape, like buttons: the theme radius for `round`, half circles for `pill`, square for `square`.
- Checkboxes round at half the theme radius; radios are round.
- Color swatches round with `Theme::swatch_radius()`: half the theme radius, so a swatch rounds in step with the controls beside it, square at radius 0. The window's adaptive corner cap keeps it from turning into a circle at large radii, like every other ordinary radius.

# Rules

- Name a tone and an emphasis. Do not pick a fill with `washed()`, `raised()` or `opacity()` at a call site outside the kit. Clippy enforces it: the root `clippy.toml` disallows `Hsla::opacity` and the mixing transforms of `Colorize` (`raised`, `washed`, `wash`, `fill`, `outline`, `subtle`, `glow`, `hover`, `active`, `on`, `floating`). The kit's own `clippy.toml` leaves them out, since the kit is where the recipes are mixed. `muted()`, `faint()` and `opaque()` stay open.
- Paint surfaces through a surface role: `.surface(role)`, `.rule_on(role, edge)`, or a widget's `.surface_role(role)`. The root `clippy.toml` disallows, outside the kit, `ala::Styled::border` and its sized and per-side forms (`border_1` to `border_4`, and `border_t`, `border_b`, `border_l`, `border_r`, `border_x` and `border_y` with their sizes), `shadow_2xs` through `shadow_2xl`, the type `ala::BoxShadow`, and the kit helpers that would get around the roles: `StyledExt::surface_edge`, `surface_outline` and `chosen_ring`, `lifted_shadow`, and `Theme::control_radius`, `menu_radius` and `row_radius`. The old recipes (`card_style`, `popover_style`, `control_surface`, `control_highlight`, `surface_fill`, `menu_item_corners`, `selection_highlight`, the `rule_*` family, `contact_shadow`, `control_shadow`, `floating_shadow`) are deleted. The kit's `clippy.toml` keeps only the raw-color ban, since the engine is built there.
- A piece of UI that is new and distinct gets its own role in `crates/zz-ui/src/roles.rs` instead of a local style (see [UI conventions](/configuration/ui-conventions.md)). Another app on penna registers its roles with `penna::roles::register` the same way, and draws them on a theme creator plate in its storybook catalog's `plates`, so a theme can be tuned for them ([storybook](/playbooks/storybook.md)).
- A color, edge or shadow that is deliberately not a role takes `#[allow(clippy::disallowed_methods, reason = ..)]` on its one function, or `clippy::disallowed_types` where it names `BoxShadow`. In chrome, today that is:
  - the theme previews in Appearance, and the focus halo around its tiles;
  - the element picker overlay (page CSS) and the blurred chrome background;
  - the pane inactive dimming and drag veil, which scale by their own settings;
  - a tmux menu's literal border colour (`literal_edge` in `command/floating.rs`);
  - the window's drop shadow under client-side decorations (`window_shadow` in `crates/zz/src/window/frame.rs`), cast only while the theme's popovers cast one;
  - the ring that cuts the agent trace count badge out of the icon it overlaps, and the ring in the row's ink that keeps a palette status dot legible on the selection (`ringed` in `command/palette.rs`).

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
| Text, number, select field | neutral at the field role's emphasis |
| Invalid field | danger at the field role's emphasis |
| Inline field (rename in place) | the field role's emphasis, no focus ring |
| Agent composer | a field shell: `field_chrome` as `agent.composer`, not a card |
| User message bubble, attachment tile, Mermaid diagram | `agent.bubble` (a card), `agent.attachment` (a tile), `figure` |
| Browser address bar | neutral · ghost field |
| Browser tab | neutral · soft when active, ghost otherwise |
| Settings control | its own recipe, on `settings_card_ground` |
| Switch, slider | primary · solid when on, neutral soft when off |
| tmux display-menu | a popover menu; neutral · ghost rows; literal tmux colours override |
| Display-panes chip | `pane.display-panes`: glass with the float shadow; accent ring when active or pointed at |
| Pane status badge, pane overlay lines | `pane.chip`: floating, with a contact shadow, and the focus ring when focused |
| Agent status (Running, Waiting for you, Exited) | `agent.status`: was a badge in the agent pane's header; the header went with the Agent pane on 2026-10-23 ([agent lens](/designs/agent-lens.md)) and the role was retired on 2026-10-27 |
| Status dot (bell, activity, agent state, host) | `navigation::status_dot`: one 6px dot in the state's color; a working agent is the muted ink |
| A pane's failure (browser, editor, agent) | `inline_alert`: the `inline-panel` role in the alert layout; primary · solid Try again last |

# Where to look

- `libs/penna/src/foundation/variant.rs`: `Tone`, `Emphasis`, `Paint`, `Ground`, the recipes.
- `libs/penna/src/foundation/surface.rs`: kinds, roles, styles, `Theme::style` and `Theme::layer`.
- `libs/penna/src/foundation/styled.rs`: `.surface`, `.surface_at`, `.layer`, `.rule_on`, `list_inset`, `focus_ring`, `row_corners` and `row_radius`.
- `libs/penna/src/foundation/theme.rs`: the material fields, `CORNER_SMOOTHING`, `casts_shadow`, `outlined`, `inset_rows`, `outer_radius` and `inner_radius`.
- `libs/penna/src/roles.rs`: the window chrome roles and the roles the kit's touch widgets, phone chrome (`penna::compact`), pickers and inline panels draw with; `crates/zz-ui/src/roles.rs`: zz's roles by region.
- `libs/penna/src/{badge,check,kbd}.rs`: `Badge`, `Check`, `Keycap`.
- The storybook's Foundation → Design system story (`#/design-system`) draws every recipe from the same code. Its sections are:
  - tones;
  - a tone × emphasis × state matrix;
  - buttons;
  - badges, keycaps and checks;
  - tiles and rows;
  - text levels;
  - the same paints on the window, a raised panel and a popover.

  The knob panel's Primary material switches the primary tone live.
- `penna_storybook::creator` (`libs/penna/storybook/src/creator/`): the theme creator, its plates for penna's own surfaces, and the parts app plates share.

# Migration

One component family at a time, each checked in the storybook and in the app:

1. **Agent cards** (done): `CardTile` paints tiles, keycaps and checks on its ground; `card_chip` is a `Badge`; Skip is neutral ghost and Submit primary solid. The error card is an `inline_alert` with its retry and sign-in buttons right-aligned.
2. **Fields** (done): `Input`, `NumberInput`, the `Select` trigger, `Switch` and `DiscreteSlider` draw from paints. The zz-ui fields that wrap a bare `Input` in their own chrome (the composer, the palette, the browser address bar, the agent title) still pick their colors and move with the clean-up.
3. **Badges** (done): `Tag` is deleted. `Badge` carries the editor's vim mode, settings provenance, the palette's mode, host and command pills, chooser and picker chips and the command kind; the palette shortcut is a `Keycap`. The agent status pill, the trace count, the plan chip and the pane's status badge, drag chip and overlay lines paint from tones too. `Badge::opaque()` and `Paint::opaque()` fill over the window for anything floating over content. The compact bar pill, browser tab pill and queue chip are controls and move with buttons and rows.
4. **Buttons** (done): the old variants, the `outline` flag and `ButtonCustomVariant` are deleted, and `.accent()` callers use `.primary()`. The neutral soft button paints opaque over the window so its resting shadow does not show through.
5. **Rows and selection** (done): a selected row is the neutral ghost paint's selected fill with its selected text, so `ListItem`, menus, the chooser, the palette, the picker and the slash menu all agree, and an accent selection gets text that reads on the accent. A hovered row that is not selected takes the ghost hover wash instead of the selection fill: ala repaints a hover's fill but not its text, so an accent hover would keep text picked for the window. Menus already move their selection with the pointer, so their hover becomes a real selection. The submenu parent and the agent picker's open provider row use the ghost hover. Since step 8 rows draw through a role: `menu-item` for rows you press, `row` for rows that show which one is current.
6. **Clean-up** (done):
   - `Kbd` draws through `Keycap`; menus and tooltips show bindings as text. The phone key row, popover keys, arrow pad, bar pill, bindings list and overview tiles paint from neutral soft, selected and solid.
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
8. **Surface roles** (done, 2026-10-13, same day): the recipes of step 7 are deleted. Every surface names a role and paints through `Theme::layer`; the look became materials plus per-role styles, and clippy also rejects the leftover edge, ring, shadow and radius helpers outside the kit. See [surface roles](/designs/surface-roles.md).

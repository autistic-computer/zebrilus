---
type: Design Plan
title: "The kit: penna owns every piece of UI chrome"
description: "Apps compose penna components and stop defining surface roles, theme patches, text sizes, spacing and colors. Tokens (Space, TextStyle) are fixed ratios measured in rems. Each app role becomes a kit recipe picked by a typed variant, and every per-theme tweak an app layer made moves into penna's own theme files. Work lands in vertical slices: a component family, its stories, then both apps in their pull."
status: In progress (2026-11-01, nidus done, zebrilus roles and typed looks in penna, zebrilus sites next)
resource: src/foundation/space.rs
tags:
- ui
- theme
- design-system
- design-plan
timestamp: 2026-11-01T00:00:00Z
---

# The kit

## Layers

- **ala** draws elements: div, text shaping, images, canvas, lists, focus, actions, drag.
- **penna foundation** holds the tokens (`Space`, `TextStyle`), the paint model (`Tone`, `Emphasis`, `Paint`, `Ground`), surface roles and the three themes.
- **penna components** put those together: `Label`, `Button`, `Badge`, `Popover` and the rest.

There is no separate behavior layer. ala's stateful elements already give hover, press, focus and keyboard activation, and penna already shares press feedback (`touch/press.rs`) and swipe dismissal (`dismissal.rs`). A piece of behavior moves into a shared module only when a second component needs it.

## Rules

1. Only penna defines surface roles, theme patches, colors, text styles and spacing.
2. Components take typed variants (`.tone()`, `.emphasis()`, `.size()`, `.shape()` and looks of their own such as `TabLook::Window`), content and behavior. No public `.surface_role()` is left once the slice that replaces its last consumer lands.
3. Apps own data, behavior, layout geometry and content: terminal cells, the agent transcript, where computers sit on the arrangement, colors that tmux names, the pane frame's material. Content code may carry `#[allow(clippy::disallowed_methods, reason = "...")]`.
4. A component is in the kit when penna's storybook shows every state of it and it has been looked at on each theme in light and dark.
5. penna changes its API freely. The commit that pulls penna into an app fixes that app. No deprecated paths.
6. Repo text, docs and commits never name other UI libraries as models.

## Tokens

Both scales are fixed. A theme changes fonts, materials and recipes, not the scale. Values are in rems, so the interface font size (the root rem, from the theme's `font_size`) scales them; window zoom is applied separately and never twice.

`Space` converts into a length, so it goes straight into ala's setters: `h_flex().gap(Space::S4).p(Space::S6)`.

| Space | S1 | S2 | S3 | S4 | S5 | S6 | S8 | S12 | S16 |
|---|---|---|---|---|---|---|---|---|---|
| px | 2 | 4 | 6 | 8 | 10 | 12 | 16 | 24 | 32 |

`TextStyle` sets size, line height and weight, and `Mono` also the code font. `Label::new(style, text)` draws a line of text; `.type_style(style, cx)` on a container sets what its text children inherit, and `.type_style_strong(style, cx)` the same at the style's strong weight, for a container whose text comes from its caller.

| TextStyle | Micro | Caption | Small | Body | Lead | Heading | Title | Display | Mono |
|---|---|---|---|---|---|---|---|---|---|
| size | 10 | 11 | 12 | 13 | 14 | 16 | 20 | 26 | 13 |
| line height | 14 | 16 | 16 | 18 | 20 | 22 | 26 | 32 | 18 |
| weight | normal | normal | normal | normal | normal | semibold | semibold | semibold | normal |

`Label::strong()` takes the small styles to medium. `muted()` and `faint()` mix the ink toward the ground the label sits on, so a label on a card passes `.on(ground)`.

Off-scale values in the apps fold by purpose, not by nearest number: 8.5 and 9 become Micro, 11.5 Caption, 12.5 Small, 15 to 18 Heading, 21 Title. Spacing of 1, 3, 5, 7 and 9 folds by eye; 14, 18, 20, 22 and 28 do too. Hairlines, optical offsets and arrangement geometry are not spacing and stay as they are. A migration commit that changes a size says so; one that only swaps a literal for the same token value says that instead.

## Themes and platforms

`default`, `macos` and `yaru` are themes (looks), not operating systems: any of them runs anywhere. A theme's tweak to a piece of UI lives in `themes/*.json` under that piece's recipe id and goes through the same resolver as today: kind defaults, then built-in recipe patches, then the theme's patches from the kind inward.

Every per-theme tweak an app layer makes today moves into penna's theme files under the recipe that replaces the app role, so nothing an app tuned is lost, and the other app gets the same look. Real operating system differences (fonts, key glyphs, window border) stay in penna behind `cfg!(target_os)`.

## Looks

A component takes a look: a typed variant that picks one penna recipe. Every look converts into a `Surface` (`From<XLook> for Surface`), so the readers below take a look wherever they take a surface.

| Component | Looks | Recipes |
|---|---|---|
| `Button::look` | `ButtonLook::Icon`, `Quiet`, `Flat`, `Chrome`, `Bar`, `Toggle`, `RowIcon`, `Floating`, `Row`, `Send`, `FloatingChip` | `icon-button`, `button.quiet`, `icon-button.flat`, `chrome.button`, `icon-button.bar`, `icon-button.toggle`, `icon-button.row`, `icon-button.floating`, `button.row`, `button.send`, `chip.floating` |
| `Panel::new(id, look)` | `PanelLook::Popover`, `Inline`, `Picker`, `Toast`, `Plain` | `popover`, `inline-panel`, `picker`, `toast`, `panel` painted opaque |
| `Card::new(id, look)` | `CardLook::Bubble`, `Code`, `Figure`, `Image` | `card.bubble`, `card.code`, `card.figure`, `card.image` |
| `Bar::new(id, look)` | `BarLook::Plain`, `Pane`, `Window` | `bar`, `bar.pane`, `titlebar` |
| `Tab::new(id, look)` | `TabLook::Window`, `Page`, `Tray`, `Title` | `tab.window`, `tab.page`, `tab.tray`, `button.title` |
| `Row::new(id, look)`, `ListItem::look` | `RowLook::Menu`, `MenuCard`, `MenuTint`, `Sidebar`, `Nav`, `Cursor` | `menu-item`, `menu-item.card`, `menu-item.tint`, `sidebar.row`, `row.nav`, `row.cursor` |
| `Chip::look` | `ChipLook::Plain`, `Bar`, `Floating`, `Marker`, `Drag`, `Icon` | `chip`, `chip.bar`, `chip.floating`, `chip.marker`, `chip.drag`, `badge.icon` |
| `Tile::variant` | `TileVariant::Card`, `Screen`, `Drop`, `Veil`, `Mini` | `tile.card`, `tile.screen`, `tile.drop`, `tile.drop` at hover with no edge, `tile.mini` |
| `Separator::look`, `.rule(look, edge, cx)` | `RuleLook::Plain`, `Bar`, `List`, `Tree` | `rule`, `rule.bar`, `rule.list`, `rule.tree` |
| `field_chrome`, `field_underlay` | `FieldLook::Composer`, `Address` | `field.composer`, `field.address` |

How they behave:

- Panel, Card, Bar, Tab, Row, Chip and Tile share one crate-private plate, a `Stateful<Div>` that paints the look first, so the caller's `Styled` calls land on top: a slide-over's `.rounded(px(0.0))`, a mini tile's `.bg`, a drop zone's corners. Tile painted after the caller before; it now paints first like the rest.
- Tab and Row are interactive by default, Chip with `.interactive(true)`: they add the hover and active looks while the state is Rest or Focused, and `.interactive(false)` paints the state alone. They take the element's one hover slot, so a caller's own `.hover()` on them is replaced; a text hover goes on a child or a group.
- `Row::row_height(height)` sizes the corners for a row that tall, as `ListItem` does at 28px by default; it does not set the height. Without it a Row keeps its look's corners. With `.hover_group(group)` a Row is a fill inside a taller row that lights when the group is pointed at, and on a coarse pointer it draws penna's press highlight. `Row::radius(px)` sets a round look's corner radius on the fill and its press highlight both; square and pill looks keep their corners.
- `Chip::new(id, label)` takes an id; `Chip::empty(id)` draws only its children, which always follow the label. `.ring(color)` recolors the ring its look draws, for a colour tmux names.
- `Panel::inset(px)` grows a rounded corner by the inset its rows sit at; square and pill corners stay.
- `Tile::edge(width)` draws the look's edge at that width where the theme draws one, so a drop zone can match the edge of the frames it lands in.
- `Badge::on(ground)` paints the badge on that ground, and `opaque()` blends over it.
- `BottomSheet` headers are `phone.bar`, `sheet_row` rows `phone.row`. `TextView` paints code blocks and inline code as `card.code` and tables as `card.figure`.

### Reading a look

Apps read a ground or a radius, never a `Layer`, so nothing paints through a reader.

- `theme.ground_of(look)` and `theme.ground_at(look, state)`: what a label or a button on that surface passes to `.on()`. `Surface::from(look).tone(tone).on(ground)` composes a toned look on a ground. Both work on a bare `&Theme`.
- `theme.radius_of(look)`: the look's corner radius, to nest corners inside it.
- `muted_on(ground, cx)`: the muted ink a label at rest takes on that ground.
- Shorthands: `Well::ground`, `Notice::ground`, `settings::settings_card_ground`, `sidebar::sidebar_fade(ground, cx)` for the sidebar fill over a ground, `compact::pill_backdrop_reach(cx)` for how far the phone pill's glass reads the backdrop.

## Recipes

An app role becomes a kit recipe: a penna-owned role picked by a component's variant, named for the kit (`tab.window`, `field.composer`, `icon-button.send`), not for the app. When a recipe lands, the app ids it replaces go into penna's retired list, so saved and pasted themes still load; a custom override a user saved for a retired id resets.

### zebrilus: 43 roles, now 0

Every zz role moved into penna (2026-11-01): 31 became kit recipes under kit names with their code patches and per-theme tweaks, 11 that had no patch and no tweak folded into the kit role they sat under, and `agent.action` merged into `icon-button.row`, which had the same look. Their old ids are in penna's retired list. penna has the typed looks that pick them (the Becomes column); zz code still picks most recipes by name (`roles::TAB_WINDOW`) until its sites move over.

Theme tweaks: D default, M macos, Y yaru. A dash means none.

| Role | Parent | Code patch | Theme tweaks | Becomes | Status |
|---|---|---|---|---|---|
| titlebar.window-tab | tab | ink rest | D hover/selected tint; M smoothing 2.5, selected glass; Y hover/selected tint | `TabLook::Window` | recipe `tab.window` |
| browser.tab | tab | - | M selected tint; Y hover/selected tint | `TabLook::Page` | recipe `tab.page` |
| agent.tray | tab | - | M hover/selected tint | `TabLook::Tray` | recipe `tab.tray` |
| titlebar.status-chip | badge | selected tint | M hover/selected lift | `ChipLook::Bar` | recipe `chip.bar` |
| pane.chip | floating | contact shadow, selected tint, capsule | M solid, outline | `ChipLook::Floating`, `ButtonLook::FloatingChip` when pressed | recipe `chip.floating` |
| pane.display-panes | floating | hover/selected ring | M hover tint, selected accent | `ChipLook::Marker` | recipe `chip.marker` |
| pane.drag-chip | floating | capsule | M solid, outline | `ChipLook::Drag` | recipe `chip.drag` |
| browser.empty-icon | badge | pill, no outline | - | `ChipLook::Icon` | recipe `badge.icon` |
| sidebar.entry | row | ink rest | D hover/selected tint; M smoothing 2.5, hover tint, selected glass; Y hover/selected tint | `RowLook::Nav` | recipe `row.nav` |
| sidebar.cursor | row | outline, contact shadow, hover tint, selected accent | - | `RowLook::Cursor` | recipe `row.cursor` |
| sidebar.host-action | button | no outline, no shadow | - | `ButtonLook::Row` | recipe `button.row` |
| sidebar.row-action | icon-button | hover/selected none | - | `ButtonLook::RowIcon` | recipe `icon-button.row` |
| pane.header.button | icon-button | hover/selected none | M, Y hover/selected tint | `ButtonLook::Bar` | recipe `icon-button.bar` |
| pane.header.flip | pane.header.button | selected tint | - | `ButtonLook::Toggle` | recipe `icon-button.toggle` |
| browser.button | icon-button | - | - | `ButtonLook::Icon` | folded into `icon-button` |
| agent.action | icon-button | hover/selected none | - | `ButtonLook::RowIcon` | recipe `icon-button.row` |
| agent.jump | icon-button | solid, outline, contact shadow, hover tint | M solid | `ButtonLook::Floating` | recipe `icon-button.floating` |
| agent.send | button | solid, outline, contact shadow, hover/selected tint | D tint, no outline, no shadow, pill, hover lift; M round, smoothing 2; Y pill | `ButtonLook::Send` | recipe `button.send` |
| pane.float | floating | selected ring | - | content: tmux panes paint their own material | recipe `floating.pane` |
| pane.pip | pane.float | float shadow | - | content: a PiP is frosted in its pane's colour | recipe `floating.pip` |
| pane.find-bar | popover | - | - | `PanelLook::Popover` | folded into `popover` |
| palette | popover | - | - | `PanelLook::Popover` | folded into `popover` |
| pane.picker | menu-item | - | M solid, selected tint | `RowLook::MenuCard` | recipe `menu-item.card` |
| agent.option | menu-item | - | - | `RowLook::Menu` | folded into `menu-item` |
| browser.suggestion | menu-item | tint | M tint | `RowLook::MenuTint` | recipe `menu-item.tint` |
| drawer.hosts | drawer | - | - | `BottomSheet` | folded into `drawer` |
| drawer.add-host | drawer | - | - | `BottomSheet` | folded into `drawer` |
| drawer.new-pane | drawer | - | - | `BottomSheet` | folded into `drawer` |
| drawer.tabs | drawer | - | - | `BottomSheet` | folded into `drawer` |
| drawer.bindings | drawer | - | - | `BottomSheet` | folded into `drawer` |
| pane-stack | tile | no outline, no shadow, hover/selected tint, capsule | D smoothing 3; M contact shadow, smoothing 3; Y contact shadow | content: stack tiles light, dim and show program faces | recipe `tile.stack` |
| pane.drop-zone | floating | tint, no shadow, hover/selected tint | M no shadow | `TileVariant::Drop`, `TileVariant::Veil` | recipe `tile.drop` |
| phone.mini | tile | no fill, selected ring | - | `TileVariant::Mini` | recipe `tile.mini` |
| pane.header | bar | fixed knobs | - | `BarLook::Pane` | recipe `bar.pane` |
| pane.header.title | button.quiet | hover/selected tint | - | `TabLook::Title` | recipe `button.title` |
| rule.split | rule | - | - | `RuleLook::Plain` | folded into `rule` |
| agent.bubble | card | - | M, Y tint, no outline, no shadow | `CardLook::Bubble` | recipe `card.bubble` |
| code | card | no outline, no shadow | M, Y tint, no outline, no shadow | `CardLook::Code` | recipe `card.code` |
| figure | card | no fill | - | `CardLook::Figure` | recipe `card.figure` |
| agent.attachment | card | - | M tint | `CardLook::Image` | recipe `card.image` |
| agent.composer | field.multiline | pill | D hover/selected none; M smoothing 2.5, hover/selected none; Y tint, outline, hover/selected none | `FieldLook::Composer` | recipe `field.composer` |
| browser.address | field | no fill, no outline | D, M selected tint | `FieldLook::Address` | recipe `field.address` |
| picker.notice | bar | - | - | `BarLook::Plain` on the picker's ground | folded into `bar` |

### nidus: 8 roles, now 0

nid-app registers no roles and its clippy list carries the kit lint. Its settings previews moved to Well, the menu bar panel to `tray::TrayPanel` with `ListItem` rows.

| Role | Parent | Code patch | Becomes | Status |
|---|---|---|---|---|
| status | badge | selected tint | Chip (`chip`) | done |
| board.chip | badge | selected tint | Chip (`chip`) | done |
| first-run.step | badge | selected accent | Steps (`step`) | done |
| banner | card | no shadow | Notice (`notice`) | done |
| computer-card | sidebar | - | Inspector (`inspector`) | done |
| board | card | no shadow | Well (`well`, now a tint without outline) | done |
| board.screen | card | contact shadow | Tile, Screen variant (`tile.screen`) | done |
| board.found | card | - | Tile, Card variant (`tile.card`) | done |

## Slices

Each slice lands in penna (component, variants, recipes, theme tweaks, stories), then in each app through `just libs pull penna` plus a fix commit, then the app's roles retire.

1. Tokens and `Label` (this doc).
2. Chip, Steps and Notice; nidus status pill, board chips, first-run steps, banners and the panel's status card. The `chip` and `step` recipes are pills: nidus asked for `rounded_full()` before, which the role's radius overrode. `feedback::callout` and `inline_alert` stay until zebrilus's notice batch moves to Notice.
3. Inspector, Well, Tile and Dock; the rest of nid-app's chrome; nidus lint on. The well is a tinted recess and tiles are painted on its ground (`Tile::on(Well::ground(cx))`), so screens stand apart from it on every theme; as plain cards both had the same fill wherever a theme turns outlines off.
4. zebrilus families, one integration each: icon buttons, chips, drawers, floating panels, tabs, menu items, canvas, bars and rules, content, rows, fields, notice; zebrilus lint on. The penna side landed first (2026-11-01): every look under Looks, the readers and their stories, with the old `.surface_role` builders still public until the sites move.
5. Close registration (done 2026-11-01): `SurfaceRole` has a private field and crate-private constructors and builders, so no other crate can make or restyle a role; `penna::register`, `roles::register`, `roles::retire` and app theme layers are gone; the retired list is penna's. Field looks the storybook used to build by hand are `Input::emphasis`.

## Lint

Defining a role is a compile error outside penna. The lint covers painting and text by hand.


Each app extends the `disallowed-methods` list it already has (`nidus/crates/nid-app/clippy.toml`, `zebrilus/clippy.toml` at the workspace root, which also covers `clients/storybook`; a per-crate file would replace the root list). Added paths: `penna::StyledExt::{surface, surface_at, layer, rule_on}`, `penna::Theme::layer`, `penna::InteractiveSurfaceExt::surface_hover`, `penna::StatefulSurfaceExt::surface_interactive`, `penna::row_surface`, `ala::Styled::{text_size, text_xs, text_sm, text_base, text_lg, text_xl, text_2xl, text_3xl, font_weight}` and the `penna::StyledExt::font_*` weight helpers. `penna::rems_from_px` is not on the list: sizes use it. Both apps carry the lint. zebrilus warns at every site its slice 4 has not moved onto a component yet; content code allows it on the narrowest fn with a reason. The lint is checked once by hand when added, on Linux and on a Mac (macOS-only code is not compiled on Linux). Spacing setters stay open: `gap(Space::S4)` and `gap(px(10.))` are the same call, so Space use is a review rule.

## How to work

- Kit slices happen in a worktree under `~/dev/worktrees/penna/<name>` and merge to penna's main. Look at the worktree's own storybook: `storybook/build.sh`, then serve `storybook/dist`. `just storybook penna` in zebrilus builds zebrilus's copy, not the worktree.
- Checks: everything `just libs check penna` in zebrilus runs, which covers all features, no default features, the storybook natively and for wasm.
- Library pushes and pulls go one at a time across both apps.

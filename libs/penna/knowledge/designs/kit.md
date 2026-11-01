---
type: Design Plan
title: "The kit: penna owns every piece of UI chrome"
description: "Apps compose penna components and stop defining surface roles, theme patches, text sizes, spacing and colors. Tokens (Space, TextStyle) are fixed ratios measured in rems. Each app role becomes a kit recipe picked by a typed variant, and every per-theme tweak an app layer made moves into penna's own theme files. Work lands in vertical slices: a component family, its stories, then both apps in their pull."
status: In progress (2026-11-01, nidus done, zebrilus next)
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
2. Components take typed variants (`.tone()`, `.emphasis()`, `.size()`, `.shape()` and enums of their own such as `TabVariant::Window`), content and behavior. No public `.surface_role()` is left once the slice that replaces its last consumer lands.
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

`TextStyle` sets size, line height and weight, and `Mono` also the code font. `Label::new(style, text)` draws a line of text; `.type_style(style, cx)` on a container sets what its text children inherit.

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

## Recipes

An app role becomes a kit recipe: a penna-owned role picked by a component's variant, named for the kit (`tab.window`, `field.composer`, `icon-button.send`), not for the app. When a recipe lands, the app ids it replaces go into penna's retired list, so saved and pasted themes still load; a custom override a user saved for a retired id resets.

### zebrilus: 43 roles

Theme tweaks: D default, M macos, Y yaru. A dash means none.

| Role | Parent | Code patch | Theme tweaks | Becomes | Status |
|---|---|---|---|---|---|
| titlebar.window-tab | tab | ink rest | D hover/selected tint; M smoothing 2.5, selected glass; Y hover/selected tint | Tab variant | todo |
| browser.tab | tab | - | M selected tint; Y hover/selected tint | Tab variant | todo |
| agent.tray | tab | - | M hover/selected tint | Tab variant | todo |
| titlebar.status-chip | badge | selected tint | M hover/selected lift | Chip | todo |
| pane.chip | floating | contact shadow, selected tint, capsule | M solid, outline | Chip | todo |
| pane.display-panes | floating | hover/selected ring | M hover tint, selected accent | Chip | todo |
| pane.drag-chip | floating | capsule | M solid, outline | Chip | todo |
| browser.empty-icon | badge | pill, no outline | - | Chip | todo |
| sidebar.entry | row | ink rest | D hover/selected tint; M smoothing 2.5, hover tint, selected glass; Y hover/selected tint | Row | todo |
| sidebar.cursor | row | outline, contact shadow, hover tint, selected accent | - | Row | todo |
| sidebar.host-action | button | no outline, no shadow | - | Button variant | todo |
| sidebar.row-action | icon-button | hover/selected none | - | IconButton variant | todo |
| pane.header.button | icon-button | hover/selected none | M, Y hover/selected tint | IconButton variant | todo |
| pane.header.flip | pane.header.button | selected tint | - | IconButton variant | todo |
| browser.button | icon-button | - | - | IconButton variant | todo |
| agent.action | icon-button | hover/selected none | - | IconButton variant | todo |
| agent.jump | icon-button | solid, outline, contact shadow, hover tint | M solid | IconButton variant | todo |
| agent.send | button | solid, outline, contact shadow, hover/selected tint | D tint, no outline, no shadow, pill, hover lift; M round, smoothing 2; Y pill | IconButton variant | todo |
| pane.float | floating | selected ring | - | FloatingPanel variant | todo |
| pane.pip | pane.float | float shadow | - | FloatingPanel variant | todo |
| pane.find-bar | popover | - | - | FloatingPanel variant | todo |
| palette | popover | - | - | FloatingPanel variant | todo |
| pane.picker | menu-item | - | M solid, selected tint | MenuItem variant | todo |
| agent.option | menu-item | - | - | MenuItem variant | todo |
| browser.suggestion | menu-item | tint | M tint | MenuItem variant | todo |
| drawer.hosts | drawer | - | - | Drawer | todo |
| drawer.add-host | drawer | - | - | Drawer | todo |
| drawer.new-pane | drawer | - | - | Drawer | todo |
| drawer.tabs | drawer | - | - | Drawer | todo |
| drawer.bindings | drawer | - | - | Drawer | todo |
| pane-stack | tile | no outline, no shadow, hover/selected tint, capsule | D smoothing 3; M contact shadow, smoothing 3; Y contact shadow | Well | todo |
| pane.drop-zone | floating | tint, no shadow, hover/selected tint | M no shadow | Tile variant | todo |
| phone.mini | tile | no fill, selected ring | - | Tile variant | todo |
| pane.header | bar | fixed knobs | - | Bar | todo |
| pane.header.title | button.quiet | hover/selected tint | - | Bar title | todo |
| rule.split | rule | - | - | Rule variant | todo |
| agent.bubble | card | - | M, Y tint, no outline, no shadow | Bubble | todo |
| code | card | no outline, no shadow | M, Y tint, no outline, no shadow | CodeBlock | todo |
| figure | card | no fill | - | Figure | todo |
| agent.attachment | card | - | M tint | ImageTile | todo |
| agent.composer | field.multiline | pill | D hover/selected none; M smoothing 2.5, hover/selected none; Y tint, outline, hover/selected none | Field variant | todo |
| browser.address | field | no fill, no outline | D, M selected tint | Field variant | todo |
| picker.notice | bar | - | - | Notice | todo |

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
4. zebrilus families, one integration each: icon buttons, chips, drawers, floating panels, tabs, menu items, canvas, bars and rules, content, rows, fields, notice; zebrilus lint on.
5. Close registration: `SurfaceRole` fields and constructors become crate-private, `penna::register` goes away, the retired list is penna's.

## Lint

Each app extends the `disallowed-methods` list it already has (`nidus/crates/nid-app/clippy.toml`, `zebrilus/clippy.toml` at the workspace root; a per-crate file would replace the root list). Added paths: `penna::SurfaceRole::new`, `penna::SurfaceRole::under`, `penna::register`, `penna::roles::register`, `penna::StyledExt::{surface, surface_at, layer, rule_on}`, `penna::Theme::layer`, `penna::InteractiveSurfaceExt::surface_hover`, `penna::StatefulSurfaceExt::surface_interactive`, `penna::row_surface`, `ala::Styled::{text_size, text_xs, text_sm, text_base, text_lg, text_xl, text_2xl, text_3xl, font_weight}`, the `penna::StyledExt::font_*` weight helpers and `penna::rems_from_px`. The lint is checked once by hand when added, on Linux and on a Mac (macOS-only code is not compiled on Linux). Spacing setters stay open: `gap(Space::S4)` and `gap(px(10.))` are the same call, so Space use is a review rule.

## How to work

- Kit slices happen in a worktree under `~/dev/worktrees/penna/<name>` and merge to penna's main. Look at the worktree's own storybook: `storybook/build.sh`, then serve `storybook/dist`. `just storybook penna` in zebrilus builds zebrilus's copy, not the worktree.
- Checks: everything `just libs check penna` in zebrilus runs, which covers all features, no default features, the storybook natively and for wasm.
- Library pushes and pulls go one at a time across both apps.

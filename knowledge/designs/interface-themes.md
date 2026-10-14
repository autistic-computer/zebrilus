---
type: Design Plan
title: "Interface themes: materials, roles and colors in one file"
description: "A theme is one JSON file holding its materials, how each piece of UI uses them (its roles), and the colors it is drawn in for light and for dark, so it can imitate a whole app; the mode only picks the color set. Default, macOS, Tahoe, Linear and Glass ship built in; the 34 palette presets, Flat/Modern/Full, the chrome color overrides and the nine-key look are gone."
status: Built
resource: crates/zz-ui/src/interface_theme.rs
tags:
- ui
- theme
- design-system
- configuration
- design-plan
timestamp: 2026-10-13T00:00:00Z
---

# Why

Before this, the Interface page had two independent pickers: a style (Flat, Modern, Full) and a palette per mode, from 34 presets ported from editor themes. The style's recipes guessed every color they needed from whichever palette came along:

- text on the accent was picked by lightness alone, so a mid-light accent got dark text;
- every shadow was the black scrim, so a cream light background got gray smudges under each card;
- panels and hover fills were the background nudged toward black or white, fine for a neutral gray and odd for a tinted one.

The editor palettes were made for syntax colors and gave six roots; everything else was a guess that only held for neutral palettes. Pairing any style with any palette produced combinations nobody designed.

# The model

A theme (`InterfaceTheme`) is one object:

- `name`: what Settings calls it;
- `materials`: the `Materials`, every value that is not a color, defined once: `radius`, `outline` (the width of every outline; 0 draws none), `row-inset`, `primary` (`foreground` or `accent`), `font`, `glass`, and the two shadows `contact` and `float`, each `y`, `blur`, `spread` and `strength`;
- `roles`: how the theme changes each kit kind and each zz role, by id, as a patch of the six style props (`fill`, `outline`, `shadow`, `shape`, `hover`, `selected`); what it leaves out draws as built in. The model is in [surface roles](/designs/surface-roles.md);
- `light` and `dark`: a `ThemeColors` each, the six roots plus `on-accent` (text on an accent fill) and `shadow-color` (the ink of every drop shadow).

The mode (`theme-mode`, or the OS appearance on System) only picks which color set applies. Materials and roles never pick a color, and a color set never changes the shape. Every other color is still derived from the roots at paint time, as the [design system](/designs/design-system.md) describes.

A file can leave keys out: materials fall back to `Materials::default()` (Default's, without glass), roles to their built-in styles, and a color to the built-in palette for the mode. Glass is a preset name or a preset with the fields it changes, and a shadow object names only the fields it changes, so a hand-written theme stays short. A key the format does not have, such as the removed `look`, is an error that names it, and a role id `zz_ui::roles::find` does not know is an error that suggests the one meant, so a file written for the older format loads only once it is rewritten (see [app config](/configuration/app-config.md)).

# Built-ins

`crates/zz-ui/themes/*.json`, compiled in and parsed by the same code as a user's file:

| Theme | Materials and roles | Colors |
|---|---|---|
| Default | 24px corners, 0.5px outlines, both shadows at full strength, an opaque glass edge, foreground primary, 4px row inset; no role patches, so every piece draws as its kind's built-in: shadowed controls, accent selection, lifting icon buttons | macOS system colors: `#ffffff` / `#1e1e1e` windows, label ink, system blue, system status colors (higher-contrast variants in light) |
| macOS | 10px corners, 0.5px outlines, frosted glass, accent primary, 5px row inset; buttons, chips, send and phone keys cast no shadow, and icon buttons, tabs, sidebar rows and agent controls tint instead of lift | the same system colors, white text on the accent, stronger popover shadows |
| Tahoe | 14px corners and macOS's colors and glass, accent primary; glass pill chrome and browser buttons, a glass composer and pane chips, flat tinted pill buttons, chips and fields without outlines, cards without outlines, the accent on the active window pill and selected chips, no bar dividers | macOS's |
| Linear | 24px corners, 0.5px outlines, no glass, accent primary; no role patches | near-black and near-white planes, indigo accent with white text, a cool tinted light shadow |
| Glass | 25px corners, no outlines, both shadows at strength 0, heavily frosted glass with a dark contour, 3px row inset, foreground primary; buttons, icon buttons, chips, tabs, fields, badges and keycaps are pills, and rows and answer options select with a tint | the macOS system colors |

They imitate an app's feel, not its pixels. A test holds every built-in to contrast floors in both modes: foreground 7:1 on the window and a popover, accent and status colors 3:1, text on the accent 3:1.

A surface whose role takes glass draws the theme's glass, tinted with its own fill, and its glass rim stands in for the outline; on a glass ground it paints a tint instead, and in a theme without glass a solid fill.

# Where it lives

| Piece | Code |
|---|---|
| Theme, materials, roles and colors; built-ins; parser | `crates/zz-ui/src/interface_theme.rs` |
| Role ids a theme may name | `zz_ui::roles` (`crates/zz-ui/src/roles.rs`) |
| Built-in ids for config validation | `zz_client::appearance::BUILTIN_THEMES` |
| `interface-theme` key, `themes/` folder, writer, retired keys | `crates/zz-config/src/lib.rs`, `crates/zz-config/src/themes.rs` |
| Desktop: load, apply, Settings tiles, Duplicate and Import | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: the selected id, saved themes and material overrides in preferences; tiles, Duplicate, Import and Delete | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| Tile preview | `zz_ui::settings::appearance::interface_theme_preview` |
| Kit values the theme sets | `ThemeColor::shadow_color`, `ThemeColor::on_accent`; `Theme::radius`, `outline`, `row_inset`, `primary`, `glass`, `contact_shadow`, `float_shadow` and `styles` |
| Material overrides | `zz_ui::interface_theme::MaterialOverrides`, `choice_name`, `parse_choice`; desktop `config::material_overrides` in `crates/zz/src/config/mod.rs` |
| Material keys, their choices and the theme's values | `zz_config::ConfigKey::MATERIALS`, `zz_config::MaterialDefaults`, `zz_client::appearance::{PRIMARY,GLASS}_CHOICES` |
| The Materials settings group | `zz_ui::settings::appearance::AppearancePageItem::MATERIALS`, `material_text`, `material_choices` |
| Theme creator and knobs | `clients/storybook/src/stories/theme_creator.rs`, `clients/storybook/src/knobs.rs`, `clients/storybook/web/main.js` |

Customizing a theme means editing a file. Settings' Duplicate writes the active theme, with the material overrides on screen folded in, to `themes/theme-N.json` and selects it; the storybook's theme creator edits a theme live (each mode's colors, the materials and every role) and copies it as JSON for Import. There is no override layer for colors or roles in `zz/config`. The materials have one: six keys (`widget-corner-radius`, `widget-outline-width`, `widget-shadow-strength`, `widget-row-inset`, `widget-primary` and `widget-glass`) each set one material over the active theme and follow the theme until set; `widget-shadow-strength` scales both shadows from 0 to 2. Settings edits them in the Materials group at the end of Interface > Advanced, and picking a theme removes all six. `chrome-contrast` stays its own setting and survives a theme change. `widget-shadows`, `widget-selection` and `widget-field` are retired into role props, and `shadow-strength` into `widget-shadow-strength`: a line with one is reported and ignored.

The web and iOS clients pick their theme themselves and never take the host's. Their preferences (local storage on web, `preferences.json` on iOS) hold the selected id and up to 32 saved themes as `[id, theme]` pairs. Duplicate and Import work as on desktop, saving under the first free `theme-N` id, and Delete removes the selected saved theme and falls back to Default. A saved theme this build cannot read is skipped instead of losing the other preferences (one duplicated before materials and roles still carries `look`, so it is skipped too), a selection that names no theme falls back to Default, and Import refuses text over 64 KB so local storage stays well under its quota. Their material overrides live in the same preferences (`Preferences.materials`, a `MaterialOverrides` flattened into the stored JSON) and clear when a theme is picked; the `shadows`, `selection` and `field` values older builds stored are ignored.

# Decisions

- **No surviving palettes.** None of the 34 presets read right across looks, so all went, with their per-mode keys and the three `chrome-*` color overrides. Old keys are reported as removed and dropped when a theme is picked.
- **Flat, Modern and Full dropped.** Default, macOS, Tahoe, Linear and Glass cover the same range with colors that fit them.
- **Nine look keys, not seventeen (2026-10-13).** Shadow strength, the share controls cast and elevation became one choice, `shadows`. Outline strength and outline width became `outline`, a width. Corner smoothing became the constant `CORNER_SMOOTHING` (4) that every window draws with. Density, control fill, divider strength and animation speed are gone, so control heights, fills, rules and transitions no longer change per theme.
- **Materials and roles instead of a look (2026-10-13, same day).** A global look could not put glass on toolbar buttons and flat fills on the chips under them. `shadows`, `selection` and `field` became role props (`shadow`, `selected`, a field role's `fill` and `outline`), the shadows became two materials with their own geometry, and `look` became `materials` plus `roles`. Unreleased, so there is no migration: a file with `look` stops loading until rewritten. See [surface roles](/designs/surface-roles.md).
- **`interface-theme` keeps `theme-mode`.** The mode stays its own setting, as in the apps themes imitate.
- **Thin clients keep their own themes.** Web and iOS save themes in their preferences instead of syncing the host's `themes/` folder, so a phone can look different from the desktop it attaches to, and a theme carries over by Import.
- **Terminal colors untouched.** Ghostty theme files still color terminal content; chrome never follows them.

---
type: Design Plan
title: "Interface themes: a look bundled with its colors"
description: "A theme is one JSON file holding a look and the colors it is drawn in for light and for dark, so it can imitate a whole app; the mode only picks the color set. Default, macOS, Linear and Glass ship built in; the 34 palette presets, Flat/Modern/Full and the chrome color overrides are gone."
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
- `look`: the `Look`, every value that is not a color, in nine keys: `radius`, `outline` (the width of the hairline controls, cards and floating surfaces draw; 0 draws none), `shadows` (`none`, `surfaces` or `all`), `row-inset`, `selection`, `primary`, `field`, `font` and `glass`;
- `light` and `dark`: a `ThemeColors` each, the six roots plus `on-accent` (text on an accent fill) and `shadow-color` (the ink of every drop shadow).

The mode (`theme-mode`, or the OS appearance on System) only picks which color set applies. The look never picks a color, and a color set never changes the shape. Every other color is still derived from the roots at paint time, as the [design system](/designs/design-system.md) describes.

A file can leave keys out: the look falls back to `Look::default()` and a color to the built-in palette for the mode. Glass is a preset name or a preset with the fields it changes, so a hand-written theme stays short. A key the look does not have, such as the removed `shadow` or `corner-smoothing`, is an error that names it, so a file written for the older look loads only once those keys are taken out (see [app config](/configuration/app-config.md)).

# Built-ins

`crates/zz-ui/themes/*.json`, compiled in and parsed by the same code as a user's file:

| Theme | Look | Colors |
|---|---|---|
| Default | 24px corners, 0.5px outlines, shadows on everything (`all`), opaque glass edge, foreground primary, accent selection | macOS system colors: `#ffffff` / `#1e1e1e` windows, label ink, system blue, system status colors (higher-contrast variants in light) |
| macOS | 10px corners, 0.5px outlines, flat controls under shadowed frosted popovers (`surfaces`), inset rows, accent primary and selection | the same system colors, white text on the accent, stronger popover shadows |
| Linear | 24px corners, 0.5px outlines, shadows on everything, opaque surfaces with no glass, accent primary and selection | near-black and near-white planes, indigo accent with white text, a cool tinted light shadow |
| Glass | 25px corners (pill controls), no outlines or shadows (`outline` 0, `shadows` `none`), heavily frosted glass with a dark contour, inset rows, gray (wash) selection, foreground primary | the macOS system colors |

They imitate an app's feel, not its pixels. A test holds every built-in to contrast floors in both modes: foreground 7:1 on the window and a popover, accent and status colors 3:1, text on the accent 3:1.

While a look has glass, menus, popovers, dialogs and the other floating surfaces draw the glass rim in place of the outline; controls and cards keep the hairline.

# Where it lives

| Piece | Code |
|---|---|
| Theme, look and colors; built-ins; parser | `crates/zz-ui/src/interface_theme.rs` |
| Built-in ids for config validation | `zz_client::appearance::BUILTIN_THEMES` |
| `interface-theme` key, `themes/` folder, writer, retired keys | `crates/zz-config/src/lib.rs`, `crates/zz-config/src/themes.rs` |
| Desktop: load, apply, Settings tiles, Duplicate and Import | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: the selected id, saved themes and look overrides in preferences; tiles, Duplicate, Import and Delete | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| Tile preview | `zz_ui::settings::appearance::interface_theme_preview` |
| Kit values the theme sets | `ThemeColor::shadow_color`, `ThemeColor::on_accent`; `Theme::outline`, `Theme::shadows` (`zz_gpui_kit::Shadows`) |
| Look overrides | `zz_ui::interface_theme::LookOverrides`, `choice_name`, `parse_choice`; desktop `config::look_overrides` in `crates/zz/src/config/mod.rs` |
| Look keys, their choices and the theme's values | `zz_config::ConfigKey::LOOK`, `zz_config::LookDefaults`, `zz_client::appearance::{SHADOWS,SELECTION,PRIMARY,FIELD,GLASS}_CHOICES` |
| The Look settings group | `zz_ui::settings::appearance::AppearancePageItem::LOOK`, `look_text`, `look_choices` |
| Theme creator and knobs | `clients/storybook/src/knobs.rs`, `clients/storybook/web/main.js` |

Customizing a theme means editing a file. Settings' Duplicate writes the active theme, with the look overrides on screen folded in, to `themes/theme-N.json` and selects it; the storybook's theme creator edits a theme live (each mode's colors included) and copies it as JSON for Import. There is no override layer for colors in `zz/config`. The look has one: eight keys (`widget-corner-radius`, `widget-outline-width`, `widget-shadows`, `widget-row-inset`, `widget-selection`, `widget-primary`, `widget-field` and `widget-glass`) each set one look value over the active theme and follow the theme until set. Settings edits them in the Look group at the end of Interface > Advanced, and picking a theme removes all eight. `chrome-contrast` stays its own setting and survives a theme change. `shadow-strength` is retired: a line with it is reported and ignored.

The web and iOS clients pick their theme themselves and never take the host's. Their preferences (local storage on web, `preferences.json` on iOS) hold the selected id and up to 32 saved themes as `[id, theme]` pairs. Duplicate and Import work as on desktop, saving under the first free `theme-N` id, and Delete removes the selected saved theme and falls back to Default. A saved theme this build cannot read is skipped instead of losing the other preferences (one duplicated before 2026-10-13 still carries the removed look keys, so it is skipped too), a selection that names no theme falls back to Default, and Import refuses text over 64 KB so local storage stays well under its quota. Their look overrides live in the same preferences (`Preferences.look`, a `LookOverrides` flattened into the stored JSON) and clear when a theme is picked; the `radius` value older builds saved still reads, and `shadow_strength` is dropped.

# Decisions

- **No surviving palettes.** None of the 34 presets read right across looks, so all went, with their per-mode keys and the three `chrome-*` color overrides. Old keys are reported as removed and dropped when a theme is picked.
- **Flat, Modern and Full dropped.** Default, macOS, Linear and Glass cover the same range with colors that fit them.
- **Nine look keys, not seventeen (2026-10-13).** Shadow strength, the share controls cast and elevation became one choice, `shadows`. Outline strength and outline width became `outline`, a width. Corner smoothing became the constant `CORNER_SMOOTHING` (4) that every window draws with. Density, control fill, divider strength and animation speed are gone, so control heights, fills, rules and transitions no longer change per theme.
- **`interface-theme` keeps `theme-mode`.** The mode stays its own setting, as in the apps themes imitate.
- **Thin clients keep their own themes.** Web and iOS save themes in their preferences instead of syncing the host's `themes/` folder, so a phone can look different from the desktop it attaches to, and a theme carries over by Import.
- **Terminal colors untouched.** Ghostty theme files still color terminal content; chrome never follows them.

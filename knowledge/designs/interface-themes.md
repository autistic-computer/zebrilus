---
type: Design Plan
title: "Interface themes: a look bundled with its colors"
description: "A theme is one JSON file holding a look and the colors it is drawn in for light and for dark, so it can imitate a whole app; the mode only picks the color set. Default, macOS and Linear ship built in; the 34 palette presets, Flat/Modern/Full and the chrome color overrides are gone."
status: Built
resource: crates/zz-ui/src/interface_theme.rs
tags:
- ui
- theme
- design-system
- configuration
- design-plan
timestamp: 2026-10-11T00:00:00Z
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
- `look`: the `Look`, every value that is not a color (corners, outlines, shadow strength and how much of it controls cast, elevation, row inset, selection and primary fills, field emphasis, density, dividers, animation speed, font, glass);
- `light` and `dark`: a `ThemeColors` each, the six roots plus `on-accent` (text on an accent fill) and `shadow-color` (the ink of every drop shadow).

The mode (`theme-mode`, or the OS appearance on System) only picks which color set applies. The look never picks a color, and a color set never changes the shape. Every other color is still derived from the roots at paint time, as the [design system](/designs/design-system.md) describes.

A file can leave keys out: the look falls back to `Look::default()` and a color to the built-in palette for the mode. Glass is a preset name or a preset with the fields it changes, so a hand-written theme stays short.

# Built-ins

`crates/zz-ui/themes/*.json`, compiled in and parsed by the same code as a user's file:

| Theme | Look | Colors |
|---|---|---|
| Default | 24px corners, opaque glass edge, foreground primary, accent selection | macOS system colors: `#ffffff` / `#1e1e1e` windows, label ink, system blue, system status colors (higher-contrast variants in light) |
| macOS | 10px corners, flat controls under shadowed frosted popovers, inset rows, accent primary and selection | the same system colors, white text on the accent, stronger popover shadows |
| Linear | 8px plain corners, opaque surfaces with ring-shadow outlines, gray (wash) selection, accent primary | near-black and near-white planes, indigo accent with white text, a cool tinted light shadow |

They imitate an app's feel, not its pixels. A test holds every built-in to contrast floors in both modes: foreground 7:1 on the window and a popover, accent and status colors 3:1, text on the accent 3:1.

# Where it lives

| Piece | Code |
|---|---|
| Theme, look and colors; built-ins; parser | `crates/zz-ui/src/interface_theme.rs` |
| Built-in ids for config validation | `zz_client::appearance::BUILTIN_THEMES` |
| `interface-theme` key, `themes/` folder, writer, retired keys | `crates/zz-config/src/lib.rs`, `crates/zz-config/src/themes.rs` |
| Desktop: load, apply, Settings tiles, Duplicate and Import | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: a built-in id in preferences | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| Tile preview | `zz_ui::settings::appearance::interface_theme_preview` |
| Kit colors the theme sets | `ThemeColor::shadow_color`, `ThemeColor::on_accent`, `Theme::control_shadow` |
| Theme creator and knobs | `clients/storybook/src/knobs.rs`, `clients/storybook/web/main.js` |

Customizing means editing a file: Settings' Duplicate writes the active theme to `themes/theme-N.json` and selects it, and the storybook's theme creator edits a theme live (each mode's colors included) and copies it as JSON for Import. There is no override layer for colors in `zz/config`; `widget-corner-radius`, `shadow-strength` and `chrome-contrast` stay as overrides of the look, and picking a theme removes the first two.

# Decisions

- **No surviving palettes.** None of the 34 presets read right across looks, so all went, with their per-mode keys and the three `chrome-*` color overrides. Old keys are reported as removed and dropped when a theme is picked.
- **Flat, Modern and Full dropped.** Default, macOS and Linear cover the same range with colors that fit them.
- **`interface-theme` keeps `theme-mode`.** The mode stays its own setting, as in the apps themes imitate.
- **Thin clients get built-ins only.** They keep a theme id; saved theme files are a desktop feature for now.
- **Terminal colors untouched.** Ghostty theme files still color terminal content; chrome never follows them.

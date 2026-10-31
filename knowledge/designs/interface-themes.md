---
type: Design Plan
title: "Interface themes: materials, roles and colors in one file"
description: "A theme is one JSON file holding its materials, how each piece of UI uses them (its roles), and the colors it is drawn in for light and for dark, so it can imitate a whole app; the mode only picks the color set. Default, macOS and Yaru ship built in. Settings draws the theme with an accent and a background tint over it, and saves the result as a theme; the 34 old palette presets, the color and material presets, Flat/Modern/Full, the chrome color overrides, the nine-key look and the per-material keys are gone."
status: Built
resource: libs/penna/src/interface_theme.rs
tags:
- ui
- theme
- design-system
- configuration
- design-plan
timestamp: 2026-10-30T00:00:00Z
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
- `materials`: the `Materials`, every value that is not a color, defined once: `radius`, `control-radius` (the corners of buttons, fields, tabs, chips and other controls, so a theme can round them tighter than its menus, cards and panes, as WinUI's 4px controls sit in 8px flyouts; left out, controls follow `radius`, and the pane corner, concentric with the controls in its header, follows it too), `smoothing` (the corner every role follows unless the theme sets its own), `picked-smoothing` (only the roles the theme sets `smoothing` for curve that way; the rest draw circular corners), `outline` (the width of every outline; 0 draws none), `picked-outlines` (only the roles the theme turns `outline` on for draw it), `row-inset`, `primary` (`foreground` or `accent`), `flat-highlights` (a lift washes a step deeper instead of rising), `bright-surfaces` (cards, panes and floating surfaces rise toward white in light mode instead of darkening), `highlight-strength` (how far a tint or lift washes from the resting fill; 1 is the built-in), `font`, `glass`, `light-glass` (what glass becomes in light mode, as the fields it changes from `glass`; left out, light mode draws `glass`), `picked-shadows` (only the roles the theme gives a `shadow` cast one), and the two shadows `contact` and `float`, each `y`, `blur`, `spread` and `strength`;
- `roles`: how the theme changes each kit kind and each zz role, by id, as a patch of the seven style props (`fill`, `outline`, `shadow`, `shape`, `smoothing`, `hover`, `selected`); what it leaves out draws as built in. The model is in [surface roles](/designs/surface-roles.md);
- `light` and `dark`: a `ThemeColors` each, the six roots plus `on-accent` (text on an accent fill) and `shadow-color` (the ink of every drop shadow).

The mode (`theme-mode`, or the OS appearance on System) only picks which color set applies. Materials and roles never pick a color, and a color set never changes the shape. Every other color is still derived from the roots at paint time, as the [design system](/designs/design-system.md) describes.

A file can leave keys out: materials fall back to `Materials::default()` (Default's, without glass), roles to their built-in styles, and a color to the built-in palette for the mode. Glass is a preset name or a preset with the fields it changes, and a shadow object names only the fields it changes, so a hand-written theme stays short. A key the format does not have, such as the removed `look`, is an error that names it, and a role id `zz_ui::roles::find` does not know is an error that suggests the one meant, so a file written for the older format loads only once it is rewritten (see [app config](/configuration/app-config.md)).

# Built-ins

penna ships them in `libs/penna/themes/*.json`: each theme's materials, its colors and the patches for penna's own roles. An app lays the patches for its own roles over a built-in by theme id, and can add whole themes under new ids, with `penna::interface_theme::register`; zz's patches are `crates/zz-ui/themes/*.json`. All of it is compiled in and parsed by the same code as a user's file, an app's file laid over penna's key by key (`parse_layers`: objects merge, any other value replaces):

| Theme | Materials and roles | Colors |
|---|---|---|
| Default | 24px corners, 0.75px outlines, both shadows at their built-in geometry, no glass, foreground primary, 5px row inset; no role patches, so every piece draws as its kind's built-in | neutral planes `#fcfcfc` / `#18181a`, near-black and near-white ink, a blue accent `#3d6df2` / `#6b8cff`, status colors that read on both planes |
| macOS | 24px corners, 0.5px outlines, 5px row inset, accent primary, glass (blur 20, bezel 3, refraction 4, tint `#ffffff8c`, light from above; in light mode a lighter contour spread evenly around the edge) on chrome buttons, selected tabs and sidebar entries and floating surfaces, contact and float shadows, 38 role patches | `#f5f5f7` / `#242424` planes, `#1d1d1f` / `#f5f5f7` ink, accent `#006fe8` / `#0a84ff`, the system green, orange and red darkened in light to pass the 3:1 floor |
| Yaru | 16px circular corners, 1px outlines only where picked, 6px row inset, accent primary, bright surfaces, highlight strength 0.8, the platform font, no glass; buttons and fields tinted with no outline or shadow, quiet, icon and chrome buttons and menu items unfilled, rows, sidebar entries and tabs tinted on hover and when selected, menus and cards outlined with a soft shadow, 21 role patches | Yaru's `#fafafa` / `#2c2c2c` planes, `#3d3d3d` / `#f7f7f7` ink, Ubuntu orange (`#e95420`, darkened to `#d6421d` in light the way Yaru's build tunes it for white text), Yaru's green, yellow and red, moved in lightness where they missed the 3:1 floor |

There are no color or material presets: a theme's materials and palettes are its own, and Settings
colors the interface with the accent and the tint below. A test holds every built-in to contrast floors in both modes: foreground 7:1 on the window and a popover, accent and status colors 3:1, text on the accent 3:1.

A surface whose role takes glass draws the theme's glass, tinted with its own fill, and its glass rim stands in for the outline; on a glass ground it paints a tint instead, and in a theme without glass a solid fill.

# Over the theme

Settings > Interface draws the active theme with up to three things set over it, each `None` in `ThemeOverrides` until set. `ThemeOverrides::apply_to` applies them in this order:

1. **Corner radius** (`widget-corner-radius`): the radius alone, over the theme's.
2. **Accent** (`chrome-accent`): replaces the accent root in each mode and drops the theme's `on-accent`, so text on the accent is picked by its lightness again. A named accent takes its light or dark color; a hex color is used in both modes.
3. **Tint** (`chrome-tint`, `chrome-tint-hue`): mixes each mode's background in Oklab toward the color of the same lightness at the tint hue with 0.02 chroma, all the way at full strength (`ThemeColor::tinted`). Panels, hover fills, edges and glass tints derive from the background, so they lean with it; text, the accent and the status colors keep theirs. The hue defaults to 260, a cool blue.

Roles always stay the theme's, and so do its materials apart from the radius and its colors apart from the accent and the tinted background.

The named accents (`zz_ui::interface_theme::ACCENT_COLORS`, names in `zz_client::appearance::ACCENTS`):

| Accent | Light | Dark |
|---|---|---|
| Blue | `#146ae3` | `#76abfd` |
| Purple | `#8a4fd7` | `#bd97fe` |
| Pink | `#c63187` | `#f585bd` |
| Red | `#d32e33` | `#ff877f` |
| Orange | `#b35407` | `#f88e4f` |
| Yellow | `#8d6a07` | `#d3a229` |
| Green | `#028132` | `#59be6d` |
| Graphite | `#6f6f6f` | `#aaaaaa` |

A test (`every_named_accent_reads_at_any_tint`) holds each one to the built-in contrast floors on Default in both modes at full tint, with the hue every 30 degrees.

Picking a theme clears the radius (`ThemeOverrides::pick_theme`, `ConfigKey::THEME_PICKS` on desktop), so it shows as designed; the accent and the tint stay, like `chrome-contrast`. Settings has no Save since 2026-10-27; a look kept as a theme is made in the theme creator and imported.

# Where it lives

| Piece | Code |
|---|---|
| Theme, materials, roles and colors; parser; built-in registry | `libs/penna/src/interface_theme.rs` |
| The built-ins (Default, macOS, Yaru) and the app layers over them | `libs/penna/themes/*.json` (`BUILTIN_THEMES`, `register`, `parse_layers` in `libs/penna/src/interface_theme.rs`); zz's patches for its own roles in `crates/zz-ui/themes/*.json` (`LAYERS` in `crates/zz-ui/src/interface_theme.rs`) |
| Role ids a theme may name | `penna::roles` and `zz_ui::roles` (`libs/penna/src/roles.rs`, `crates/zz-ui/src/roles.rs`) |
| Built-in ids, named accents and the radius limit for config validation | `zz_client::appearance::BUILTIN_THEMES`, `ACCENTS`, `AccentSetting`, `DEFAULT_TINT_HUE`, `MAX_CORNER_RADIUS` |
| `interface-theme`, `chrome-accent` and the tint keys, `themes/` folder, writer, retired keys | `crates/zz-config/src/lib.rs` (`ConfigKey::THEME_PICKS`, `RETIRED_MATERIAL_KEYS`), `crates/zz-config/src/themes.rs` |
| Overrides over the theme | `zz_ui::interface_theme::ThemeOverrides` (`apply_to`, `pick_theme`), `accent_color`, `ACCENT_COLORS`; desktop `config::theme_overrides` and `config::shown_theme` in `crates/zz/src/config/mod.rs` |
| The tint | `ThemeColor::tinted` and the `oklch` helper in `libs/penna/src/foundation/` |
| Desktop: load, apply, Settings rows and Import | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: the selected id, saved themes and overrides in preferences; the same rows, Import and Delete | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| The Interface page and its rows | `penna::settings::appearance`: `appearance_page`, `AppearancePageItem`, `appearance_page_items`, `ThemePicks`, `theme_mode_control`, `theme_picker`, `theme_labels`, `accent_swatches`, `tint_hue_slider`, `tint_slider`, `corner_radius_slider`, `contrast_slider`; zz's own rows (Program colors, App Icon) are `zz_ui::settings::appearance::ZzRow`, added by `zz_ui::settings::appearance::appearance_page_items` |
| Tile preview | `penna::settings::appearance::interface_theme_preview` |
| The sliders | the kit's continuous `Slider` in `libs/penna/src/slider.rs` |
| The custom accent | the kit's `ColorPicker` (`libs/penna/src/color_picker/mod.rs`) drawn as a bare swatch with `.swatch(size)`; swatches round with `Theme::swatch_radius()` |
| Kit values the theme sets | `ThemeColor::shadow_color`, `ThemeColor::on_accent`; `Theme::radius`, `outline`, `row_inset`, `primary`, `glass` (`light-glass` in light mode), `contact_shadow`, `float_shadow` and `styles`; `Theme::pane_radius()` follows the radius, and zz-ui's `PaneLook::edge_width` the outline |
| Theme creator and knobs | `penna_storybook::creator` (`libs/penna/storybook/src/creator/`: the creator story, penna's plates and the parts plates share), `libs/penna/storybook/src/knobs.rs`, `libs/penna/storybook/web/main.js`; an app's plates are its catalog's `plates`, zz's in `clients/storybook/src/stories/theme_creator.rs` |

Settings changes a theme only through the overrides above; anything finer means editing a file. Settings' Import writes the theme on the clipboard to `themes/theme-N.json` and selects it. The storybook's theme creator edits a theme live (each mode's colors, the materials and every role) and copies it as JSON for Import. It lives in penna's storybook, so any app on penna gets it: the app registers its roles with `penna::roles::register` in its catalog's `register` function and lists plates that draw them in the catalog's `plates`; the Roles step then shows them under their parents, Copy JSON writes their patches and Import reads a theme that names them, dropping retired ids and refusing unknown ones like every theme file. See [storybook](/playbooks/storybook.md). There is no override layer for roles in `zz/config`, and none for a single color besides the accent. `chrome-contrast` stays its own setting and survives a theme change. `interface-style`, `widget-outline-width`, `widget-shadow-strength`, `widget-row-inset`, `widget-primary` and `widget-glass` are retired into the theme (`RETIRED_MATERIAL_KEYS`) and `pane-corner-radius` and `pane-border-width` into the theme's radius and outline; `widget-shadows`, `widget-selection` and `widget-field` are retired into role props, and `shadow-strength` points to `interface-theme`. A line with any of them is reported and ignored, and picking a theme in Settings drops it.

The web and iOS clients pick their theme themselves and never take the host's. Their preferences (local storage on web, `preferences.json` on iOS) hold the selected id and up to 32 saved themes as `[id, theme]` pairs. Import works as on desktop, saving under the first free `theme-N` id, and Delete removes the selected saved theme and falls back to Default. A saved theme this build cannot read is skipped instead of losing the other preferences (one saved before materials and roles still carries `look`, so it is skipped too), a selection that names no theme falls back to Default, and Import refuses text over 64 KB so local storage stays well under its quota. Their overrides live in the same preferences (`Preferences.overrides`, a `ThemeOverrides` flattened into the stored JSON as `radius`, `accent`, `tint` and `tint-hue`), and `Preferences::shown_theme` composes them over the theme as on desktop. Picking a theme clears the radius. Loading drops an override of the wrong type or an unknown accent, and clamps the numbers; the values older builds stored (`style`, and the material values `outline`, `shadow-strength`, `row-inset`, `primary`, `glass`, and `shadows`, `selection` and `field` before them) are ignored.

# Decisions

- **No surviving palettes.** None of the 34 presets read right across looks, so all went, with their per-mode keys and the three `chrome-*` color overrides. Old keys are reported as removed and dropped when a theme is picked.
- **Flat, Modern and Full dropped.** Built-in themes with colors that fit them took their place.
- **One built-in theme and material presets (2026-10-14).** macOS, Tahoe, Linear and Glass are gone so themes can be rebuilt from scratch in the creator. Default was redrawn, and material presets give a theme's materials a starting point; roles, colors and the font stay the theme's own.
- **Color presets (2026-10-14).** The same idea for colors: a preset sets both palettes' six roots and nothing else, so picking macOS colors keeps Liquid glass materials and the roles. Unlike the 34 presets that went, they are checked against zz's own floors (foreground 7:1, accent and status 3:1 on the window and on a popover), and a color that missed moved in lightness only. Removed on 2026-10-15 with `interface-colors` and Settings' Colors row, neither ever released: the tint is how Settings colors the interface now, and a theme's own palettes are made in the creator.
- **Style, an accent and a tint, in Settings (2026-10-15).** The Interface page was one strip of whole themes, with six material keys under Advanced. It now picks a style from the presets the creator already had, so any theme goes with any materials without writing a file, and Save keeps the result as a theme. The material keys went with the Materials group: a style sets outlines, shadows, row inset, primary and glass together, only the radius stays a key of its own, and a look outside the presets is made in the theme creator. Pane corners and the pane edge follow the theme too (`Theme::pane_radius()` is the radius plus 3.5, the glint is the outline width), so a square style squares the panes and a style without outlines has no glint; `pane-corner-radius` and `pane-border-width` were retired with them. The accent comes as eight names, each a light and a dark color checked at every tint hue, or a hex color for both modes. The tint changes only the background, toward a low-chroma color of the same lightness, so contrast hardly moves. `interface-style` and `chrome-accent`, retired on 2026-10-11 with other meanings, are reused for these settings.
- **Pane opacity and window blur stay two settings (2026-10-15).** Folding them into one transparency setting was considered and not done: `pane-background-opacity` also decides how much a terminal's own background shows over the chrome while the window is opaque, so merging it into blur would change how every terminal looks. Superseded on 2026-10-18: `pane-background-opacity` is gone, panes always show the window behind them, and only PiPs keep an opacity, `pip-background-opacity`.
- **Themes only, Liquid Glass built in (2026-10-15).** Styles and themes side by side confused people: two strips that both changed the corners, the glass and the shadows. Settings' Style row and `interface-style` went the same day they arrived, the material presets behind them with them, and zz bundles finished themes instead, each one's materials, roles and colors made together in the theme creator. Liquid Glass is the first, drawn in macOS 27's own colors. `interface-style` was never released with its preset meaning, so it is simply retired again.
- **Liquid Glass removed (2026-10-22).** Default and macOS are the built-ins. A config or saved preference that names `liquid-glass` loads Default, as for any theme that is not found.
- **Yaru built in (2026-10-23).** Ubuntu's GNOME look, from Yaru's and libadwaita's stylesheets. In light, GNOME's menus and cards are white over a `#fafafa` window, while zz raised every light surface toward black, so a theme could not draw them; the `bright-surfaces` material added the same day lets a theme raise them toward white, and Yaru turns it on.
- **Nine look keys, not seventeen (2026-10-13).** Shadow strength, the share controls cast and elevation became one choice, `shadows`. Outline strength and outline width became `outline`, a width. Corner smoothing became the constant `CORNER_SMOOTHING` (4) that every window draws with. Density, control fill, divider strength and animation speed are gone, so control heights, fills, rules and transitions no longer change per theme.
- **Materials and roles instead of a look (2026-10-13, same day).** A global look could not put glass on toolbar buttons and flat fills on the chips under them. `shadows`, `selection` and `field` became role props (`shadow`, `selected`, a field role's `fill` and `outline`), the shadows became two materials with their own geometry, and `look` became `materials` plus `roles`. Unreleased, so there is no migration: a file with `look` stops loading until rewritten. See [surface roles](/designs/surface-roles.md).
- **`interface-theme` keeps `theme-mode`.** The mode stays its own setting, as in the apps themes imitate.
- **Thin clients keep their own themes.** Web and iOS save themes in their preferences instead of syncing the host's `themes/` folder, so a phone can look different from the desktop it attaches to, and a theme carries over by Import.
- **Terminal colors untouched.** Ghostty theme files still color terminal content; chrome never follows them.

---
type: Design Plan
title: "Interface themes: materials, roles and colors in one file"
description: "A theme is one JSON file holding its materials, how each piece of UI uses them (its roles), and the colors it is drawn in for light and for dark, so it can imitate a whole app; the mode only picks the color set. Default ships built in, with five material presets that set only the materials. Settings draws the theme with any style, an accent and a background tint over it, and saves the result as a theme; the 34 old palette presets, the color presets, Flat/Modern/Full, the chrome color overrides, the nine-key look and the per-material keys are gone."
status: Built
resource: crates/zz-ui/src/interface_theme.rs
tags:
- ui
- theme
- design-system
- configuration
- design-plan
timestamp: 2026-10-15T00:00:00Z
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
- `materials`: the `Materials`, every value that is not a color, defined once: `radius`, `outline` (the width of every outline; 0 draws none), `row-inset`, `primary` (`foreground` or `accent`), `flat-highlights` (a lift washes a step deeper instead of rising), `font`, `glass`, and the two shadows `contact` and `float`, each `y`, `blur`, `spread` and `strength`;
- `roles`: how the theme changes each kit kind and each zz role, by id, as a patch of the six style props (`fill`, `outline`, `shadow`, `shape`, `hover`, `selected`); what it leaves out draws as built in. The model is in [surface roles](/designs/surface-roles.md);
- `light` and `dark`: a `ThemeColors` each, the six roots plus `on-accent` (text on an accent fill) and `shadow-color` (the ink of every drop shadow).

The mode (`theme-mode`, or the OS appearance on System) only picks which color set applies. Materials and roles never pick a color, and a color set never changes the shape. Every other color is still derived from the roots at paint time, as the [design system](/designs/design-system.md) describes.

A file can leave keys out: materials fall back to `Materials::default()` (Default's, without glass), roles to their built-in styles, and a color to the built-in palette for the mode. Glass is a preset name or a preset with the fields it changes, and a shadow object names only the fields it changes, so a hand-written theme stays short. A key the format does not have, such as the removed `look`, is an error that names it, and a role id `zz_ui::roles::find` does not know is an error that suggests the one meant, so a file written for the older format loads only once it is rewritten (see [app config](/configuration/app-config.md)).

# Built-ins

`crates/zz-ui/themes/*.json`, compiled in and parsed by the same code as a user's file:

| Theme | Materials and roles | Colors |
|---|---|---|
| Default | 10px corners, 0.5px outlines, both shadows at their built-in geometry, no glass, foreground primary, 4px row inset; no role patches, so every piece draws as its kind's built-in | neutral planes `#fcfcfc` / `#18181a`, near-black and near-white ink, a blue accent `#3d6df2` / `#6b8cff`, status colors that read on both planes |

`crates/zz-ui/themes/materials/*.json` hold material presets: a name, a line saying what it is and
`materials`, nothing else, so a preset never touches roles, colors or the font.
`zz_ui::interface_theme::material_presets` lists them after a Default preset built from the
Default theme's materials, and `MaterialPreset::apply_to` puts one's materials on a theme, keeping
the theme's font. Settings' Style row and `interface-style` pick one over the active theme, and the
storybook's Materials step shows them as its Presets.

| Preset | Materials |
|---|---|
| Liquid glass | 18px corners, 0.5px lines, 5px row inset, accent primary, Tahoe's glass, a light contact shadow and a soft deep float |
| Capsule | 25px corners (so buttons, fields, rows and tabs are pills), 0.5px lines, 5px row inset, accent primary, flat highlights, dense glass (blur 22, 80% of the surface's own color, a bright top glint, little lensing and no contour), faint shadows; matched to iOS's floating tab bar |
| Flat | 6px corners, 1px lines, 3px row inset, accent primary, no glass, no contact shadow and a faint float |
| Brutal | square corners, 2px lines, no row inset, accent primary, no glass, hard unblurred shadows |
| Terminal | square corners, 1px lines, no row inset, foreground primary, no glass, no shadows |

There are no color presets: a theme's palettes are its own, and Settings colors the interface
with the accent and the tint below. A test holds every built-in to contrast floors in both modes: foreground 7:1 on the window and a popover, accent and status colors 3:1, text on the accent 3:1.

A surface whose role takes glass draws the theme's glass, tinted with its own fill, and its glass rim stands in for the outline; on a glass ground it paints a tint instead, and in a theme without glass a solid fill.

# Over the theme

Settings > Interface draws the active theme with up to four things set over it, each `None` in `ThemeOverrides` until set. `ThemeOverrides::apply_to` applies them in this order:

1. **Style** (`interface-style`): a material preset replaces the materials, keeping the theme's font.
2. **Corner radius** (`widget-corner-radius`): the radius alone, over the style's.
3. **Accent** (`chrome-accent`): replaces the accent root in each mode and drops the theme's `on-accent`, so text on the accent is picked by its lightness again. A named accent takes its light or dark color; a hex color is used in both modes.
4. **Tint** (`chrome-tint`, `chrome-tint-hue`): mixes each mode's background in Oklab toward the color of the same lightness at the tint hue with 0.02 chroma, all the way at full strength (`ThemeColor::tinted`). Panels, hover fills, edges and glass tints derive from the background, so they lean with it; text, the accent and the status colors keep theirs. The hue defaults to 260, a cool blue.

Roles always stay the theme's, and so do its colors apart from the accent and the tinted background. The Style strip draws every tile over what is on screen, so each tile shows what picking it would look like.

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

Picking a theme clears the style and the radius (`ThemeOverrides::pick_theme`, `ConfigKey::THEME_PICKS` on desktop), so it shows as designed; the accent and the tint stay, like `chrome-contrast`. Picking a style clears the radius (`pick_style`), so the style's corners show. Save folds everything on screen into a new theme, so it no longer needs the overrides.

# Where it lives

| Piece | Code |
|---|---|
| Theme, materials, roles and colors; built-ins; parser | `crates/zz-ui/src/interface_theme.rs` |
| Role ids a theme may name | `zz_ui::roles` (`crates/zz-ui/src/roles.rs`) |
| Built-in ids, preset ids, named accents and the radius limit for config validation | `zz_client::appearance::BUILTIN_THEMES`, `STYLE_PRESETS`, `ACCENTS`, `AccentSetting`, `DEFAULT_TINT_HUE`, `MAX_CORNER_RADIUS` |
| `interface-theme`, `interface-style`, `chrome-accent` and the tint keys, `themes/` folder, writer, retired keys | `crates/zz-config/src/lib.rs` (`ConfigKey::THEME_PICKS`, `RETIRED_MATERIAL_KEYS`), `crates/zz-config/src/themes.rs` |
| Overrides over the theme | `zz_ui::interface_theme::ThemeOverrides` (`apply_to`, `pick_theme`, `pick_style`), `accent_color`, `ACCENT_COLORS`, `material_preset`; desktop `config::theme_overrides` and `config::shown_theme` in `crates/zz/src/config/mod.rs` |
| The tint | `ThemeColor::tinted` and the `oklch` helper in `crates/zz-gpui-kit/src/foundation/` |
| Desktop: load, apply, Settings rows, Save and Import | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: the selected id, saved themes and overrides in preferences; the same rows, Save, Import and Delete | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| The Interface page's rows | `zz_ui::settings::appearance`: `AppearancePageItem`, `appearance_page_items`, `ThemePicks`, `style_strip`, `themes_strip`, `accent_swatches`, `tint_hue_slider`, `tint_slider`, `corner_radius_slider`, `contrast_slider` |
| Tile preview | `zz_ui::settings::appearance::interface_theme_preview` |
| The sliders | the kit's continuous `Slider` in `crates/zz-gpui-kit/src/slider.rs` |
| The custom accent | the kit's `ColorPicker` (`crates/zz-gpui-kit/src/color_picker/mod.rs`) drawn as a bare swatch with `.swatch(size)`; swatches round with `Theme::swatch_radius()` |
| Kit values the theme sets | `ThemeColor::shadow_color`, `ThemeColor::on_accent`; `Theme::radius`, `outline`, `row_inset`, `primary`, `glass`, `contact_shadow`, `float_shadow` and `styles`; `Theme::pane_radius()` and `pane_edge_width()` follow the radius and the outline |
| Theme creator and knobs | `clients/storybook/src/stories/theme_creator.rs`, `clients/storybook/src/knobs.rs`, `clients/storybook/web/main.js` |

Settings changes a theme only through the overrides above; anything finer means editing a file. Settings' Save writes the look on screen, overrides and an explicit UI font folded in, to `themes/theme-N.json`, removes the accent and tint keys so the tint is not applied twice, and selects the new theme. The storybook's theme creator edits a theme live (each mode's colors, the materials and every role) and copies it as JSON for Import. There is no override layer for roles in `zz/config`, and none for a single color besides the accent. `chrome-contrast` stays its own setting and survives a theme change. `widget-outline-width`, `widget-shadow-strength`, `widget-row-inset`, `widget-primary` and `widget-glass` are retired into the styles and `pane-corner-radius` and `pane-border-width` into the theme's radius and outline; `widget-shadows`, `widget-selection` and `widget-field` are retired into role props, and `shadow-strength` points to `interface-style`. A line with any of them is reported and ignored.

The web and iOS clients pick their theme themselves and never take the host's. Their preferences (local storage on web, `preferences.json` on iOS) hold the selected id and up to 32 saved themes as `[id, theme]` pairs. Save and Import work as on desktop, saving under the first free `theme-N` id (Save is `Preferences::duplicate_theme`, which also drops every override, since they are now in the theme), and Delete removes the selected saved theme and falls back to Default. A saved theme this build cannot read is skipped instead of losing the other preferences (one saved before materials and roles still carries `look`, so it is skipped too), a selection that names no theme falls back to Default, and Import refuses text over 64 KB so local storage stays well under its quota. Their overrides live in the same preferences (`Preferences.overrides`, a `ThemeOverrides` flattened into the stored JSON as `style`, `radius`, `accent`, `tint` and `tint-hue`), and `Preferences::shown_theme` composes them over the theme as on desktop. Picking a theme clears the style and radius. Loading drops an override of the wrong type, an unknown preset or accent, and clamps the numbers; the material values older builds stored (`outline`, `shadow-strength`, `row-inset`, `primary`, `glass`, and `shadows`, `selection` and `field` before them) are ignored.

# Decisions

- **No surviving palettes.** None of the 34 presets read right across looks, so all went, with their per-mode keys and the three `chrome-*` color overrides. Old keys are reported as removed and dropped when a theme is picked.
- **Flat, Modern and Full dropped.** Built-in themes with colors that fit them took their place.
- **One built-in theme and material presets (2026-10-14).** macOS, Tahoe, Linear and Glass are gone so themes can be rebuilt from scratch in the creator. Default was redrawn, and material presets give a theme's materials a starting point; roles, colors and the font stay the theme's own.
- **Color presets (2026-10-14).** The same idea for colors: a preset sets both palettes' six roots and nothing else, so picking macOS colors keeps Liquid glass materials and the roles. Unlike the 34 presets that went, they are checked against zz's own floors (foreground 7:1, accent and status 3:1 on the window and on a popover), and a color that missed moved in lightness only. Removed on 2026-10-15 with `interface-colors` and Settings' Colors row, neither ever released: the tint is how Settings colors the interface now, and a theme's own palettes are made in the creator.
- **Style, an accent and a tint, in Settings (2026-10-15).** The Interface page was one strip of whole themes, with six material keys under Advanced. It now picks a style from the presets the creator already had, so any theme goes with any materials without writing a file, and Save keeps the result as a theme. The material keys went with the Materials group: a style sets outlines, shadows, row inset, primary and glass together, only the radius stays a key of its own, and a look outside the presets is made in the theme creator. Pane corners and the pane edge follow the theme too (`Theme::pane_radius()` is the radius plus 3.5, the glint is the outline width), so a square style squares the panes and a style without outlines has no glint; `pane-corner-radius` and `pane-border-width` were retired with them. The accent comes as eight names, each a light and a dark color checked at every tint hue, or a hex color for both modes. The tint changes only the background, toward a low-chroma color of the same lightness, so contrast hardly moves. `interface-style` and `chrome-accent`, retired on 2026-10-11 with other meanings, are reused for these settings.
- **Pane opacity and window blur stay two settings (2026-10-15).** Folding them into one transparency setting was considered and not done: `pane-background-opacity` also decides how much a terminal's own background shows over the chrome while the window is opaque, so merging it into blur would change how every terminal looks.
- **Nine look keys, not seventeen (2026-10-13).** Shadow strength, the share controls cast and elevation became one choice, `shadows`. Outline strength and outline width became `outline`, a width. Corner smoothing became the constant `CORNER_SMOOTHING` (4) that every window draws with. Density, control fill, divider strength and animation speed are gone, so control heights, fills, rules and transitions no longer change per theme.
- **Materials and roles instead of a look (2026-10-13, same day).** A global look could not put glass on toolbar buttons and flat fills on the chips under them. `shadows`, `selection` and `field` became role props (`shadow`, `selected`, a field role's `fill` and `outline`), the shadows became two materials with their own geometry, and `look` became `materials` plus `roles`. Unreleased, so there is no migration: a file with `look` stops loading until rewritten. See [surface roles](/designs/surface-roles.md).
- **`interface-theme` keeps `theme-mode`.** The mode stays its own setting, as in the apps themes imitate.
- **Thin clients keep their own themes.** Web and iOS save themes in their preferences instead of syncing the host's `themes/` folder, so a phone can look different from the desktop it attaches to, and a theme carries over by Import.
- **Terminal colors untouched.** Ghostty theme files still color terminal content; chrome never follows them.

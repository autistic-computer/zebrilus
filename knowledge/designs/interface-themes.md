---
type: Design Plan
title: "Interface themes: knobs over one look"
description: "The look is penna's one look with a person's knobs over it: accent, tint, corner radius, control shape, glass and UI font, each following the platform's theme file (macos on Apple platforms, default elsewhere) until set. There are no presets to pick and no saved themes; a theme file is the knobs as JSON, and Settings copies the look as one, pastes one in (which sets every knob) or resets them."
status: Built
resource: libs/penna/src/foundation/interface_theme.rs
tags:
- ui
- theme
- design-system
- configuration
- design-plan
timestamp: 2026-11-06T00:00:00Z
---

# Why

Before this, the Interface page had two independent pickers: a style (Flat, Modern, Full) and a palette per mode, from 34 presets ported from editor themes. The style's recipes guessed every color they needed from whichever palette came along:

- text on the accent was picked by lightness alone, so a mid-light accent got dark text;
- every shadow was the black scrim, so a cream light background got gray smudges under each card;
- panels and hover fills were the background nudged toward black or white, fine for a neutral gray and odd for a tinted one.

The editor palettes were made for syntax colors and gave six roots; everything else was a guess that only held for neutral palettes. Pairing any style with any palette produced combinations nobody designed.

# The model

A theme file (`InterfaceTheme`) is the knobs penna's one look takes, and nothing else (penna's `knowledge/designs/foundations.md`, Look, is the spec):

- `name`: a label the file carries; nothing person-facing picks by it;
- `tint`: a named accent, one hex color, or `{ "light": ..., "dark": ... }` (`blue` when left out). It sets the accent, which colors only the details (a switch that is on, a checked box, a slider's fill, the attention dot, links); primary fills and the highlight are always the ink;
- `neutral`: `{ "hue", "amount" }`, how far `bg`, `fg` and the surfaces above them lean toward a hue; the default (hue 60, amount 0.15) draws the base colors exactly;
- `radius` (containers) and `control-radius` (controls, or `full` for pills), both 0 to 25, and `smoothing` (2 to 5);
- `glass`: `none`, `liquid` or a material of its own; left out, `liquid`. Every material's tint rises until text on it reads at 4.5:1;
- `face`: the interface font;
- `mode`: `follow`, `light` or `dark`; anything but `follow` beats the person's mode.

Outline width (1), row inset (6), highlight strength, every kind's fill, edge and shadow, and the status colors are the look and have no knob. Unknown keys are ignored, so a file from before the knobs (with `materials`, `roles`, `light` and `dark`) loads as the default look, and a knob with a bad value (`"control-radius": "round"`) is an error that names it.

# The kit's theme files

penna ships three in `libs/penna/themes/*.json` (default, macos, yaru), compiled in and parsed by the same code as a pasted file (`parse_theme`, over `parse_layers`: objects merge, any other value replaces). The look starts from the platform's: `platform_theme()` is `macos` on macOS and iOS and `default` everywhere else. The other files stay for the tokens, the storybook and the tests; nothing person-facing picks among them.

| File | Knobs |
|---|---|
| default | none: blue tint, the warm neutral, 12 corners, smoothing 3, the platform's glass |
| macos | tint `blue`, `control-radius` `full`, smoothing 4, `liquid` glass |
| yaru | tint orange, radius 8, smoothing 2, no glass |

An app puts the look on screen with `Appearance::apply`, or with `InterfaceTheme::activate` on `Appearance::shown_theme()` when it applies the rest itself (zz desktop does); [penna](/references/penna.md) has the setup. penna's tests hold every theme file, at every accent and neutral, to the contrast floors.

# The knobs

Settings > Interface > Look sets the knobs over the platform's theme file, each `None` in `ThemeOverrides` until set. `ThemeOverrides::apply_to` applies them, and `Appearance::shown_theme` is the platform's file with them applied:

1. **Accent** (`chrome-accent`): `theme` (unset, the platform's), a named accent or a hex color. A named accent takes its light or dark color; a hex color is used in both modes.
2. **Tint** (`chrome-tint`, `chrome-tint-hue`): the amount and hue of the `neutral`. Set alone, each keeps the other from the platform's.
3. **Corner radius** (`widget-corner-radius`): the `radius` knob, clamped to 0 to 25.
4. **Controls** (`control-shape`): `rounded` or `pill` (`control-radius` `full`).
5. **Glass** (`interface-glass`): a switch; `liquid` when on, `none` when off, or a material as one line of JSON (what pasting a theme file with its own material writes).
6. **UI font** (`ui-font-family`): unset follows the platform file's `face`, which no file sets, so the system font.

The Theme file row's menu copies the shown look as a theme file, pastes one in (`AppearanceChange::Import`: `ThemeOverrides::from_theme` sets every knob and `face` the UI font) and resets every knob (`AppearanceChange::ResetLook`). On desktop, Paste writes each knob key and Reset removes them from the config file.

`interface-frost` (Appearance's `frost`) sets how thick glass is, and with it how see-through the window is; it is a person's setting and stays across Paste and Reset, like `chrome-contrast`. There is no Reduce transparency row: the system's setting turns glass off.

The named accents (`penna::foundation::interface_theme::ACCENTS`, their colors through `accent_color`; zz's config checks names against `zz_client::appearance::ACCENTS`):

| Accent | Light | Dark |
|---|---|---|
| Blue | `#0a64dd` | `#3183ff` |
| Purple | `#8549d1` | `#a366f7` |
| Pink | `#c02a82` | `#e648a0` |
| Red | `#cc252d` | `#f44647` |
| Orange | `#ad4f00` | `#de6700` |
| Yellow | `#876400` | `#af8200` |
| Green | `#007b2c` | `#009f3b` |
| Graphite | `#6a6a6a` | `#868686` |

penna's `every_accent_reads_in_every_theme_at_any_neutral` holds each one to the contrast floors on every theme file in both modes, at neutral amounts 0 to 1 and the hue every 30 degrees.

# Where it lives

| Piece | Code |
|---|---|
| Theme knobs (`Tint`, `Neutral`, `ControlRadius`, `Glass`, `ModeLock`); parser; the kit's theme files and `platform_theme`; `InterfaceTheme::activate`; the settings model `Appearance` with `AppearanceChange` and `Appearance::apply` | `libs/penna/src/foundation/interface_theme.rs` |
| The kit's theme files (default, macos, yaru) | `libs/penna/themes/*.json` (`builtin_themes`, `parse_layers` in `libs/penna/src/foundation/interface_theme.rs`) |
| Named accents, control shapes, glass names, the radius limit and the default neutral hue for config validation | `zz_client::appearance::ACCENTS`, `AccentSetting` (with `Theme` and `Ink`), `ControlShapeSetting`, `GLASSES`, `DEFAULT_TINT_HUE`, `MAX_CORNER_RADIUS`; zz-ui's `zz_names_the_accents_and_limits_the_kit_draws` pins them against penna |
| The knob keys, the writer that sets and clears several at once, retired keys | `crates/zz-config/src/lib.rs` (`ConfigKey::LOOK`, `write_config_keys_at`, `RETIRED_THEME_KEYS`, `RETIRED_MATERIAL_KEYS`) |
| Knobs over the platform's file | `penna::foundation::interface_theme::ThemeOverrides` (`apply_to`, `from_theme`) and `accent_color`, re-exported by `zz_ui::interface_theme` with `parse_glass`, `spelled_glass` and the control shape conversions; desktop `config::appearance`, `config::shown_theme` and `config::look_edits` in `crates/zz/src/config/mod.rs` |
| The neutral | `ThemeColor::neutral` and the `oklch` helper in `libs/penna/src/foundation/` |
| Desktop: load, apply, Settings rows, Paste and Reset | `crates/zz/src/config/mod.rs`, `crates/zz/src/theme.rs`, `crates/zz/src/config/settings.rs` |
| Thin clients: the knobs in preferences; the same rows | `clients/app/src/preferences.rs`, `clients/app/src/app/settings.rs` |
| The Interface page and its rows | `penna::blocks::appearance`: `appearance_page`, `AppearancePageItem`, `appearance_page_items`, `kit_row` and `AppearanceControls` over a `penna::foundation::interface_theme::Appearance` (edits come back as `AppearanceChange`), and the parts they use: `theme_mode_control`, `accent_swatches`, `tint_hue_slider`, `tint_slider`, `corner_radius_slider`, `controls_control`, `glass_control`; zz's own rows (Program colors, App Icon and the window rows) are `zz_ui::settings::appearance::ZzRow`, added by `zz_ui::settings::appearance::appearance_page_items` |
| The sliders | the kit's continuous `Slider` in `libs/penna/src/components/slider/mod.rs` |
| The custom accent | the kit's `ColorPicker` (`libs/penna/src/blocks/color_picker/mod.rs`) drawn as a bare swatch with `.swatch(size)`; swatches round with `Theme::swatch_radius()` |
| Kit values the theme sets | `Theme::colors`, `radius`, `control_radius`, `smoothing`, `accent`, `glass` and `font_family`; `outline` (1), `row_inset` (6) and the shadows are fixed. `Theme::pane_radius()` follows the radius up to 16, and zz-ui's `PaneLook::edge_width` the outline |
| Theme creator and knobs | `penna_storybook::creator` (`libs/penna/storybook/src/creator/`: the creator story, penna's plates and the parts plates share), `libs/penna/storybook/src/knobs.rs`, `libs/penna/storybook/web/main.js`; an app's plates are its catalog's `plates`, and zz's catalog lists none |

Settings changes the look only through the knobs above; anything finer means editing a theme file and pasting it. The storybook's theme creator edits a file's knobs live and copies it as JSON for Paste. It lives in penna's storybook and opens penna's own page (`just storybook penna`); an app on penna gets it by listing plates that draw its pieces in its catalog's `plates`, which zz's page does not. See [storybook](/playbooks/storybook.md). There is no knob for a single color besides the accent. `chrome-contrast` stays its own setting. `interface-theme` is retired (`RETIRED_THEME_KEYS`): a leftover line is reported with a pointer to Settings > Interface and ignored, and zz no longer reads or watches a `themes/` folder. `interface-style`, `widget-outline-width`, `widget-shadow-strength`, `widget-row-inset`, `widget-primary` and `widget-glass` are retired into the look (`RETIRED_MATERIAL_KEYS`; `widget-glass` points to `interface-glass`) and `pane-corner-radius` and `pane-border-width` into its radius and outline; `widget-shadows`, `widget-selection` and `widget-field` are retired into role props. A line with any of them is reported and ignored.

The web and iOS clients set their knobs themselves and never take the host's. Their preferences (local storage on web, `preferences.json` on iOS) hold them in `Preferences.appearance`, a `penna::foundation::interface_theme::Appearance` flattened into the stored JSON, with its `ThemeOverrides` as `radius`, `accent`, `tint`, `tint-hue`, `controls` and `glass`; `Preferences::shown_theme` is the platform's file with them applied, as on desktop. Paste and Reset are the kit's menu items. Loading drops an override of the wrong type or an unknown accent, and clamps the numbers; the values older builds stored (the picked `theme` and the saved `themes`, `style`, and the material values `outline`, `shadow-strength`, `row-inset`, `primary`, and `shadows`, `selection` and `field` before them) are ignored, while an older `glass` value now reads as the glass knob.

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
- **Thin clients keep their own look.** Web and iOS keep their knobs in their preferences instead of syncing the host's, so a phone can look different from the desktop it attaches to, and a look carries over by copying and pasting a theme file.
- **Knobs over one look (2026-11-05).** penna's step 10 draws every kind from one look and shrinks a theme to its knobs: the materials, the role patches and each mode's color set went, Default, macOS and Yaru became presets, and Appearance gained Reduce transparency (`reduce-transparency`). zz kept its keys; `chrome-tint` and `chrome-tint-hue` now set the theme's neutral, which defaults to a warm 0.15 at hue 60 where the tint used to default to none at 260.
- **No presets to pick (2026-11-06).** Picking default, macOS or Yaru and then overriding the accent and the radius left the presets saying almost nothing. The look starts from the platform's theme file and every knob is a row: `control-shape` and `interface-glass` joined the keys, `chrome-accent` took `ink`, `interface-theme` and the `themes/` folder were retired with saved themes on every client, and the palette's select theme became select accent. Thin clients' older preferences keep their knobs; they used to drop them when no theme was picked.
- **Chrome wears the terminal theme (2026-11-07).** With `chrome-terminal-colors` on (the default), a themed terminal hands the chrome its colors through penna's `colors` knob: `background` exactly, so a flush pane and the window are one surface, and `foreground` lifted to 7:1 on it when the theme's text reads under that (2026-11-08: a theme with 4.95:1 text left the interface's muted text near 3:1 on a card); the accent from `cursor-color`, else palette 4, 12, 5, 13, 6 or 14, the first with OKLCH chroma 0.04 or more that isn't within 20° of palette 1's hue; danger, success and warning from palette 1, 2 and 3, or their bright slots when the normal one reads under 4.5:1 on the background, taken only when they read as red, green and yellow (chroma 0.06 or more, within 45° of the hue), else the kit's. penna then fits accent and status to 4.5:1 and lightens the muted mix as needed. zz's mode decides, not the terminal (2026-11-08): the chrome wears the terminal's colors only while the terminal is light in light mode or dark in dark mode (a background lighter than the foreground is light, `TerminalAppearance::background_scheme`), and otherwise keeps the look's own colors for the mode, so a dark theme never turns a light window dark. A `light:`/`dark:` pair switches with the mode, and a lone theme paints only its own. An untouched terminal (zz's built-in colors) leaves the chrome alone, and a person's Accent beats the terminal's. Over all 821 themes Ghostty ships, the accent needs no lightness move in 70% and red comes straight from the palette in 73%. Thin clients still derive their terminal palette from the theme (`theme_terminal_colors`) and don't follow the terminal yet.
- **Glass owns the window, Frost sets how thick (2026-11-09).** Glass, Reduce transparency and Window blur were three switches for two choices, and Reduce transparency did exactly what Glass off did. Now Glass on also blurs the window where the compositor can, `interface-frost` (Frost, 0 to 100%, 50% by default) scales the glass blur and the window's opacity together (0.86 to 1.0, 0.93 at the default, opaque at 100%), and the system's own Reduce transparency (macOS, iOS, Windows' Transparency effects, a browser's `prefers-reduced-transparency`) turns glass off without a row of ours. `window-background-blur` and `reduce-transparency` are reported as removed. One cost: Glass is on by default, so a fresh config gets the blurred window too.

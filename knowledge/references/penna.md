---
type: Reference
title: penna
description: How to build an app on penna, the widget kit on ala - the dependency and its features, dev profiles, licenses, the setup order (penna::init, Root, activating a theme), roles and themes, the Appearance page, the kit pieces apps reuse, a storybook catalog and the lint list, with nidus as the worked example.
resource:
- libs/penna/Cargo.toml
- libs/penna/src/lib.rs
- libs/penna/src/foundation/interface_theme.rs
tags: [penna, ala, ui, theme, storybook, reference]
timestamp: 2026-11-03T00:00:00Z
---

# Overview

penna is the widget kit on [ala](/references/ala.md): the theme and its surface roles, primitives,
widgets, settings pages with an Appearance page, window chrome, phone chrome (`penna::blocks::compact`,
`penna::blocks::touch`), icons, a CSS token writer (`penna::foundation::theme_css`) and an HTML kit (`web/`). It lives
in [autistic-computer/penna](https://github.com/autistic-computer/penna) and comes back into this
repo as the git subtree `libs/penna`, with no zz dependencies. zz uses it by path; nidus
([autistic-computer/nidus](https://github.com/autistic-computer/nidus), `crates/nid-app`) keeps it
in the same `libs/penna` subtree and is the worked example below. `libs/penna/storybook` (crate `penna-storybook`) is the
storybook harness with the theme creator, and penna's own page.

This page is the setup for a new app. The pieces have their own pages: the
[design system](/designs/design-system.md), [surface roles](/designs/surface-roles.md),
[interface themes](/designs/interface-themes.md), [UI conventions](/configuration/ui-conventions.md)
and the [storybook](/playbooks/storybook.md).

# Depend on it

```toml
[dependencies]
ala = { git = "https://github.com/autistic-computer/ala", branch = "main", features = ["default"] }
ala-platform = { git = "https://github.com/autistic-computer/ala", branch = "main", features = ["font-kit", "wayland", "x11"] }
penna = { git = "https://github.com/autistic-computer/penna", branch = "main" }
```

penna depends on ala with `default-features = false`, so the app picks ala's features itself.
ala's `default` is `font-kit`, `wayland`, `x11` and `windows-manifest` (its build script embeds a
manifest into every Windows binary). nidus asks for `default` on ala and `font-kit`, `wayland` and
`x11` on ala-platform, plus `runtime_shaders` on macOS. zz asks for `default` on ala in
`crates/zz` and `crates/zz-ui`, and for the three ala-platform features in `crates/zz`, without
`runtime_shaders`.

Both libs follow `main` and have no tags yet, so the app's own `Cargo.lock` is what pins them, and
`cargo update -p penna` can bring API changes. penna commits `Cargo.lock` and
`storybook/Cargo.lock` for its own CI only; Cargo ignores a dependency's lockfile.

To build against penna or ala edits that are not pushed yet, add an untracked
`.cargo/config.toml` to the app's checkout, list it in `.git/info/exclude`, and point it at a
zebrilus checkout:

```toml
[patch."https://github.com/autistic-computer/ala"]
ala = { path = "../zebrilus/libs/ala/crates/ala" }
ala-platform = { path = "../zebrilus/libs/ala/crates/ala-platform" }

[patch."https://github.com/autistic-computer/penna"]
penna = { path = "../zebrilus/libs/penna" }
penna-storybook = { path = "../zebrilus/libs/penna/storybook" }
```

Never put that file in a folder above several checkouts: a `[patch]` in Cargo config beats one in
`Cargo.toml`, so it would override zebrilus's own patch too. For a one-off check, pass the same
patches to Cargo as `--config 'patch."https://github.com/autistic-computer/penna".penna.path="<path>"'`,
and for an app storybook also
`--config 'patch."https://github.com/autistic-computer/penna".penna-storybook.path="<path>/storybook"'`.

## Features

None is on by default.

| Feature | What it adds |
| --- | --- |
| `editor` | `penna::blocks::code_editor` (the code editor widget) and nothing else |
| `tree-sitter` | the tree-sitter parser and its JSON, Markdown, Rust and TOML grammars, so the code editor and Markdown code blocks highlight. The highlighter API (`SyntaxHighlighter`, `LanguageConfig::new` and `plain`, `LanguageRegistry`) is the same with it off, so app code compiles either way. An app adds a grammar with `LanguageRegistry::singleton().register(name, &LanguageConfig::new(..))`, the way `zz_ui::init` adds tmux |
| `tray` | `penna::blocks::tray`, a tray or menu bar icon with a menu: StatusNotifierItem on Linux, `NSStatusItem` on macOS, `Shell_NotifyIcon` on Windows |

## Toolchain and dev profiles

penna is edition 2024 with `rust-version = "1.99"`, and its CI runs the pinned 1.99.0
(`rust-toolchain.toml`, which also adds the wasm32 target). It pins `ropey = "=2.0.0-beta.1"`, a
pre-release whose `Rope` is in penna's API, so an app gets that exact version.

Cargo profiles are not inherited from a dependency, so an app copies the block zz and nidus use
into its own workspace `Cargo.toml`:

```toml
[profile.dev.package]
"*" = { opt-level = 2 }
taffy = { opt-level = 3 }
ala = { opt-level = 3 }
ala-macros = { opt-level = 2 }
ala-platform = { opt-level = 2 }
```

Without it a debug build is too slow to use: the storybook drew one theme creator frame in about
100 ms. `"*"` covers dependencies but not workspace members, so the app's own crates stay at
`opt-level = 0`.

## Licenses

penna is `Apache-2.0 AND CC-BY-4.0`. Its icons (`IconName`, drawn from `src/components/icon/solar.json`) are
adapted from the Solar Icons Set by 480 Design under CC BY 4.0, so an app that ships them credits
the set in its own notices or About page, as penna's `NOTICE.md` and `src/components/icon/LICENSE-SOLAR` do. The
kit's code started as gpui-component (Apache-2.0), credited in the same `NOTICE.md`.
`penna-storybook` is `Apache-2.0 AND OFL-1.1` for the Inter and Lilex fonts its page bundles.

# Set up an app

The order matters, and penna panics with a message that names the fix when it is wrong.

1. **Define no roles.** penna owns every surface role, every theme's patches for them, the text
   styles and the spacing scale; an app composes penna's components (penna's
   `knowledge/designs/kit.md`). zz and nidus register no roles or theme layers.
2. **Give the application penna's assets** with `.with_assets(penna::Assets)`, or the icons draw
   nothing. An app with assets of its own passes an `AssetSource` that falls back to
   `penna::Assets` for paths it does not have, as `zz_ui::Assets` does. Then **`penna::init(cx)`**
   binds the kit's keys and actions (inputs, menus, dialogs, popovers, selects, text, the code
   editor with `editor`) and activates the built-in Default theme in the OS appearance.
   A native app also sets `penna::blocks::appearance::AvailableFonts(platform.text_system())`
   as a global, so the UI font picker lists only installed families
   (`crates/zz/src/lib.rs` creates it before the platform moves into the application).
3. **Open every window with `Root`** as its root view:
   `cx.new(|cx| Root::new(view, window, cx))`. Root sets the window's corner smoothing
   (`CORNER_SMOOTHING`) and adaptive corner fraction, draws the client-side frame
   (`bordered(true)` by default; nidus passes `cfg!(target_os = "linux")`), the window's text
   selection, and the dialog and toast layers over the view. `own_layers(false)` leaves the layers
   to the app, which then places `Root::render_dialog_layer` and `render_notification_layer`
   itself. `penna::TitleBar::title_bar_options(Theme::global(cx).title_bar_height())` gives the
   window's `titlebar` options.
4. **Apply a theme**, and again whenever the OS appearance changes. Root does not do this, because
   it cannot know what the app sets over the theme. `InterfaceTheme::activate(mode, window, cx)`
   switches the mode (`None` follows the window's appearance, `Some(ThemeMode::Dark)` pins one),
   then writes the theme's colors for that mode, its materials and its role patches into the
   `Theme` global in one step, and refreshes the windows; `default_theme().activate(None,
   Some(window), cx)` is the whole call for an app without settings. The font is one of the
   materials: a theme with no font sets ala's system UI font (`ala::Font::default()`), as
   `Appearance::apply` does when no UI font is picked, so the last theme's font never stays.
   With an Appearance page, `Appearance::apply` does it (below). Call it from the root view's
   `cx.observe_window_appearance(window, ..)`, and set anything the app lays over the theme after
   it in the same callback, the way zz's `apply_zz_overrides` sets fonts, contrast and the pane
   look (`crates/zz/src/theme.rs`); an app with a font of its own sets it there.
   `Theme::change` and `Theme::sync_system_appearance` on their own only swap in penna's fallback
   palette for the mode and keep the last theme's materials and role patches; follow them with the
   theme's `apply`, or call `activate`, which does both.

```rust
fn main() {
    let platform = ala_platform::current_platform(false);
    let fonts = penna::blocks::appearance::AvailableFonts(platform.text_system());
    let application = ala::Application::with_platform(platform).with_assets(penna::Assets);
    application.run(move |cx| {
        penna::init(cx);
        cx.set_global(fonts);
        let opened = cx.open_window(ala::WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| MainView::new(window, cx));
            cx.new(|cx| penna::Root::new(view, window, cx))
        });
        if let Ok(handle) = opened {
            penna::foundation::overlay::set_toast_host(handle, cx);
        }
    });
}
```

Dialogs and toasts go through `WindowExt` on the window (`window.open_dialog`,
`window.push_notification`), which logs and does nothing in a window whose root is not a Root.
Code that has only `App`, such as a background task, calls `penna::foundation::overlay::push_toast` after
`set_toast_host` named the window to show it in.

# Roles and themes

Every painted surface names a role ([surface roles](/designs/surface-roles.md)), and every role is
penna's: the kit's kinds and groups (`SurfaceRole::KIT`), its chrome (`penna::foundation::roles::CHROME`) and
the recipes its components pick by variant (`penna::foundation::roles::RECIPES`: `tab.window`,
`field.composer`, `button.send` and the rest). A theme's tweak to a piece of UI lives in
`libs/penna/themes/*.json` under that recipe's id. Ids that are gone stay in penna's retired list,
so an old theme file loads with those entries dropped. `parse_theme` reads a user's file with the
same checks: an unknown key or role id is an error, and when it matches a real one apart from case,
`-` or `_` (and `.` for a role id), or is the part of a role id after a dot, the error names the one
meant. The file format is in [interface themes](/designs/interface-themes.md).

# The Appearance page

`penna::foundation::interface_theme::Appearance` is the model: `mode` (`system`, `light` or `dark`),
the `ThemeOverrides` flattened in (`radius`, `accent`, `tint`, `tint-hue`, `controls`, `glass`),
`contrast`, `reduce_transparency`, `ui_font_family`, `zoom`, `motion` (`off`, `default` or
`fast`) and `density`. It is serde with defaults, so an app keeps it in its own preferences with
`#[serde(flatten)]` (`clients/app/src/preferences.rs`), and those key names are what users' files
hold. There is no theme to pick: the look starts from `platform_theme()` (the `macos` file on macOS
and iOS, `default` elsewhere) and each override is a knob over it.

- `Appearance::change(AppearanceChange)` applies one edit from the page; `Import` sets every knob
  from a theme file (`ThemeOverrides::from_theme` and its `face`, leaving the mono font), `ResetLook`
  clears them and the mono font.
- `Appearance::shown_theme()`, `corner_radius()`, `neutral()`, `controls()`, `glass()`, `accent()`
  and `ui_font()` read the look with the platform's value wherever a knob is unset.
- `Appearance::apply(window, cx)` puts it on screen: reduce motion and animation speed from `motion`, the
  platform's file with its overrides (`shown_theme`) activated in the mode, the UI font (when
  installed, else ala's system UI font), the mono font (when installed, else
  `platform_mono_font_family()`; an app with a default of its own, like the terminal font, sets it
  after), contrast and zoom.
- `penna::blocks::appearance::appearance_page(title, appearance_page_items::<T>(), render)` lays
  the rows out in a virtual column, in the groups `LOOK`, `SIZE` and `ACCESSIBILITY`, and `render`
  draws each item with
  `kit_row(item, &appearance, &controls, stacked, window, cx)`, which gives a titled
  `SettingEntry` for every kit row. It returns `None` for `AppearancePageItem::App(T)`, the app's
  own rows, which the app inserts into the item list (in groups of its own) and draws. `stacked` puts swatches and sliders under their titles for narrow pages.
  `render` gets `(item, position, window, cx)`: set `.position(position)` on the entry `kit_row`
  returns and on the app's own rows, or every row draws as a middle row, without the rounded ends
  and outer borders of its card.
- `AppearanceControls::new(&appearance, on_change, window, cx)` holds the accent picker, the font
  select and the zoom field's focus, and reports edits as `on_change(AppearanceChange, window,
  cx)`. Call `sync` when the appearance changed elsewhere. The page has no theme file row; an app that
  reads theme files sets every knob from one with `AppearanceChange::Import` and clears them with
  `AppearanceChange::ResetLook`.

The Kit story's Appearance page (`libs/penna/storybook/src/kit/appearance.rs`) is the smallest
complete page. zz's two pages wrap the same rows with its own `ZzRow` items
(`zz_ui::settings::appearance`).

# Kit pieces apps reuse

- Window shell: `penna::blocks::shell::app_shell_surface` (window fill, the frame fill under the
  title bar and content, sidebar, title bar, content, overlays; zz passes `zz_ui::shell::shell_frame`,
  the sidebar's own fill, so gapped panes sit on the sidebar's color whether or not the window is
  blurred),
  `app_titlebar_strip`, `window_drag_handle`, and the sidebar parts in `penna::blocks::sidebar`.
- Settings: `SettingsStack`, `SettingEntry` and the page columns in `penna::blocks::settings`;
  `settings_navigation(id, sections, selected)` with `NavSection` and `on_pick`/`on_back` for the
  section list; `penna::blocks::settings::about` (`About`, `AboutLink`, `about_hero`,
  `about_project_stack`, `about_build_stack`) for an About page. A button, select, check,
  segmented control or input on a settings card takes `.on(settings_card_ground(cx))`, or it paints
  against the window instead of the card. A switch takes no ground.
- Feedback: `penna::blocks::feedback::callout(tone, icon, title, message, cx)` on the `inline-panel` role
  in any tone, `inline_alert` for a failure in place, and `empty_state` for a centered page
  message.
- Lists: `penna::components::tree::IndentGuides`, the indent guides for a uniform list tree, on the
  `rule.tree` role.
- Phone chrome: `penna::blocks::compact` (the floating pill, its menu, folding title bars, the bento grid)
  and `penna::blocks::touch` (pager, key row, arrow pad and the rest).
- HTML: in the penna repo, `cargo run --example theme_css -- <dir>` writes every built-in
  theme's tokens as CSS for both modes, which `web/penna.css` and `web/kit.js` paint with
  (`penna::foundation::theme_css::write_tokens` from code).

# A storybook for the app

An app's storybook is a wasm crate in its own workspace that depends on `penna` and
`penna-storybook` (both from the penna repo) and `wasm-bindgen` on wasm, and exports a `run` built
on a `Catalog` ([storybook](/playbooks/storybook.md) has every field):

```rust
pub static CATALOG: Catalog = Catalog {
    title: "board storybook",
    name: "board",
    storage: "board-storybook",
    init: board::init,
    logo: IconName::Layers.path(),
    plates: &[BOARD_PLATE, penna_storybook::creator::CONTROLS],
    ..PENNA
};

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run() -> Result<(), wasm_bindgen::JsValue> {
    penna_storybook::start(&CATALOG, penna::Assets)
}
```

`..PENNA` keeps penna's stories, knobs and floating sections for every field the app leaves out,
so a field added later does not break it. An app that sets its own `stories` without
`penna_storybook::KIT` also sets `floating` to its own list, `&[]` when it has none: penna's list
names Kit stories, and `problems()` reports each one the catalog lacks. `plates` puts the app's own UI into the theme creator so the roles it draws can be tuned. A host test installs the catalog and asserts `CATALOG.problems()` is empty
(`libs/penna/storybook/tests/creator.rs` plays such an app). Give the workspace the dev profile
block above, and set `penna`, `penna-storybook` and the app's crates to `opt-level = 0` in it:
optimized and incremental, an edit once left an rlib that failed to link on wasm with undefined
`anon.*.llvm.*` symbols.

`storybook/build.sh --manifest <app>/storybook/Cargo.toml [--release] [--out <dir>]`, run from
any folder, builds the page in the manifest's folder, so the app's `.cargo/config.toml` patches
and its `rust-toolchain.toml` apply: the crate's library as a wasm32 cdylib (with its `page` and
`syntax-highlighting` features when it has them), bound by the wasm-bindgen CLI at the version the
app's lockfile resolves (the script says how to install it when it is missing), and penna's
`index.html`, `main.js` and `style.css` beside it. From a Cargo git
checkout the script sits next to `penna-storybook`'s manifest, which `cargo metadata` reports.
Serve the output folder with any static server and open it in a browser with WebGPU.

# Lints

penna's rules are lints. `libs/penna/storybook/clippy.toml` holds the list an app copies into its
own `clippy.toml` (`disallowed-methods` and `disallowed-types`): no raw `ala::rgb`, `rgba` or
`hsla` (colors come from `cx.theme()`), none of `Colorize`'s mixing transforms or `Hsla::opacity`
(name a tone and an emphasis), and no hand-drawn borders, shadows or `BoxShadow`, nor the kit
helpers that would skip a look (draw with a kit component's look). A piece that breaks one on purpose
takes a scoped `#[allow(clippy::disallowed_methods, reason = "..")]`. zz's root `clippy.toml`
carries the same list, the kit lint against painting a role or setting text by hand (penna's
`knowledge/designs/kit.md`), and a few rules of zz's own.

# nidus, the worked example

nidus was the first app on penna outside zz:

- `Cargo.toml`: the dependency lines above (penna with `tray`), the dev profile block.
- `crates/nid-app/src/theme.rs`: `init` and `apply`, and no roles: the board, its chips, the
  first-run steps and the banners draw with penna's `Well`, `Tile`, `Chip`, `Steps` and `Notice`.
- `crates/nid-app/src/main.rs`: the window, opened with `TitleBar::title_bar_options`, client
  decorations except on X11, and `Root::new(view, window, cx).bordered(cfg!(target_os = "linux"))`.
- `crates/nid-app/src/window.rs`: the shell from `app_shell_surface`, the sidebar and settings
  navigation rows, and `observe_window_appearance` applying the theme again.

nidus was written before Root drew the dialog and toast layers and before `activate` existed. When
it moves to this penna it drops its own `Root::render_dialog_layer` and `render_notification_layer`
lines in `window.rs` (or sets `own_layers(false)`), or it draws them twice; its `theme::apply`
becomes `default_theme().activate(None, window, cx)`, since `Root::new` sets the corners; and its
callout and banner widgets can become `feedback::callout`.

# Checks

From zebrilus, `just libs check penna` runs what penna's CI runs: formatting, clippy with all
features for the host and with `page` for wasm, clippy and the highlighter test without features,
and the tests of both crates. penna's CI (`.github/workflows/ci.yml`) adds a daily run against the
latest ala and crates, a Windows lint of `tray` and `editor`, and a release build of the storybook
page with the HTML kit, kept as the `storybook-page` artifact.

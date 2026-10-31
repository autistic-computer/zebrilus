---
type: Reference
title: penna
description: How to build an app on penna, the widget kit on ala - the dependency and its features, dev profiles, licenses, the setup order (penna::register, penna::init, Root, activating a theme), roles and theme layers, the Appearance page, the kit pieces apps reuse, a storybook catalog and the lint list, with nidus as the worked example.
resource:
- libs/penna/Cargo.toml
- libs/penna/src/lib.rs
- libs/penna/src/interface_theme.rs
tags: [penna, ala, ui, theme, storybook, reference]
timestamp: 2026-10-31T00:00:00Z
---

# Overview

penna is the widget kit on [ala](/references/ala.md): the theme and its surface roles, primitives,
widgets, settings pages with an Appearance page, window chrome, phone chrome (`penna::compact`,
`penna::touch`), icons, a CSS token writer (`penna::theme_css`) and an HTML kit (`web/`). It lives
in [autistic-computer/penna](https://github.com/autistic-computer/penna) and comes back into this
repo as the git subtree `libs/penna`, with no zz dependencies. zz uses it by path; nidus
([autistic-computer/nidus](https://github.com/autistic-computer/nidus), `crates/nid-app`) uses it
from git and is the worked example below. `libs/penna/storybook` (crate `penna-storybook`) is the
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
| `editor` | `penna::code_editor` (the code editor widget) and nothing else |
| `tree-sitter` | the tree-sitter parser and its JSON, Markdown, Rust and TOML grammars, so the code editor and Markdown code blocks highlight. The highlighter API (`SyntaxHighlighter`, `LanguageConfig::new` and `plain`, `LanguageRegistry`) is the same with it off, so app code compiles either way. An app adds a grammar with `LanguageRegistry::singleton().register(name, &LanguageConfig::new(..))`, the way `zz_ui::init` adds tmux |
| `tray` | `penna::tray`, a tray or menu bar icon with a menu: StatusNotifierItem on Linux, `NSStatusItem` on macOS, `Shell_NotifyIcon` on Windows |

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

penna is `Apache-2.0 AND CC-BY-4.0`. Its icons (`IconName`, drawn from `src/icon/solar.json`) are
adapted from the Solar Icons Set by 480 Design under CC BY 4.0, so an app that ships them credits
the set in its own notices or About page, as penna's `NOTICE` and `src/icon/LICENSE-SOLAR` do. The
kit's code started as gpui-component (Apache-2.0), credited in the same `NOTICE`.
`penna-storybook` is `Apache-2.0 AND OFL-1.1` for the Inter and Lilex fonts its page bundles.

# Set up an app

The order matters, and penna panics with a message that names the fix when it is wrong.

1. **Register** the app's roles, retired role ids and theme layers once at startup, before
   `penna::init` and before anything reads a theme or a role:
   `penna::register(ROLES, RETIRED, LAYERS)`. An app with none of them skips it; one with roles
   only can call `penna::roles::register(ROLES)`, as nidus does. Calling again with the same
   lists is fine (`zz_ui::register` runs before every zz-ui theme lookup); different lists, a role
   id taken from a kit role, an id a theme file cannot spell (lowercase letters, `-` and `.`), the
   same id twice, or layers registered after a theme was read each panic. `penna::register` also
   parses the layers before it returns, so a layer penna cannot read panics at that call, not
   later inside the application's launch; the message names the theme, says when the app's layer
   is the one that failed, and quotes the parse error.
2. **Give the application penna's assets** with `.with_assets(penna::Assets)`, or the icons draw
   nothing. An app with assets of its own passes an `AssetSource` that falls back to
   `penna::Assets` for paths it does not have, as `zz_ui::Assets` does. Then **`penna::init(cx)`**
   binds the kit's keys and actions (inputs, menus, dialogs, popovers, selects, text, the code
   editor with `editor`) and activates the built-in Default theme in the OS appearance.
   A native app also sets `penna::settings::appearance::AvailableFonts(platform.text_system())`
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
static LAYERS: [(&str, &str); 1] = [("macos", include_str!("../themes/macos.json"))];

fn main() {
    penna::register(theme::ROLES, &[], &LAYERS);
    let platform = ala_platform::current_platform(false);
    let fonts = penna::settings::appearance::AvailableFonts(platform.text_system());
    let application = ala::Application::with_platform(platform).with_assets(penna::Assets);
    application.run(move |cx| {
        penna::init(cx);
        cx.set_global(fonts);
        let opened = cx.open_window(ala::WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| MainView::new(window, cx));
            cx.new(|cx| penna::Root::new(view, window, cx))
        });
        if let Ok(handle) = opened {
            penna::overlay::set_toast_host(handle, cx);
        }
    });
}
```

Dialogs and toasts go through `WindowExt` on the window (`window.open_dialog`,
`window.push_notification`), which logs and does nothing in a window whose root is not a Root.
Code that has only `App`, such as a background task, calls `penna::overlay::push_toast` after
`set_toast_host` named the window to show it in.

# Roles and theme layers

Every painted surface names a role ([surface roles](/designs/surface-roles.md)). penna brings the
kit's kinds and groups (`SurfaceRole::KIT`: `button`, `menu-item`, `floating` and so on) and its
chrome roles (`penna::roles::CHROME`: `window`, `titlebar`, `chrome.button`, `sidebar.row`,
`rule.tree`, `inline-panel`, `picker`, the `phone.*` roles and more). An app adds its own:

```rust
pub const BOARD: SurfaceRole = SurfaceRole::new("board", Kind::Card, "Arrangement", "The well the screens sit on")
    .with(StylePatch::EMPTY.shadow(Shadow::None));
pub const CHIP: SurfaceRole = SurfaceRole::under(&SurfaceRole::BADGE, "board.chip", "Computer names", "The name above each computer");
pub const ROLES: &[SurfaceRole] = &[BOARD, CHIP];
```

`SurfaceRole::new` hangs a role under a kind, `SurfaceRole::under` under a kit role or one of the
app's own, and `.with` gives it a built-in look. Elements draw it with `.surface(ROLE, cx)`,
`.rule_on(ROLE, edge, cx)` or a widget's `.surface_role(ROLE)`. Role ids never change once a theme
file can name them; a role that goes away keeps its id in the retired list, so an old file loads
with the entry dropped. An app may list penna's chrome roles unchanged to set their order;
any other role under a chrome id panics at register, because penna's widgets draw their own
consts. `penna::roles::all`, `find` and `members` list the kit's, penna's chrome and the app's
roles together.

A theme layer is `(theme id, JSON)`. Under a built-in id (`default`, `macos`, `yaru`, the
`BUILTIN_THEMES`), it is laid over penna's file key by key: objects merge, and any other value
replaces (`interface_theme::parse_layers`). An app's layer usually holds only `roles` patches for
its own roles, as zz's `crates/zz-ui/themes/*.json` do. Under a new id it adds a whole theme, where
missing materials take `Materials::default()` and missing colors the built-in palette. The file
format is in [interface themes](/designs/interface-themes.md). `builtin_themes`, `builtin_theme(id)`
and `default_theme` read the registered result, and `parse_theme` reads a user's file with the same
checks: an unknown key or role id is an error. When it matches a real one apart from case, `-` or
`_` (and `.` for a role id), or is the part of a role id after a dot (`preview` for
`settings.preview`), the error names the one meant; otherwise an unknown key lists the keys
allowed there.

# The Appearance page

`penna::interface_theme::Appearance` is the model: `mode` (`system`, `light` or `dark`), `theme`
(an id), the `ThemeOverrides` flattened in (`radius`, `accent`, `tint`, `tint-hue`), `contrast`,
`ui_font_family`, `zoom`, `animations` and `animation_speed`. It is serde with defaults, so an app
keeps it in its own preferences with `#[serde(flatten)]` (`clients/app/src/preferences.rs`), and
those key names are what users' files hold. The user's own themes are `SavedThemes`, a slice of
`(id, InterfaceTheme)`; `&[]` when the app has none.

- `Appearance::change(AppearanceChange, saved)` applies one edit from the page.
- `Appearance::apply(saved, window, cx)` puts it on screen: reduce motion and animation speed, the
  theme with its overrides (`shown_theme`) activated in the mode, the UI font (when installed,
  else ala's system UI font), contrast and zoom.
- `penna::settings::appearance::appearance_page(title, appearance_page_items::<T>(advanced),
  render)` lays the rows out in a virtual column, and `render` draws each item with
  `kit_row(item, &appearance, saved, &controls, stacked, window, cx)`, which gives a titled
  `SettingEntry` for every kit row. It returns `None` for `AppearancePageItem::App(T)`, the app's
  own rows, which the app inserts into the item list and draws, and for `Advanced`, the disclosure
  header the app draws with `settings_list_disclosure_header(ADVANCED_DISCLOSURE, "Advanced",
  expanded)` and toggles. `stacked` puts swatches and sliders under their titles for narrow pages.
  `render` gets `(item, position, window, cx)`: set `.position(position)` on the entry `kit_row`
  returns and on the app's own rows, or every row draws as a middle row, without the rounded ends
  and outer borders of its card.
- `AppearanceControls::new(&appearance, on_change, window, cx)` holds the accent picker, the font
  select and the zoom field's focus, and reports edits as `on_change(AppearanceChange, window,
  cx)`. Call `sync` when the appearance changed elsewhere; `theme_menu` adds a more menu beside
  the Theme row (zz puts Import and Delete there).

The Kit story's Appearance page (`libs/penna/storybook/src/kit/appearance.rs`) is the smallest
complete page. zz's two pages wrap the same rows with its own `ZzRow` items
(`zz_ui::settings::appearance`).

# Kit pieces apps reuse

- Window shell: `penna::shell::app_shell_surface` (sidebar, title bar, content, overlays),
  `app_titlebar_strip`, `window_drag_handle`, and the sidebar parts in `penna::sidebar`.
- Settings: `SettingsStack`, `SettingEntry` and the page columns in `penna::settings`;
  `settings_navigation(id, sections, selected)` with `NavSection` and `on_pick`/`on_back` for the
  section list; `penna::settings::about` (`About`, `AboutLink`, `about_hero`,
  `about_project_stack`, `about_build_stack`) for an About page. A button, select, check,
  segmented control or input on a settings card takes `.on(settings_card_ground(cx))`, or it paints
  against the window instead of the card. A switch takes no ground.
- Feedback: `penna::feedback::callout(tone, icon, title, message, cx)` on the `inline-panel` role
  in any tone, `inline_alert` for a failure in place, and `empty_state` for a centered page
  message.
- Lists: `penna::list::tree::IndentGuides`, the indent guides for a uniform list tree, on the
  `rule.tree` role.
- Phone chrome: `penna::compact` (the floating pill, its menu, folding title bars, the bento grid)
  and `penna::touch` (pager, key row, arrow pad and the rest).
- HTML: in the penna repo, `cargo run --example theme_css -- <dir>` writes every built-in
  theme's tokens as CSS for both modes, which `web/penna.css` and `web/kit.js` paint with
  (`penna::theme_css::write_tokens` from code).

# A storybook for the app

An app's storybook is a wasm crate in its own workspace that depends on `penna` and
`penna-storybook` (both from the penna repo) and `wasm-bindgen` on wasm, and exports a `run` built
on a `Catalog` ([storybook](/playbooks/storybook.md) has every field):

```rust
pub static CATALOG: Catalog = Catalog {
    title: "board storybook",
    name: "board",
    storage: "board-storybook",
    register: board::register,
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
names Kit stories, and `problems()` reports each one the catalog lacks. `register` is the app's
`penna::register` call, and `plates` puts the app's own UI into the theme creator so its roles can
be tuned. A host test installs the catalog and asserts `CATALOG.problems()` is empty
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
helpers that would skip a role (draw through a surface role). A piece that breaks one on purpose
takes a scoped `#[allow(clippy::disallowed_methods, reason = "..")]`. zz's root `clippy.toml`
carries the same list and a few rules of zz's own.

# nidus, the worked example

nidus was the first app on penna outside zz:

- `Cargo.toml`: the dependency lines above (penna with `tray`), the dev profile block.
- `crates/nid-app/src/theme.rs`: five roles (`board`, `board.screen`, `board.chip`, `board.found`,
  `banner`), registered with `penna::roles::register` before `penna::init`.
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

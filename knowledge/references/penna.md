---
type: Reference
title: penna
description: How to build an app on penna, the widget kit on ala - the dependency and its features, dev profiles, licenses, the setup order (penna::init, Root, activating a theme), roles and themes, the Appearance page, the kit pieces apps reuse, a storybook catalog and the lint list, with nidus as the worked example.
resource:
- libs/penna/Cargo.toml
- libs/penna/src/lib.rs
- libs/penna/src/foundation/interface_theme.rs
tags: [penna, ala, ui, theme, storybook, reference]
timestamp: 2026-11-24T00:00:00Z
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

With these git dependencies the app's `Cargo.lock` pins the revisions.
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
| `tree-sitter` | the tree-sitter parser and its grammars (Bash, C, CSS, diff, Go, HTML, JavaScript, JSON, Markdown, Python, Rust, TOML, TypeScript and TSX, YAML), so the code editor and Markdown code blocks highlight. The highlighter API (`SyntaxHighlighter`, `LanguageConfig::new` and `plain`, `LanguageRegistry`) is the same with it off, so app code compiles either way. An app adds a grammar with `LanguageRegistry::singleton().register(name, &LanguageConfig::new(..))`, the way `zz_ui::init` adds tmux |
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
   `libs/penna/knowledge/designs/kit.md`). zz and nidus register no roles or theme layers.
2. **Give the application penna's assets** with `.with_assets(penna::Assets)`, or the icons draw
   nothing. An app with assets of its own passes an `AssetSource` that falls back to
   `penna::Assets` for paths it does not have, as `zz_ui::Assets` does. Then **`penna::init(cx)`**
   binds the kit's keys and actions (inputs, menus, dialogs, popovers, selects, text, the code
   editor with `editor`) and activates `platform_theme()` in the OS appearance (macOS theme on macOS and iOS, Default elsewhere).
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
   `Theme` global in one step, and refreshes the windows. An app following the platform uses `platform_theme().activate(None, Some(window), cx)`. The font is one of the
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

`Appearance` in `libs/penna/src/foundation/interface_theme.rs` stores the mode,
flattened theme overrides, UI, mono and chat font families, contrast, zoom,
motion, density and pointer preference. It deserializes with defaults. The thin
client flattens it into its preferences (`clients/app/src/preferences.rs`).

- `change` applies one edit. `Import` replaces the theme overrides and UI font.
  `ResetLook` clears the overrides and all three font choices.
- `shown_theme` combines the platform theme and overrides. `apply` activates it
  in the chosen mode, resolves installed fonts and applies motion, contrast,
  density, pointer and zoom. The UI and mono fonts fall back to ala's system UI
  font and penna's platform mono font respectively. An unavailable chat font
  leaves the chat override unset.
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
  cx)`. Call `sync` when the appearance changed elsewhere. The app can call `AppearanceChange::Import` from its own theme file picker.

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

An app's wasm entry calls `penna_storybook::start(&CATALOG, assets)`.
`Catalog` in `libs/penna/storybook/src/catalog.rs` names its stories, plates,
floating sections, knobs, logo and initialization. Use `..PENNA` to inherit the
kit defaults. If replacing the Kit stories, supply matching `floating` entries
(or `&[]`). A host test can assert that `CATALOG.problems()` is empty.
The [storybook playbook](/playbooks/storybook.md) covers authoring and checking plates and stories.

Use `libs/penna/storybook/build.sh --manifest <path-to-Cargo.toml>` to build an
app's page. It resolves the toolchain and Cargo config in the manifest's directory
and requires the wasm-bindgen CLI version in the app's lockfile. Serve the output
with a static server in a WebGPU-capable browser. The zz storybook keeps penna,
the harness and its own crates at `opt-level = 0` in its dev profile. Optimized
incremental wasm builds previously left an rlib with undefined `anon.*.llvm.*` symbols.

# Lints

penna's rules are lints. `libs/penna/storybook/clippy.toml` holds the list an app copies into its
own `clippy.toml` (`disallowed-methods` and `disallowed-types`): no raw `ala::rgb`, `rgba` or
`hsla` (colors come from `cx.theme()`), none of `Colorize`'s mixing transforms or `Hsla::opacity`
(name a tone and an emphasis), and no hand-drawn borders, shadows or `BoxShadow`, nor the kit
helpers that would skip a look (draw with a kit component's look). A piece that breaks one on purpose
takes a scoped `#[allow(clippy::disallowed_methods, reason = "..")]`. zz's root `clippy.toml`
carries the same list, the kit lint against painting a role or setting text by hand (`libs/penna/knowledge/designs/kit.md`), and a few rules of zz's own.

# nidus, the worked example

In the nidus repository, the workspace manifest uses the two subtrees under
`libs/` and enables penna's `tray` feature. Under `crates/nid-app/src/`:

- `main.rs` supplies `penna::Assets` and calls `theme::init`.
- `theme.rs` syncs appearance, applies the platform theme and refreshes windows.
- `lib.rs` opens the main `Root`, observes window appearance and chooses server
  decorations on X11. The root's border is enabled on Linux.
- `window.rs` composes the title bar and page with `app_shell_surface`.

# Checks

From zebrilus, `just libs check penna` runs what penna's CI runs: formatting, clippy with all
features for the host and with `page` for wasm, clippy and the highlighter test without features,
and the tests of both crates. penna's CI (`.github/workflows/ci.yml`) adds a daily run against the
latest ala and crates, a Windows lint of `tray` and `editor`, and a release build of the storybook
page with the HTML kit, kept as the `storybook-page` artifact.

---
type: Playbook
title: Working with the zz storybook
description: Run the zz storybook and penna's own, add a story, add an app's roles and plates to the theme creator, change a look and check it in every theme, and drive or screenshot it from an agent.
resource: libs/penna/storybook/src/lib.rs
tags:
- storybook
- zz-ui
- penna
- wasm
- agents
timestamp: 2026-10-30T00:00:00Z
---

# What it is

There are two pages on one harness. `clients/storybook` (crate `zz-storybook`) shows zz's own UI,
not a generic widget catalog: every piece of `zz-ui` and `penna` in its states, built from fixture
data with the same functions the app calls. `libs/penna/storybook` (crate `penna-storybook`) is
penna's own page, its theme creator, Foundation and Kit stories with penna's roles and assets
only, and the harness both pages run on. The page is plain HTML (navigation, prose, knobs). Each section of the
open story is its own ala window, mounted into the section's element with `WindowOptions::mount`
and sized to its content, so a section looks exactly like that piece does in the app.

`penna-storybook` holds the story types (`story.rs`: `Story`, `Section`, `stateless`, `states`,
`row`, `Framed`), the section host, the backdrop under floating sections, the theme knobs, the JSON
the page reads, the wasm exports, the page's `web/` files, the Inter and Lilex fonts the page
draws with (`fonts/`, OFL, noted in penna's `NOTICE`), the icon review sheet (`icons/`), the
theme creator (`src/creator`) and the Foundation and Kit stories (`src/foundation`, `src/kit`). An
app's storybook is that harness plus a `penna_storybook::Catalog`:

| Field | What it gives the page |
|---|---|
| `title`, `name` | the nav heading and tab titles (`zz storybook`, `zz`) |
| `storage` | the `localStorage` prefix: `<storage>-role-edits`, `<storage>-knobs-width` |
| `register`, `init` | what runs before built-in themes or roles are read (`zz_ui::register`), and the app's init (`zz_ui::init`) |
| `logo` | the icon path the Title bar and Icons stories show as the app's mark |
| `plates` | the theme creator's regions, in order; the creator opens the page (`#/theme-creator`) when the list is not empty. penna's are `penna_storybook::creator::PLATES` |
| `stories` | story lists in page order after the creator; penna's are `penna_storybook::FOUNDATION` and `KIT` |
| `knobs`, `apply` | app knobs on the App step (a `Knob` names its URL key, label, section heading and a `Control::Check` or `Control::Range`) and the function that applies them after the theme knobs; stories read them with `knobs().value(name)` or `flag(name)` |
| `floating` | `story` or `story/section` entries that draw the backdrop knob under them; penna's are `backdrop::KIT_FLOATING` |

zz's catalog is `zz_storybook::CATALOG` in `clients/storybook/src/lib.rs`: the creator's plates
(`PLATES` in `clients/storybook/src/stories/theme_creator.rs`), penna's Foundation, zz's agent,
workspace and console stories, then penna's Kit; the pane knobs
(`pane-gaps`, `pane-margin`, `pip-opacity`, `pane-inactive-opacity`, `pane-glow`, read through
`crate::pane()`) and `program-colors`; zz's floating sections; `zz_ui::Assets` and `zz_ui::ZZ_LOGO`.
Each page's cdylib defines its own `#[wasm_bindgen] run` that calls
`penna_storybook::start(&CATALOG, assets)`; every other export comes from `penna-storybook`, and
penna's own `run` is behind its `page` feature, so a linked app never exports two. Both pages load
`wasm/storybook.js`. `Catalog::problems` lists bad or duplicate ids (plates included) and floating
entries that name no section; each crate's tests assert it is empty for its catalog.

Each is its own wasm Cargo workspace, like `clients/web`, and each keeps a lockfile. penna's two
lockfiles pin ala from GitHub for the penna repo; in zebrilus every build of penna's page patches
its git dependencies on ala and ala-platform to `libs/ala` with `--config`, and the just recipe puts
the lockfiles back afterwards (`scripts/penna-locks.sh`). `penna-storybook`'s default `editor`
feature turns on penna's `editor` and the Code editor story; an app that leaves it out sets
`default-features = false`. The two workspaces' dev profiles optimize dependencies the way the root
workspace does (`[profile.dev.package]`): third-party crates and ala optimized, the crates they
edit (`penna`, `penna-storybook`, `zz-ui`, `zz-client`, `zz-protocol`, `zz-terminal`) not. Without it, a debug build drew one Theme creator frame in about 100 ms. Keep
those at `opt-level = 0`: optimized and incremental, an edit to `zz-ui` once left an rlib that
failed to link with undefined `anon.*.llvm.*` symbols.

A mounted window more than half a scroll height out of view skips its frames until it scrolls
back, so a long story costs about what is on screen. See [ala web windows](/references/ala-web-agents.md).

# Run it

```bash
just storybook run          # build, serve on http://127.0.0.1:8097, rebuild on save
just storybook build        # build into clients/storybook/dist; --release for an optimized build
just storybook serve        # serve the last build
just storybook penna        # build penna's own page, serve it on http://127.0.0.1:8098 (PORT overrides)
just storybook penna-build  # build it into libs/penna/storybook/dist; --release for an optimized build
```

`libs/penna/storybook/build.sh [--manifest <Cargo.toml>] [--release] [--out <dir>] [-- <cargo args>]`
builds a storybook page: penna's own by default, in the penna repo too, or an app's with
`--manifest`. `scripts/build-storybook.sh` is zz's call of it: `--manifest
clients/storybook/Cargo.toml --out clients/storybook/dist`, `-- --locked`, and `target/storybook` as
the target directory. The script runs Cargo in the manifest's folder, so the Cargo config and
toolchain file there apply wherever it is called from (`WEB_TOOLCHAIN` overrides the toolchain). It
reads the crate with `cargo metadata`, builds its library as a wasm32 cdylib, turns on
the crate's `page` and `syntax-highlighting` features when it has them (`WEB_SYNTAX_HIGHLIGHTING=0`
skips the second, which needs llvm-ar), wants the wasm-bindgen CLI at the version the lockfile
resolves, and writes into `dist` next to the manifest unless `--out` says otherwise. The just
recipe for penna's page passes the `libs/ala` patches after `--` and shares `target/storybook` with
zz's page. penna's CI builds the page with `--release` on every run and keeps it as the
`storybook-page` artifact, with the HTML kit and its tokens under `kit/`.

The URL holds the story, the section and the knobs: `#/agent/composer?theme=dark&radius=4`.
Knobs: `theme` (the mode: `system`, `light`, `dark`), `interface-theme` (`default` or
`macos`: the built-in the knobs start from, materials and roles both, picked under Start from), the theme's `name`,
the materials (`radius`, `outline`, `row-inset`, `primary`, `highlights` (`lift` or `flat`), `surfaces` (`darker` or `brighter`, the `bright-surfaces` material), `font`, `glass` with its sliders, the
contact shadow as `contact-y`, `contact-blur`, `contact-spread`, `contact-strength` and the float
shadow as `float-y`, `float-blur`, `float-spread`, `float-strength`), `roles` (the theme file's
`roles` object as JSON, written only while it differs from the start theme's), each mode's colors
(`light-accent`, `dark-on-accent`, `dark-shadow-color`, ...; a `#rrggbb` shadow color keeps the
theme's alpha, or the palette's when the theme sets none), `contrast`, the tint (`tint-hue`, Oklch
degrees with 260 as the default, and `tint`, 0 to 1, as `chrome-tint-hue` and `chrome-tint` set it
in the app), `zoom`, zz's pane knobs
(`pane-gaps`, `pane-margin`, `pip-opacity`,
`pane-inactive-opacity`, `pane-glow`, mirroring Settings → Panes, where pane corners follow the
theme's radius and the pane edge its glass, or its outline in a theme without glass; `pane-gaps` drives the Theme
creator and makes the glass pane frame own the pane fill, as in the app, while other stories show
gaps on and off side by side, so look at gapped frames with `pane-gaps=1`), `motion` (`motion=0` turns
animations off and freezes spinners). Two more keys only steer the page: `step` (`colors`,
`materials`, `roles`, `export`, `app`) opens a step of the knob panel, and `inspect=<role id>` picks a role
on the Roles step. The tint and pane knobs are app config, not theme, so Copy JSON leaves them out;
they stay in the URL. The Settings story's Appearance pickers section (`#/settings/appearance`)
draws the Interface page's Theme rows with the builders the app uses: Mode as a segmented control,
the Theme picker (Default, macOS, Yaru and a saved theme) with its more menu, the accent swatches with a custom picker (both
rounding with the corner radius) and the tint sliders (which follow the tint knobs), then
the Advanced heading and its Corner radius and Contrast sliders.

The Theme creator lives in `penna-storybook` (`penna_storybook::creator`), so every page built on
the harness has it: the knob panel, the role tree, export and import come from there, and the
catalog's `plates` are its regions, each its own window, so a hover or a spinner redraws only its
region. penna's own page has five plates built from penna alone (`creator::WINDOW`, `CONTROLS`,
`FLOATING`, `CONTENT`, `PHONE`): an app window with the sidebar, title bar and the appearance
rows, a form with switches, buttons and the settings card, menus with a picker, toasts, a dialog
and which-key, markdown and an inline alert, and the phone chrome from `penna::compact` and
`penna::touch`. zz's creator is every surface of zz in eleven plates (Workspace, Flat sidebar,
Lens, Panes, PiP, Start, Controls, Floating, Content, Phone, Drawers); its Controls, Floating and
Phone plates reuse the creator's parts (`tiles`, `card`, `heading`, `labeled`, `over_backdrop`,
`phone_frame`, `Preferences`, `Toasts`, `buttons`, `dialog`). The Lens region has
the plan-approval card with Show terminal and the welcome before a first prompt. The bar links (on
windows wider than 1240px) jump between regions, and `theme-creator/<region>` in the address opens
at one.
zz's Phone region is phone-width pages from penna's `compact` builders (the floating pill, the HUD,
the bento grid and `bento_add_tile`, which the Hosts page titles Add host) and zz-ui's `compact`
tiles (`host_tile`, `window_tile`, `overview_session_heading`, which the thin client draws with
too): a terminal page with the
HUD, the key row with the prefix menu open (`PopoverKey::open`) and the arrow pad's bubble
(`arrow_pad_bubble`), the bindings list, the sessions sheet, a host's windows and the Hosts page.
The creator's knob panel is four steps and an App tab:
Colors (the mode with Contrast, Tint hue and Tint, palettes, font), Materials
(shape, glass, both shadows),
Roles and Export (the theme file with Copy and Import), then App (the catalog's knobs, in zz the
pane settings, and the preview backdrop and animations, none of which are in the theme). The Roles step lists every kind with its groups and
roles indented under it (`penna::roles::members`, which lists the roles the app registered too), with the props each role's knobs allow
(`SurfaceRole::knobs`) as segmented controls. A prop left on inherit shows the value it inherits
and from which kind or group, built in or from this theme. Setting a prop on a role
with nothing under it writes only what differs from that into the theme's `roles` patch; on a
kind or a group it always writes, since its patch beats the built-in look of everything under it
(Floating set to glass makes the drop zones glass). Picking a role hides the regions that do not draw it, says which ones do above the
showcase (Show all clears the pick), and flashes a magenta ring just inside the edge of each of
its surfaces for about a second; picking a kind or group does the same for every role in it.
Hovering the list does nothing. Each section host adds the role ids and kind names each of its
frames resolves to that section's set (`penna::foundation::note_drawn_roles` and `drawn_roles`, read back
through the page's `drawn_roles` export), skipping frames drawn at zero size while the section is hidden, and the page
filters on those sets, so nothing lists a region's roles by hand. A kind or group only counts, and
only rings, where its patch still reaches the role (`Theme::group_reaches`): a role that sets all
six props itself, or whose groups in between do, is left out. `window` is the one role nothing draws
through yet, so picking it keeps every region. Roles also go to `localStorage`, keyed by the
start theme, so they survive a reload when the address drops them for length.

# Add an app's roles and plates to the creator

The creator edits every role in `penna::roles::all()`: the kit's, penna's chrome roles and the ones
the app registered. To tune an app's own roles with it:

1. Register the roles in the catalog's `register` function, the one the app calls at startup,
   before `penna::init`: `penna::register(ROLES, RETIRED, LAYERS)` takes the roles, the ids that
   are gone and the app's theme layers (`&[]` for none). A role made with
   `SurfaceRole::under(&PARENT, ..)` shows in the Roles step under that parent, whether the parent
   is a kit role or one of the app's; `SurfaceRole::new(id, kind, ..)` hangs under its kind. zz's
   is `zz_ui::register`.
2. Add a plate for them: a `Section` whose `build` paints the app's real UI with those roles, listed
   in the catalog's `plates`. List penna's plates beside it (`penna_storybook::creator::WINDOW` and
   the rest, or all of `creator::PLATES`) to keep the kit's surfaces in view. The section host notes
   the roles each plate draws as it paints, and picking a role in the creator shows only the plates
   that drew it, so nothing lists a plate's roles by hand. A role no plate draws reads "not in the
   showcase".
3. Export (Copy JSON) writes the theme's `roles` patch for every registered role, and Import takes
   a theme that names them: a retired id is dropped and an unknown id is refused with "unknown role
   `id`", and "did you mean" when one is close, the same as every theme file.

`libs/penna/storybook/tests/creator.rs` plays such an app: it registers `board`, `board.screen`
(under `board`) and `board.chip` (under `badge`), a retired `board.shelf` and a Board plate beside
penna's Controls, then checks the creator comes first with those plates, the roles JSON lists the
new roles under their parents, an edit to `board.screen` sent the way the page sends it lands in
the exported theme and parses back with `penna::interface_theme`, import drops the retired id and
names the unknown one, and the Board plate, drawn on ala's test platform, reports the three roles.

# Add a story

penna's stories live in `libs/penna/storybook/src/foundation/` and `src/kit/` and use penna only;
zz's live in `clients/storybook/src/stories/<area>/` (agent, workspace, console), each area
exporting `STORIES`, listed in `CATALOG`, and zz's creator plates in
`clients/storybook/src/stories/theme_creator.rs`. `clients/storybook/README.md` has the
authoring rules; the short version:

- One section per component piece, its states laid out with `states()`.
- Fixtures from constants only. No daemon, network, filesystem or `std::time::Instant`.
- Sections holding entities (`InputState`, `SelectState`, `AgentTimelineStore`) build a view once
  in `build`; stateless ones use `stateless(fn, cx)`.
- Open dialogs deferred: `Root` does not exist yet while `build` runs.

# Check a change

Edit the component in `crates/zz-ui` or `libs/penna`, let `just storybook run` rebuild,
reload, and walk the knobs: light and dark, both built-in themes, radius 0 and 25 (where every
button turns pill), outline 0,
both shadow strengths at 0, row inset 0, contrast extremes, a full tint, zoom. The sections re-render live as
knobs move.

The storybooks are their own Cargo workspaces, so the root clippy run skips them. `just storybook
lint` runs clippy on zz's for the host and for wasm under the root `clippy.toml`, and its host tests
run with `cargo test --manifest-path clients/storybook/Cargo.toml`. `just libs check penna` checks
penna's formatting, lints it for the host (all features) and for wasm (with `page`) and runs its
tests, as penna's own CI does, under `libs/penna/storybook/clippy.toml`, which carries the root's
surface-role rules. Both ban
hand-mixed colors and hand-drawn borders and shadows: use the kit's paints and surface roles, and
put a scoped `#[allow(clippy::disallowed_methods, reason = ..)]` only on a story that
shows a raw transform on purpose (the Colors story), on the theme JSON the HTML knob panel paints
itself with, or on `story::Framed::framed`, the edge of a frame that stands in for a window.

# For agents

- `scripts/storybook-shot.mjs <story>[/<section>][?knobs] out.png [--url ..]` (also
  `just storybook shot`) opens the story in headless Chrome over CDP, waits for `ala.idle()` and
  saves a PNG clipped to the section, kept inside the page where a section touches its edge (the
  theme creator has no nav column). Pass `--url http://127.0.0.1:8098/` for penna's page. Its JSON output lists wasm panics and console errors under
  `problems`. The knob panel keeps its own scroll, which the capture does not, so pass a taller
  `--height` to see a step's lower half, as in `theme-creator?step=roles&inspect=button`, which
  shows only the regions that draw buttons. Only a click on a role or a step tab flashes the ring;
  page loads and the `storybook.*` calls do not, so shots never show it.
- In a browser tab: `storybook.show(id, section)`, `storybook.setKnobs({ theme: "dark" })`,
  `storybook.theme()` (the theme file the knobs describe), `storybook.importTheme(json)`,
  `storybook.step("roles")`, `storybook.roles()` (the role tree with inherited and resolved styles),
  `storybook.setRole("chrome.button", "fill", "glass")` (`"inherit"` clears a prop),
  `storybook.inspect("button")`, and
  the `ala` API from [ala web windows for agents](/references/ala-web-agents.md) for the live
  UI (`ala.find`, `ala.click`, `ala.capture`).

# Traps

- Headless Chrome screenshots must use `--force-device-scale-factor`. CDP's
  `Emulation.setDeviceMetricsOverride` changes `devicePixelRatio` without changing what the
  ResizeObserver device-pixel box reports, so windows lay out at half width and render at twice
  the size.
- `--headless --screenshot --virtual-time-budget` captures before WebGPU and wasm finish booting.
- Agent shells may resolve `bash` to macOS's 3.2, where `set -u` scripts with empty arrays fail;
  run the build scripts with Homebrew's bash.

---
type: Playbook
title: Working with the zz storybook
description: Run the zz-ui storybook, add a story, change a look and check it in every theme, and drive or screenshot it from an agent.
resource: clients/storybook/src/lib.rs
tags:
- storybook
- zz-ui
- penna
- wasm
- agents
timestamp: 2026-10-15T00:00:00Z
---

# What it is

`clients/storybook` shows zz's own UI, not a generic widget catalog: every piece of `zz-ui` and
`penna` in its states, built from fixture data with the same functions the app calls. The page
is plain HTML (navigation, prose, knobs). Each section of the open story is its own ala window,
mounted into the section's element with `WindowOptions::mount` and sized to its content, so a
section looks exactly like that piece does in zz.

It is its own wasm Cargo workspace, like `clients/web`, with its own lockfile. Its dev profile
optimizes dependencies the way the root workspace does (`[profile.dev.package]`): third-party
crates and ala optimized, the zz crates it edits (`zz-ui`, `penna`, `zz-client`,
`zz-protocol`, `zz-terminal`) not. Without it, a debug build drew one Theme creator frame in about
100 ms. Keep those zz crates at `opt-level = 0`: optimized and incremental, an edit to `zz-ui` once
left an rlib that failed to link with undefined `anon.*.llvm.*` symbols.

A mounted window more than half a scroll height out of view skips its frames until it scrolls
back, so a long story costs about what is on screen. See [ala web windows](/references/ala-web-agents.md).

# Run it

```bash
just storybook run      # build, serve on http://127.0.0.1:8097, rebuild on save
just storybook build    # build into clients/storybook/dist; --release for an optimized build
just storybook serve    # serve the last build
```

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
in the app), `zoom`, the pane knobs
(`pane-gaps`, `pane-margin`, `pane-opacity`,
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

The Theme creator story is every surface of zz in eleven regions (Workspace, Flat sidebar, Lens,
Panes, PiP, Start, Controls, Floating, Content, Phone, Drawers), each its own window, so a hover or a
spinner redraws only its region. The Lens region has the plan-approval card with Show terminal and
the welcome before a first prompt. The bar links (on windows wider than 1240px) jump between them, and
`theme-creator/<region>` in the address opens at one.
The Phone region is phone-width pages from penna's `compact` builders (the floating pill, the HUD,
the bento grid and `bento_add_tile`, which the Hosts page titles Add host) and zz-ui's `compact`
tiles (`host_tile`, `window_tile`, `overview_session_heading`, which the thin client draws with
too): a terminal page with the
HUD, the key row with the prefix menu open (`PopoverKey::open`) and the arrow pad's bubble
(`arrow_pad_bubble`), the bindings list, the sessions sheet, a host's windows and the Hosts page.
Its knob panel is four steps and an App tab:
Colors (the mode with Contrast, Tint hue and Tint, palettes, font), Materials
(shape, glass, both shadows),
Roles and Export (the theme file with Copy and Import), then App (the pane settings and the
preview backdrop and animations, none of which are in the theme). The Roles step lists every kind with its groups and
roles indented under it (`zz_ui::roles::members`), with the props each role's knobs allow
(`SurfaceRole::knobs`) as segmented controls. A prop left on inherit shows the value it inherits
and from which kind or group, built in or from this theme. Setting a prop on a role
with nothing under it writes only what differs from that into the theme's `roles` patch; on a
kind or a group it always writes, since its patch beats the built-in look of everything under it
(Floating set to glass makes the drop zones glass). Picking a role hides the regions that do not draw it, says which ones do above the
showcase (Show all clears the pick), and flashes a magenta ring just inside the edge of each of
its surfaces for about a second; picking a kind or group does the same for every role in it.
Hovering the list does nothing. Each section host adds the role ids and kind names each of its
frames resolves to that section's set (`zz_ui::note_drawn_roles`, read back by
`zz.drawn_roles()`), skipping frames drawn at zero size while the section is hidden, and the page
filters on those sets, so nothing lists a region's roles by hand. A kind or group only counts, and
only rings, where its patch still reaches the role (`Theme::group_reaches`): a role that sets all
six props itself, or whose groups in between do, is left out. `window` is the one role nothing draws
through yet, so picking it keeps every region. Roles also go to `localStorage`, keyed by the
start theme, so they survive a reload when the address drops them for length.

# Add a story

Stories live in `clients/storybook/src/stories/<area>/`, one directory per area (foundation,
agent, workspace, console, kit), each exporting `STORIES`. `clients/storybook/README.md` has the
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

The storybook is its own Cargo workspace, so the root clippy run skips it. `just storybook lint`
runs clippy on it for the host and for wasm under the root `clippy.toml`, which bans hand-mixed
colors and hand-drawn borders and shadows here too: use the kit's paints and surface roles, and
put a scoped `#[allow(clippy::disallowed_methods, reason = ..)]` only on a story that
shows a raw transform on purpose (the Colors story), on the theme JSON the HTML knob panel paints
itself with, or on `story::Framed::framed`, the edge of a frame that stands in for a window.

# For agents

- `scripts/storybook-shot.mjs <story>[/<section>][?knobs] out.png [--url ..]` (also
  `just storybook shot`) opens the story in headless Chrome over CDP, waits for `ala.idle()` and
  saves a PNG clipped to the section. Its JSON output lists wasm panics and console errors under
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

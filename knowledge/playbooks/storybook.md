---
type: Playbook
title: Working with the zz storybook
description: Run the zz-ui storybook, add a story, change a look and check it in every theme, and drive or screenshot it from an agent.
resource: clients/storybook/src/lib.rs
tags:
- storybook
- zz-ui
- zz-gpui-kit
- wasm
- agents
timestamp: 2026-10-13T00:00:00Z
---

# What it is

`clients/storybook` shows zz's own UI, not a generic widget catalog: every piece of `zz-ui` and
`zz-gpui-kit` in its states, built from fixture data with the same functions the app calls. The page
is plain HTML (navigation, prose, knobs). Each section of the open story is its own zz-gpui window,
mounted into the section's element with `WindowOptions::mount` and sized to its content, so a
section looks exactly like that piece does in zz.

It is its own wasm Cargo workspace, like `clients/web`, with its own lockfile.

# Run it

```bash
just storybook run      # build, serve on http://127.0.0.1:8097, rebuild on save
just storybook build    # build into clients/storybook/dist; --release for an optimized build
just storybook serve    # serve the last build
```

The URL holds the story, the section and the knobs: `#/agent/composer?theme=dark&interface-theme=macos`.
Knobs: `theme` (the mode: `system`, `light`, `dark`), `interface-theme` (`default`, `macos`,
`linear`, `glass`), the look values (`radius`, `outline`, `row-inset`, `shadows` as `none`,
`surfaces` or `all`, `selection`, `primary`, `field`, `font`, and `glass` with its sliders; while
glass is on, a note under Outline says menus, popovers and dialogs draw the glass rim instead),
each mode's colors (`light-accent`, `dark-on-accent`, `dark-shadow-color`, ...; a `#rrggbb` shadow color keeps the
theme's alpha, or the palette's when the theme sets none), `contrast`, `zoom`, the pane knobs
(`pane-gaps`, `pane-margin`, `pane-corner-radius`, `pane-border-width`, `pane-opacity`,
`pane-inactive-opacity`, `pane-glow`, mirroring Settings → Panes; `pane-gaps` drives the Theme
creator and makes the glass pane frame own the pane fill, as in the app, while other stories show
gaps on and off side by side, so look at gapped frames with `pane-gaps=1`), `motion` (`motion=0` turns animations off and freezes
spinners). Pane knobs are app config, not theme, so Copy JSON leaves them out; they stay in the URL. The Theme creator story is every surface on one canvas;
its Copy JSON copies a theme file Settings can Import, and Import loads one back into the knobs.

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

Edit the component in `crates/zz-ui` or `crates/zz-gpui-kit`, let `just storybook run` rebuild,
reload, and walk the knobs: light and dark, each interface theme, radius 0 and full, outline 0,
each shadows choice, row inset 0, contrast extremes, zoom. The sections re-render live as knobs move.

The storybook is its own Cargo workspace, so the root clippy run skips it. `just storybook lint`
runs clippy on it for the host and for wasm under the root `clippy.toml`, which bans hand-mixed
colors and hand-drawn borders and shadows here too: use the kit's paints, surfaces, edges and
shadows, and put a scoped `#[allow(clippy::disallowed_methods, reason = ..)]` only on a story that
shows a raw transform on purpose (the Colors story), on the theme JSON the HTML knob panel paints
itself with, or on `story::Framed::framed`, the edge of a frame that stands in for a window.

# For agents

- `scripts/storybook-shot.mjs <story>[/<section>][?knobs] out.png [--url ..]` (also
  `just storybook shot`) opens the story in headless Chrome over CDP, waits for `zzGpui.idle()` and
  saves a PNG clipped to the section. Its JSON output lists wasm panics and console errors under
  `problems`.
- In a browser tab: `storybook.show(id, section)`, `storybook.setKnobs({ theme: "dark" })`,
  `storybook.theme()` (the theme file the knobs describe), `storybook.importTheme(json)`, and
  the `zzGpui` API from [zz-gpui web windows for agents](/references/zz-gpui-web-agents.md) for the live
  UI (`zzGpui.find`, `zzGpui.click`, `zzGpui.capture`).

# Traps

- Headless Chrome screenshots must use `--force-device-scale-factor`. CDP's
  `Emulation.setDeviceMetricsOverride` changes `devicePixelRatio` without changing what the
  ResizeObserver device-pixel box reports, so windows lay out at half width and render at twice
  the size.
- `--headless --screenshot --virtual-time-budget` captures before WebGPU and wasm finish booting.
- Agent shells may resolve `bash` to macOS's 3.2, where `set -u` scripts with empty arrays fail;
  run the build scripts with Homebrew's bash.

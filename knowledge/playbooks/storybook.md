---
type: Playbook
title: Working with the zz storybook
description: Build and inspect zz and penna stories, add sections and theme creator plates, and capture reproducible screenshots.
resource: scripts/just/storybook.just
tags: [storybook, zz-ui, penna, wasm, agents]
timestamp: 2026-11-24T00:00:00Z
---

## Run it

| Page | Build and serve | Build only | Contents |
| --- | --- | --- | --- |
| zz | `just storybook run` on port 8097, rebuilds on save | `just storybook build [--release]` into `clients/storybook/dist` | Agent, workspace and console stories from `clients/storybook/src/lib.rs` (`CATALOG`) |
| penna | `just storybook penna` on port 8098, `PORT` overrides | `just storybook penna-build [--release]` into `libs/penna/storybook/dist` | Theme creator, Foundation and Kit stories from `penna_storybook::PENNA` |

Reload after a rebuild. `just storybook serve` serves zz's existing build on 8097.
`just storybook macos` opens the native AppKit reference and a zz example on the same
backdrop on a Mac with the macOS 27 SDK.

Both pages use `libs/penna/storybook/build.sh`. For another app pass
`--manifest PATH [--release] [--out DIR] [-- CARGO_ARGS]`. The script reads the
manifest's features and lockfile, checks the matching wasm-bindgen CLI and builds the wasm32
library. `WEB_TOOLCHAIN` selects another toolchain. Syntax highlighting needs `llvm-ar`,
or set `WEB_SYNTAX_HIGHLIGHTING=0`. In this checkout the penna recipe patches in `libs/ala`
and restores its standalone lockfiles. The watcher needs `cargo-watch`.

## Add a story

Use one section per component piece. Show its states together with `states()`. The page
mounts each section in a separate ala window. Give a focused control or open menu a section
of its own. Use fixtures made from the same types the application consumes.

- Put zz-specific data, layout and behavior in `clients/storybook/src/stories/<area>/`.
  Register the story in the area's `STORIES`. Add kit components or new kit states in
  `libs/penna/storybook/src/foundation/` or `src/kit/` instead.
- Use a lowercase, hyphenated story or section ID and unique element IDs per section.
  Create stateful entities once in `build` and use `stateless` for sections that can
  redraw from fixtures. Defer dialogs until the window's `Root` exists.
- For chat use a `Script` in `stories/agent/fixtures.rs` and the existing `Thread` view.
  For floating sections add the story or story/section to the catalog's `floating` list
  to draw the backdrop. See `clients/storybook/README.md` for a complete section example.

## Add an app's plates to the creator

A nonempty `Catalog::plates` puts the theme creator first. zz supplies an empty slice.
To add a plate, build a `Section` with the app's real components and list it alongside
the desired `penna_storybook::creator` plates. The section host records the surface roles
it actually draws so `inspect` can filter the examples. `libs/penna/storybook/tests/creator.rs`
checks ordering, drawn roles and the export/import round trip with an example app plate.

The page exposes **Look**, **Person**, **Export** and **App**. Export copies the theme file.
App controls such as pane gaps and PiP progress stay in the preview. Use the URL to share
them, for example `#/agent/composer?theme=dark&radius=12`. `libs/penna/storybook/web/main.js`
defines the accepted steps and browser API.

## Check a change

Try light and dark, each starting theme, radius and contrast extremes, reduced transparency
and zoom. Use `pane-gaps=1` for a framed zz pane, or `pip-hold=1&pip-progress=0.4` to
inspect a PiP flight. The separate Cargo workspaces have their own checks:

```sh
just storybook lint
cargo test --manifest-path clients/storybook/Cargo.toml
just libs check penna
```

## For agents

With the corresponding page already running:

```sh
just storybook shot 'agent/composer?theme=dark' /tmp/composer.png
just storybook shot 'theme-creator/controls?theme=light' /tmp/controls.png --url http://127.0.0.1:8098/
```

The capture waits for ala to settle. A story/section route clips the PNG to that section.
The JSON result lists console errors and wasm panics under `problems`. Use `--width`,
`--height`, `--scale`, `--hover x,y`, `--timeout` or `CHROME` to reproduce a viewport
or control state. The script sets Chrome's device scale at launch so canvas pixels and
section bounds agree. A plain headless screenshot can run before WebGPU and WASM finish.

In the page console use `storybook.stories()`, `await storybook.show("agent", "composer")`,
`await storybook.setKnobs({ theme: "dark" })`, `storybook.theme()`,
`storybook.importTheme(json)`, `storybook.step("export")` or `storybook.inspect("button")`.
For live controls use [ala's accessibility and capture API](/references/ala-web-agents.md).

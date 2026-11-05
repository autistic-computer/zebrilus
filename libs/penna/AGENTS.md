# AGENTS.md

Read `~/dev/autistic-computer/.agents/AGENTS.md` first: the rules every autistic-computer repo
shares. This file adds what is specific to penna.

@~/dev/autistic-computer/.agents/AGENTS.md

penna is the widget kit on ala, a maintained fork of gpui-component. Read `UPSTREAM.md` before changing ported code or pulling an
upstream fix.

penna is the only place UI chrome is defined: tokens, surface roles, theme patches and components.
Read `knowledge/designs/foundations.md` before any UI work: it is the spec for every value, name
and rule (space, type, sizes, color, layout) and the order of work bringing penna onto it. Read
`knowledge/designs/kit.md` before adding a component or a role. Two rules from it:

- A component is in the kit when the storybook shows every state of it and it has been looked at
  on each theme in light and dark.
- Break the API freely. The commit that pulls penna into an app fixes that app; no deprecated paths.

## Source tree

`src/lib.rs` is the only file directly in `src/`. The three tiers are:

- `foundation/`: shared theme, tokens, roles, motion, CSS token generation, traits,
  highlighting and overlay hosting. Shared support lives here; support used by one widget
  lives with that widget.
- `components/<name>/`: one widget primitive. Its folder holds `mod.rs`, any Rust
  submodules, and its DOM implementation as `<name>.css` and `<name>.js` when present.
- `blocks/<name>/`: a piece of UI that owns the layout and state of several components.
  When a piece owns a layout of other penna components, put it here. Appearance is a block.

Components: badge, bar, button, card, check, chip, field, icon, input, kbd, label, list, menu,
panel, plate (crate-private), popover, row, scroll, segmented, select, separator, slider,
spinner, switch, tab, text, tile, tooltip, tree, well.

Blocks: appearance, code_editor (editor feature), color_picker, compact, dialog, dock,
docs, feedback, inspector, notice, notification, picker, settings, sheet, shell, sidebar,
steps, title_bar, touch, tray (tray feature), which_key.

Public module paths follow the tiers: `penna::components::button`,
`penna::blocks::settings`, `penna::blocks::appearance` and
`penna::foundation::interface_theme`. Common types remain re-exported at the crate root.
There are no aliases for the old module paths.

## Docs site

`docs/site.json` lists the page groups and `docs/pages/<slug>.md` holds each page.
`cargo run --example docs --features tree-sitter` writes the site to `target/docs` (or the
folder given as its argument): penna's markdown rendered into the `text` component's HTML,
code highlighted by penna's highlighter, inside the `docs` block's layout, with the tokens,
fonts and widget CSS and JS copied under `assets/`. Consecutive fences whose info string holds
`example` (```` ```rust example ````, ```` ```html example ````) become one `Example` with a tab per
fence. An html fence renders live above the tabs. `story=<story>/<section>` on a fence
(```` ```rust example story=buttons/variants ````) uses that storybook section instead: its parity
page markup is the live HTML and an HTML tab, and when `storybook/dist` is built the example gets an
HTML / Native switch that mounts the same section from the storybook wasm (its `mount(story,
section, selector)` export) and follows the page's theme. The choice is remembered across pages.

A ```` ```figure ```` fence is a picture with no code: `story=<story>/<section>` shows that section
with the HTML / Native switch, and otherwise the fence body is penna DOM markup rendered as is,
with `uses=label,input,...` adding those components' or blocks' CSS and JS to the page. Story
labels are hidden inside figures. The site's own CSS is `docs/site.css`.

## Design system

`knowledge/designs/foundations.md` is penna's design language and the source of truth: principles, the rules and scales, and a section per component (purpose, kinds, anatomy, sizes, states, behavior, content, placement, HTML, Status). Write a change there first, then draw it on its docs page, then make the code match. The docs site is how people see the spec: `docs/pages/principles.md`, the Foundations pages (`foundations.md`, `color.md`, `typography.md`, `spacing.md`, `sizes.md`) and a page per component, drawn with real penna components. Pages are terse: one or two short lines under each heading, then a picture. Pictures sit flush with the text, left-aligned, with no card around them, and show one or two cases built from penna markup (`.docs-show`, `.docs-cell` with a `.docs-note` caption); storybook sections (`story=`) are for pages that need the HTML / Native switch. Rules come as do and don't pairs (`.docs-dodont`: two panels with a mark and a caption). Values may appear as clean tables (`.docs-tokens`: name, value, a live example per row). Component pages add one code sample and an API table. A component page whose component doesn't match its section yet opens with a warning notice that names the gaps, and loses it once the Status reads matches. When a page and the spec disagree, the spec wins and the page gets fixed.

## Storybook

`storybook/` is penna's own page and the harness apps build theirs on: an app's storybook crate
depends on `penna-storybook`, hands `start` a `Catalog` of its stories, and builds and serves with
the two scripts here.

```bash
storybook/build.sh      # build penna's page into storybook/dist against ala from GitHub (--release for an optimized build)
storybook/serve.py      # serve storybook/dist on 127.0.0.1:8098 (PORT overrides)
storybook/build.sh --manifest <app storybook>/Cargo.toml --out <dir>   # build an app's catalog
storybook/serve.py <dir> <port>                                         # serve it
node storybook/parity.mjs buttons --theme macos --mode dark   # score the buttons DOM page against its canvas story (--all, --section s, --update, --check)
```

`#/<story>?solo=1` shows only the section canvases. Every DOM story page lives beside its Rust
story module: `storybook/src/kit/<story>.rs` pairs with
`storybook/src/kit/<story>/{index.html,page.css,page.js,parity.json,parity.baseline.json}`
(page CSS and JS are optional). Pages import widget CSS and JS from `src/components/` and
`src/blocks/`, and use `web/story.css`. Parity takes the Rust story module name, including
`buttons`, `inputs`, `selection`, `navigation`, `rows`, `scrollbars` and `markdown`;
`parity.json` names the story route when it differs. `--all` discovers every story folder
with `parity.json`; its baseline stays in that folder. Elements carry `data-id`
set to the canvas node's author id, which is the id the story passed to the component (`("tab", 3)` reads
`tab-3`). parity matches by id and falls back to `role:name` only for nodes with no id. Results land in
`target/parity/report.html`.

Only an element with an accessibility role reaches that tree. Plates, rows, list items, which-key and
scrollbars announce themselves from the id they already take (a scrollbar adds `<id>-vertical` and
`<id>-vertical-thumb`). A row given `hover_group` or `press_highlight(true)` is a fill under its
parent and stays out of the tree. `Kbd`, `Spinner`, `Separator`, `Steps`, `SettingsStack`, `Input`, `NumberInput`,
`ColorPicker` and a `SettingEntry` without `on_click` have no id of their own: give them `.id(..)` to make them a node
(a number input names its parts `<id>-decrement`, `<id>-field`, `<id>-increment`; a color picker `<id>-field`, `<id>-hue`,
`<id>-hex`, `<id>-clear`). Never give two siblings the same id: ala drops the second node and panics in a debug
build once accessibility is on.

A section scores 100 × √(geometry × pixels): pixels allow 3 levels per channel, so fills and hover tints count,
and the canvas's own ink is the denominator. Steps score only the target's box and need the DOM to change where
the canvas did. A section scores 0 if it holds an `img`, `canvas`, `iframe` or similar, or a `url()` image other
than the kit's icon masks. The DOM server exposes only `web/`, `storybook/fonts/`, and HTML, CSS and JS under
`storybook/src/kit/`, `src/components/` and `src/blocks/`. A page reaches the shared web
files through `../../../../web/` and widgets through `../../../../src/<tier>/<name>/`.

How parity drives the pages:

- The DOM page loads with `?theme=<t>&mode=<m>&bare&flat&motion=0` and `prefers-reduced-motion: reduce`
  emulated, so `kit.js` skips the glass lens (an `feImage`, still forbidden otherwise) and motion is off
  through the media query. Leave `--penna-glass` on; pages need no motion switch of their own.
- A step in `parity.json` is `{ "name", "section", "hover" | "press" | "click" | "key", "wait", "keep", "still" }`.
  `hover`, `press` and `click` take an id, `{ "id": "penna-number-decrement", "nth": 3 }` for the nth node
  sharing an id (from 1, in tree order on the canvas and document order in the DOM),
  `{ "at": [x, y] }` for a point in CSS px from the section's top-left corner on both sides, or
  `{ "selector": ".thumb" }` for the centre of a DOM element in the section, whose section-relative point
  the canvas gets too. Point and selector steps score the whole section; id steps score the node's box.
  An id or selector centre is rounded down to a whole CSS px, since ala's web build reads pointer
  positions in whole px: a tooltip on the edge of fitting otherwise flips on one side only.
- Each story loads fresh, so entity ids don't depend on what ran before. When a step
  leaves its canvas section changed (an open menu, a picked value, a focus ring), both pages reload
  before the next step and the next theme and mode, so no step has to undo itself or another.
  `"keep": true` runs a step on the previous step's state instead; key steps keep by default
  (`"keep": false` starts one fresh). `"still": true` expects a step to change nothing on either
  page (a click on a disabled control) and scores it 0 if either does. Steps that leave nothing
  behind keep the fast path: one load per story, knobs switched in place. The line after each
  story counts story loads.
- `--timeout` is per story (default 15 minutes); Ctrl-C and `kill` close Chrome and the servers.

`storybook/src/kit/buttons/` and `src/components/button/` are the templates to copy. What it learned about matching ala in the DOM:

- Wrap every text run in a span marked `data-penna-text`: ala ceils a text box's width to a whole px, and
  `kit.js` does the same for those spans on load, after fonts load and on a theme switch, or labels drift left
  along a row. The span must be a flex item or inline-block. Call `pennaKit.ceilText(root)` after adding or
  changing runs, or `pennaKit.ceilText(root, selector)` for runs a page marks its own way.
- `--penna-k` is the smoothing penna draws the role with (`Layer::smoothing`, as `superellipse(log2)`): the
  theme's, capped at 2.5 on rows, and 1 (a plain round) for a full radius. `penna.css` applies it; don't
  hardcode a `corner-shape`. A radius set on one button drops the full-radius round and draws the role's own
  smoothing, which every role exports as `--penna-style-k`: set `--penna-k: var(--penna-style-k)` next to it.
- An element with no role draws with the window's smoothing, as ala draws a quad with no smoothing of its own:
  give a rounded box with no role `corner-shape: superellipse(var(--penna-window-k))`. A circle or pill keeps
  the plain `round`, since ala draws a full radius round whatever the smoothing.
- Focus is the tokens' `data-state="focused"`: on a button, row, tab, list item, chip or tile it leaves the
  fill alone and sets `--penna-focus`. ala draws the ring 1.5px outside the border box with its own adaptive
  radius, which a CSS outline can't follow, so each widget's CSS swaps `penna.css`'s outline for an `::after`
  ring that `kit.js` sizes on every `[data-penna]` element (`--penna-ring-inset`, `--penna-ring-r`;
  `pennaKit.focusRing(element)` for one without a role). `--penna-focus` doesn't inherit, so the ring reads it
  through `--penna-ring` set on the element. Link and text buttons have no role, so their ring takes the root
  `--penna-focus-neutral`. Only fields change their
  fill or edge on focus, and only their rule also follows `:focus-within`.
- `kit.js` and a page's own script are classic scripts sharing one global scope, so a top-level function
  named like one of kit.js's (`settle`, `round`) replaces it and kit.js stops rounding corners.
- `story.css` takes `data-columns="3"` for a grid that snaps its tracks to device pixels like ala does.
- The storybook host ceils each section's height to a whole px; `web/story.js` does the same for every
  `section[data-section]` (`pennaStory.fit()` reruns it).
- Root tokens a page reaches for instead of mixing its own: `--penna-fg-muted` (the foreground's muted ink, which
  `.story-label` and `states()` labels draw in; `--penna-muted` is the ghost paint's, lighter in dark), `--penna-fg-faint` (the foreground's faint ink, for disabled link and text),
  `--penna-syntax-{keyword,type,constructor,function,comment,string,number,boolean,property,constant,title,literal}`
  (the highlighter's palette for the mode) and `--penna-glass-backdrop`, already in the shader's order: contrast,
  then brightness added, not multiplied.
- ala snaps a glyph's baseline to a device pixel rounding halves toward zero (`round_half_toward_zero`), and
  `text.js` places markdown baselines the same way. Copy that rounding wherever the DOM computes a baseline:
  rounding halves up puts every line that lands on a half device pixel one pixel low.
- What is left is text raster: ala snaps glyph baselines and dilates light text, which the DOM can't copy exactly.

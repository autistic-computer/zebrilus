# AGENTS.md

Read `~/dev/autistic-computer/.agents/AGENTS.md` first: the rules every autistic-computer repo
shares. This file adds what is specific to penna.

@~/dev/autistic-computer/.agents/AGENTS.md

penna is the widget kit on ala, a maintained fork of gpui-component. Read `UPSTREAM.md` before changing ported code or pulling an
upstream fix.

penna is the only place UI chrome is defined: tokens, surface roles, theme patches and components.
Read `knowledge/designs/kit.md` before adding a component or a role. Two rules from it:

- A component is in the kit when the storybook shows every state of it and it has been looked at
  on each theme in light and dark.
- Break the API freely. The commit that pulls penna into an app fixes that app; no deprecated paths.

## Storybook

`storybook/` is penna's own page and the harness apps build theirs on: an app's storybook crate
depends on `penna-storybook`, hands `start` a `Catalog` of its stories, and builds and serves with
the two scripts here.

```bash
storybook/build.sh      # build penna's page into storybook/dist against ala from GitHub (--release for an optimized build)
storybook/serve.py      # serve storybook/dist on 127.0.0.1:8098 (PORT overrides)
storybook/build.sh --manifest <app storybook>/Cargo.toml --out <dir>   # build an app's catalog
storybook/serve.py <dir> <port>                                         # serve it
node storybook/parity.mjs button --theme macos --mode dark   # score src/button's DOM page against the canvas story (--all, --section s, --update, --check)
```

`#/<story>?solo=1` shows only the section canvases. A component's DOM version lives next to its Rust
(`src/<component>/{index.html, parity.json}`, laid out with `web/story.css`); its elements carry `data-id`
set to the canvas node's author id, or `role:name` when it has none. Results land in `target/parity/report.html`.

A section scores 100 × √(geometry × pixels): pixels allow 3 levels per channel, so fills and hover tints count,
and the canvas's own ink is the denominator. Steps score only the target's box and need the DOM to change where
the canvas did. A section scores 0 if it holds an `img`, `canvas`, `iframe` or similar, or a `url()` image other
than the kit's icon masks. The DOM page is served only `web/`, `storybook/fonts/` and its own folder.

`src/button` is the template to copy. What it learned about matching ala in the DOM:

- Wrap every text run in a span: ala ceils a text box's width to a whole px, so `button.js` does the same,
  or labels drift left along a row.
- ala draws every quad with corner smoothing 4 (`corner-shape: superellipse(2)`), whatever the role's
  `--penna-k` says, and a full radius (`--penna-r` of 9999px) as a plain round.
- A focus ring is drawn 1.5px outside the border box, with its own adaptive radius, and leaves the fill alone,
  so don't use `data-state="focused"` for it.
- `story.css` takes `data-columns="3"` for a grid that snaps its tracks to device pixels like ala does.
- What is left is text raster: ala snaps glyph baselines and dilates light text, which the DOM can't copy exactly.

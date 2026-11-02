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
```

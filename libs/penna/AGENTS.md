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

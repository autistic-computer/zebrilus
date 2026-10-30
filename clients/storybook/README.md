# zz storybook

Every piece of `zz-ui` and `penna` in its states, on one web page. The page is plain HTML:
navigation, prose and knobs. Each section of the story on screen is its own ala window, mounted
into the section's element and sized to its content.

The page, the story types, the theme knobs and penna's Foundation and Kit stories come from
`penna-storybook` in `libs/penna/storybook`, the theme creator included. This crate adds zz's
catalog in `src/lib.rs` (`CATALOG`: the creator's plates from `src/stories/theme_creator.rs`, the
agent, workspace and console stories, the pane and program color knobs, zz's floating sections,
`zz_ui::init` and `zz_ui::Assets`) and its `run` entry.

```bash
just storybook run      # build, serve on http://127.0.0.1:8097, rebuild on save
just storybook build    # build into clients/storybook/dist (add --release for an optimized build)
just storybook penna    # penna's own page: the creator with penna's plates, Foundation and Kit, on http://127.0.0.1:8098
```

URLs carry the story, the section and the knobs: `#/agent/composer?theme=dark&radius=12`.

## Writing a story

A story is a `Story` constant. zz's live in `src/stories/<area>/`, each area exporting `STORIES`,
listed in `CATALOG`; penna's live in `libs/penna/storybook/src/foundation` and `src/kit` and use
penna only.

```rust
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "buttons",
    name: "Buttons",
    group: "Kit",
    summary: "One sentence for the page header.",
    sections: &[Section {
        id: "variants",
        name: "Variants",
        summary: "One sentence under the section title.",
        build: |_, cx| stateless(variants, cx),
    }],
};

fn variants(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state("default", Button::new("a").label("Save"))
        .state("disabled", Button::new("b").label("Save").disabled(true))
        .into_any_element()
}
```

- One section per component piece. Each section shows that piece's states with `states()`:
  a labelled list, or a grid with `.columns(n)`. `row()` lays small items out side by side.
- `build` runs once when the story opens. A section that needs entities (an `InputState`, a
  `SelectState`, a `ListState`) builds a view with `cx.new(..)` and returns it as an `AnyView`;
  stateless ones use `stateless(fn, cx)`, which re-renders on every frame.
- Build fixtures from the same types the app uses. Never reach a daemon, the network or the
  filesystem: everything must render in a browser tab from constants.
- Ids must be lowercase with hyphens; `Catalog::problems`, checked by the tests, finds bad and
  duplicate ids and floating entries that name no section.
- A section that floats over the content (a menu, a dialog, a palette) goes in the catalog's
  `floating` list, so the backdrop knob draws under it.
- A theme creator plate is a `Section` in `PLATES` (`src/stories/theme_creator.rs`), which the
  catalog's `plates` lists. It paints real zz UI; the roles it draws are recorded as it paints, so
  picking a role in the creator finds its plates without a list kept by hand. Shared plate parts
  (`tiles`, `card`, `heading`, `over_backdrop`, `phone_frame`, `Preferences`, `Toasts`, `buttons`,
  `dialog`) come from `penna_storybook::creator`.
- Element ids inside one section must be unique: the same id twice shares keyed state.
- Use `web_time::Instant`, never `std::time::Instant`, which panics on wasm.
- One focused element exists per window. A state that needs focus or an open menu goes in its
  own section.
- Pane knobs come from `crate::pane()`, the rest from `crate::knobs()`.

## For agents

The page exposes `window.storybook`:

```js
storybook.stories();                       // the registry: stories, sections and mount ids
await storybook.show("agent", "composer"); // open a story, scroll to a section
await storybook.setKnobs({ theme: "dark", radius: 12 });
```

The live UI is reachable through `globalThis.ala` (see
`knowledge/references/ala-web-agents.md`): every section window's accessibility tree is
mirrored into the page, so `ala.find({ role: "button", name: "Send" })`, `ala.click(ref)` and
`ala.capture(windowId)` work, and so do the browser's own accessibility snapshots.

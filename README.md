# kit

Widgets for [zz-gpui](https://github.com/autistic-computer/gpui): theme, primitives, widgets and an icon set. It started as a fork of [gpui-component](https://github.com/longbridge/gpui-component) and no longer depends on it. `UPSTREAM.md` records what was ported and how it changed.

## Use it

```toml
[dependencies]
zz-gpui = { git = "https://github.com/autistic-computer/gpui", branch = "main" }
zz-gpui-kit = { git = "https://github.com/autistic-computer/kit", branch = "main" }
```

Features: `editor` turns on the code editor widgets, `tree-sitter` adds syntax highlighting to markdown and the editor.

To build against a local gpui checkout instead of the one on GitHub:

```toml
[patch."https://github.com/autistic-computer/gpui"]
zz-gpui = { path = "../gpui/crates/zz-gpui" }
```

## Where changes come from

[zz](https://github.com/demfabris/zz) carries this repo as a git subtree in `libs/kit`, edits it in place and sends its commits here with `git subtree push`. Commits made here directly go back into zz with `git subtree pull`.

## License

Apache-2.0, see `LICENSE-APACHE`. The icons are adapted from the Solar Icons Set (CC BY 4.0, `src/icon/LICENSE-SOLAR`); the Chrome mark is from Tabler Icons (MIT, `assets/icons/LICENSE-TABLER`).

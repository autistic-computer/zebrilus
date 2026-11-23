# Select

Picks one value from a list that opens under it. For a few short options in view use a [segmented control](segmented.html); for on or off, a [switch](switch.html); to type a value, an [input](input.html); to run a command, a [button](button.html) with a caret.

## Anatomy

```figure uses=select
<div class="docs-show" data-gap="wide" style="align-items: flex-start; min-height: 224px">
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-value="0" data-open data-pinned style="width: 160px"></div></div>
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-placeholder="Language" style="width: 160px"></div></div>
</div>
```

1. **Trigger**: a field, as wide as its value unless given a width.
2. **Value**: cut with an ellipsis, or a faint placeholder when nothing is picked.
3. **Chevron**: muted, one rung smaller than the field from medium up.
4. **Menu**: opens `space-4` below the trigger aligned to its trailing edge (`Anchor::TopRight`), at least as wide, with the picked row checked; scrolls past `.menu_max_h()`, or shows an empty-list view (`.empty()`, 28 px inbox icon by default).

```rust
let language = cx.new(|cx| {
    SelectState::new(vec!["Rust", "Go", "Zig"], Some(0), window, cx)
});
Select::new(&language).placeholder("Language")
```

## Sizes

```figure uses=select
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="xsmall"></div></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="small"></div></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1"></div></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="large"></div></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="xlarge"></div></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="touch"></div></span>
  <span>touch-large</span><span><code>56</code></span><span><code>touch-body</code></span><span><div class="penna-select" data-options="Rust, Go, Zig" data-value="1" data-size="touch-large"></div></span>
</div>
```

Medium is the default and follows density. Same size as the controls beside it.

## States

```figure uses=select
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-value="0" style="width: 160px"></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-value="2" aria-disabled="true" style="width: 160px"></div><span class="docs-note">Off</span></div>
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-placeholder="Language" data-tone="danger" style="width: 160px"></div><span class="docs-note">Error</span></div>
</div>
```

- Hover: the fill steps up.
- Focused: the field's focused fill and ring from the keyboard.
- Open: the focused fill, with the menu under it and the picked row highlighted.
- Off: faint, leaves the keyboard path, never opens.
- Error: `.tone(Tone::Danger)` paints only the edge, like a field. Its [Field](field.html) says what is wrong.

## Keys

Enter, Space, Up or Down open it. Up and Down move, Enter picks, Tab closes an unscoped menu, and Escape closes and returns focus to the trigger.

## Search, scopes and phone

Past 12 options the menu carries a search field automatically (or force it with `.searchable(true)`), and `.scopes()` adds a full-width segmented control underneath to filter groups (Tab and Shift+Tab cycle scopes). Options implementing `SelectItem::font_family()` render in their own typeface. Under a coarse pointer, the list opens as a bottom [sheet](sheet.html) (up to 85% tall) of ruled touch rows with the picked option checked and a 44 px search field above the list.

```figure uses=select,input,segmented
<div style="width: 100%; min-height: 300px">
  <div class="penna-select" data-options="CommitMono, JetBrains Mono, Fira Code, Inter, SF Pro" data-value="0" data-searchable="true" data-search-placeholder="Filter fonts" data-scopes='[{"label":"All"},{"label":"Mono","keep":["CommitMono","JetBrains Mono","Fira Code"]},{"label":"Sans","keep":["Inter","SF Pro"]}]' data-open data-pinned style="width: min(100%, 220px)"></div>
</div>
```

## Options

Two options read better side by side. Keep the select for longer lists.

```figure uses=select,segmented
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="Layout"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Floating</span></span></div></div></div><figcaption>Show two options at once</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-select" data-options="Tiled, Floating" data-value="0"></div></div><figcaption>Don't hide two options in a menu</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Select::new(&state)` | `.penna-select` | Trigger and menu view |
| `SelectState::new(items, picked, ..)` | `data-options`, `data-value` | Options and initial selection |
| `.selected_value()`, `.set_selected_index(..)`, `.set_selected_value(..)` | `data-value` | Read or change selection on state |
| `.placeholder(text)` | `data-placeholder` | Shown with nothing picked |
| `.xsmall()` … `.touch_large()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Error edge |
| `.on(ground)` | `data-ground` on parent | Paint for the ground under it |
| `.w(width)`, `.menu_max_h(height)` | `style="width"`, `--penna-select-max` | Trigger width and menu scroll height |
| `.searchable(bool)`, `.search_placeholder(text)` | `data-searchable`, `data-search-placeholder` | Search field (needs `input` CSS/JS) |
| `.scopes(scopes)` | `data-scopes`, `data-scope` | Scoped groups (needs `segmented` CSS/JS) |
| `.empty(builder)`, `.title(text)` | | Custom empty view; touch sheet title |
| `SelectItem::font_family()` | | Renders font rows in their own typeface |
| `SelectEvent::Confirm` | `change`, `data-open`, `data-pinned` | A row was picked; open state |

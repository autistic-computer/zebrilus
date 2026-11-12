# Select

Picks one value from a list that opens under it. For a few short options in view use a segmented control.

## Anatomy

```figure uses=select
<div class="docs-show" data-gap="wide" style="align-items: flex-start; min-height: 196px">
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-value="0" data-open data-pinned style="width: 160px"></div></div>
  <div class="docs-cell"><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-placeholder="Language" style="width: 160px"></div></div>
</div>
```

1. **Trigger**: a field. As wide as its value unless given a width.
2. **Value**: cut with an ellipsis. A faint placeholder when nothing is picked.
3. **Chevron**: muted, a size smaller than the field from medium up.
4. **Menu**: opens under the trigger, at least as wide, the picked row checked.

```rust
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
- Open: the focused fill, with the menu under it.
- Off: faint, leaves the keyboard path, never opens.
- Error: the edge turns to danger, like a field. Its [Field](field.html) says what is wrong.

## Keys

Enter, Space, Up or Down open it. Up and Down move, Enter picks, Escape closes.

## Options

Two options read better side by side. Keep the select for longer lists.

```figure uses=select,segmented
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Floating</span></span></div></div></div><figcaption>Show two options at once</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-select" data-options="Tiled, Floating" data-value="0"></div></div><figcaption>Don't hide two options in a menu</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `SelectState::new(items, picked, ..)` | `data-options`, `data-value` | Options and the picked one |
| `.placeholder(text)` | `data-placeholder` | Shown with nothing picked |
| `.xsmall()` … `.touch()` | `data-size` | Size; medium by default |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Error edge |
| `.w(width)` | `style="width"` | Fixed width |
| `.menu_max_h(height)` | | Menu height |
| `.empty(builder)` | | Empty list |
| `.title(text)` | | Sheet title under touch |
| `SelectEvent::Confirm` | `change` | A row was picked |

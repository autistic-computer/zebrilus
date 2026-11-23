# Segmented control

Picks one of two to five short options, all in view, and applies the pick at once. For more use a [select](select.html); for on or off, a [switch](switch.html).

## Anatomy

```figure uses=segmented
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="2" role="radiogroup" aria-label="Appearance mode"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>System</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Light</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Dark</span></span></div></div><span class="docs-note">Track, pill, segments</span></div>
  <div class="docs-cell" style="width: min(100%, 240px)"><div class="penna-segmented" data-penna="field" data-width="full" data-value="0" role="radiogroup" aria-label="Scope"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>All</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Mono</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Sans</span></span></div></div><span class="docs-note">Full width</span></div>
</div>
```

1. **Track**: a field at the control's size with `space-2` inner inset.
2. **Pill**: concentric inside the track behind the picked segment; slides to a new pick over 220ms (instant under reduced motion).
3. **Segments**: each as wide as its label, or equal width with `.full_width()`.
4. **Labels**: the rung's text style, in the ink on the picked segment.

```rust
SegmentedControl::new("mode", ["System", "Light", "Dark"])
    .selected(picked)
    .on_select(pick)
```

## Sizes

```figure uses=segmented
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" data-size="xsmall" role="radiogroup" aria-label="xsmall view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" data-size="small" role="radiogroup" aria-label="small view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="medium view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" data-size="large" role="radiogroup" aria-label="large view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" data-size="xlarge" role="radiogroup" aria-label="xlarge view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><div class="penna-segmented" data-penna="field" data-value="0" data-size="touch" role="radiogroup" aria-label="touch view"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>List</span></span></div></div></span>
</div>
```

Medium is the default and follows density. Each segment's side inset is the rung's field inset plus 2: 8, 8, 10, 12, 14, 14. Match the other controls in its row.

## States

```figure uses=segmented
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="First picked"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">First picked</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup" aria-label="Second picked"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Second picked</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup" aria-label="Hovered segment"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false" data-state="hover"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup" aria-label="Pressed segment"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false" data-state="hover active"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" data-state="focused" role="radiogroup" aria-label="Focused control"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" aria-disabled="true" role="radiogroup" aria-label="Disabled control"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Disabled</span></div>
</div>
```

- Picked: the pill under it; always one. A click picks without focusing.
- Hover and pressed: a quiet fill behind the segment, as tall and round as the pill, never on the picked one.
- Focused: a ring outside the track and the field's focused fill, from the keyboard only (a tab stop only with `on_select`). Arrow keys step and stop at the ends; Home and End jump.
- Disabled: the field's disabled paint; the pill keeps its selected paint at half strength with no shadow, and it leaves the focus order.

## Labels

One or two words each, about the same length.

```figure uses=segmented
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup" aria-label="Short labels"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>System</span></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Light</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Dark</span></span></div></div></div><figcaption>Short, even labels</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="Long labels"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Follow the system</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Dark</span></span></div></div></div><figcaption>Don't write a sentence in a segment</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `SegmentedControl::new(id, labels)` | `.penna-segmented[data-penna="field"][role="radiogroup"]`, `.penna-segments`, `.penna-segment-pill`, `.penna-segment` | The options |
| `.selected(index)` | `data-value`, `pennaSegmented.set(el, i)` | The picked one |
| `.on_select(handler)` | `change` | Gets the picked index |
| `.xsmall()` … `.touch()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.full_width()` | `data-width="full"` | Equal-width segments filling the container |
| `.dots(indices)` | `.penna-segment-dot` after the label | An accent dot after those segments' labels |
| `.dimmed(indices)` | `data-dim` on `.penna-segment` | Those labels at half strength, still pickable |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.on(ground)` | `data-ground` on parent | Paints for the surface under it |
| | `pennaSegmented.wire(root)` | Wires dynamically inserted controls |

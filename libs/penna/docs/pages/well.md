# Well

A tinted recess that things are arranged on, like screens on a desk. For a box of content use a card.

## Anatomy

```figure uses=well,tile,label
<div class="penna-well" data-penna="well" style="padding: var(--penna-space-24)">
  <div class="penna-well-row">
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>HDMI-1</span></div>
  </div>
</div>
```

1. **Container**: a tint of the ink over its ground. No outline, no shadow, theme radius.
2. **Inset**: the caller's. The canvas story pads `space-24`.
3. **Tiles**: in a wrapping row `gap-group` 16 apart, or where the app arranges them.
4. **Ground**: tiles on it take `Well::ground(cx)`, so they rise a step from the recess.

```rust
Well::new()
    .p(Space::S24)
    .child(inline(Space::GAP_GROUP).children(screens))
```

## Content

Things to arrange or pick. Text and settings go in a card.

```figure uses=well,tile,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen" style="width: 96px; height: 60px"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div></div></div><figcaption>Screens to arrange</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-well penna-stack" data-penna="well" data-gap="tight" style="padding: var(--penna-space-16)"><span class="penna-type" data-type="body" data-penna-text>Share the clipboard</span><span class="penna-type" data-type="small" data-muted data-penna-text>Copy here, paste there.</span></div></div><figcaption>Don't use it as a card</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Well::new()` | `.penna-well`, `data-penna="well"` | The well |
| `inline(Space::GAP_GROUP).flex_wrap()` | `.penna-well-row` | Tiles in a row, 16 apart |
| `Tile::on(Well::ground(cx))` | a `.penna-tile` inside | A tile on the well's ground |
| `.on(Well::ground(cx))` on controls | `data-ground` | Controls paint on the well |

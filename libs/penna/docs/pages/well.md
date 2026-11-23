# Well

A tinted recess that things are arranged on, like screens on a desk. For a box of content use a [card](card.html); for a raised surface, a [panel](panel.html).

## Anatomy

```figure uses=well,tile,label
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell">
    <div class="penna-well" data-penna="well" style="padding: var(--penna-space-24)">
      <div class="penna-well-row">
        <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div>
        <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>HDMI-1</span></div>
      </div>
    </div>
    <span class="docs-note">Tiles arranged in a well</span>
  </div>
  <div class="docs-cell">
    <div class="penna-well penna-inline" data-penna="well" data-ground data-gap="related" style="padding: var(--penna-inset-card)">
      <button class="penna-button" data-penna="button" data-size="small" type="button"><span class="penna-label" data-penna-text>Identify</span></button>
      <span class="penna-type" data-type="small" data-muted data-penna-text>On the well's ground</span>
    </div>
    <span class="docs-note">Controls with data-ground</span>
  </div>
</div>
```

1. **Container**: a 4% tint of the ink over its ground, sitting at window level with no outline, no shadow, and the `radius` knob. Takes no id, focus or pointer state of its own.
2. **Inset**: the caller's. The canvas story pads `space-24`.
3. **Tiles**: in a wrapping row `gap-group` 16 apart, or placed where the app arranges them.
4. **Ground**: tiles and controls inside take `Well::ground(cx)` so they rise cleanly from the recess.

```rust
Well::new()
    .p(Space::S24)
    .child(inline(Space::GAP_GROUP).flex_wrap().children(screens))
```

## Content

Things to arrange or pick. Text and settings go in a [card](card.html).

```figure uses=well,tile,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen" style="width: 96px; height: 60px"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div></div></div><figcaption>Screens to arrange</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-well penna-stack" data-penna="well" data-gap="tight" style="padding: var(--penna-space-16)"><span class="penna-type" data-type="body" data-penna-text>Share the clipboard</span><span class="penna-type" data-type="small" data-muted data-penna-text>Copy here, paste there.</span></div></div><figcaption>Don't use it as a card</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Well::new()` | `.penna-well[data-penna="well"]` | The well |
| `Well::ground(cx)` | `data-ground` on `.penna-well` | Ground painted for children inside the well |
| `inline(Space::GAP_GROUP).flex_wrap()` | `.penna-well-row` | Wrapping row of tiles, 16 apart |
| `Tile::new(id).on(Well::ground(cx))` | `.penna-tile` inside `.penna-well` | Tile painted on the well's ground |
| `.on(Well::ground(cx))` on controls | `data-ground` | Controls and muted text paint on the well |

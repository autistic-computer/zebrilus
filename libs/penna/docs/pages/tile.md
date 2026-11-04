# Tile

A thumbnail you pick or drag, drawn to scale. For a box of content use a card, for a list a row.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>The picked ring is the accent, not the highlight. Tiles take no focus and don't lift while dragged. The HTML tile fixes its size and drags on its own.</span></div></div></div>
```

## Variants

```figure uses=well,tile,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div></div><span class="docs-note">Screen, on a well</span></div>
  <div class="docs-cell"><div class="penna-tile" data-penna="tile.card" data-variant="card"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>xps</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Linux · 1 screen</span></div><span class="docs-note">Card, in a dock</span></div>
</div>
```

- **Screen**: a screen drawn to scale on a well.
- **Card**: something you can place, waiting in a dock.
- **Drop**: where a dragged thing will land.
- **Veil**: the tint over panes while something is dragged.
- **Mini**: a pane inside a window's tile. Takes the caller's fill.

```rust
Tile::new("dp-1")
    .on(Well::ground(cx))
    .variant(TileVariant::Screen)
    .state(PaintState::Selected)
```

## States

```figure uses=well,tile,label
<div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)">
  <div class="penna-well-row">
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>Rest</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="selected"><span class="penna-type" data-type="caption" data-penna-text>Selected</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="disabled"><span class="penna-type" data-type="caption" data-penna-text>Away</span></div>
  </div>
</div>
```

- Selected: a 2px ring outside the edge.
- Off: half the fill, faint label, no shadow.
- Dragged: the app sets hover; a screen draws as at rest.

## Tone

A tone tints the label. Say what it means in words.

```figure uses=tile,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-tone="danger"><span class="penna-type" data-type="caption" data-penna-text>Can't go there</span></div></div><figcaption>Danger with a reason</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-tone="danger"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div><figcaption>Don't rely on the color</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Tile::new(id)` | `.penna-tile`, `data-penna="tile.card"`, `data-variant="card"` | Card variant |
| `.variant(TileVariant::Screen)` | `tile.screen`, `screen` | Also `Drop`, `Veil`, `Mini` |
| `.state(PaintState::Selected)` | `data-state="selected"` | Also `Hover`, `Focused`, `Disabled` |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Tints the label |
| `.on(ground)` | inside `.penna-well` | Paints it on a ground |
| `.edge(width)` | `data-edge` | Drop zone edge width |
| | `[data-tiles]`, `[data-pick]` | Drag and pick, HTML only |

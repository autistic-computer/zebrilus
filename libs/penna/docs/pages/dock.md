# Dock

A bottom tray anchored under a canvas or arrangement view, holding a titled row of [Tile](tile.html) cards and an optional trailing action.

## Tray and tile row

`Dock` draws a top `RuleLook::Bar` hairline with a transparent ground so it sits directly on the window surface. Place selectable or draggable `tile.card` items inside `.penna-dock-tiles`.

```figure uses=dock,label,tile
<div class="penna-dock" style="width: 100%; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); overflow: hidden; background: var(--penna-bg)">
  <div class="penna-dock-header" style="height: auto; min-height: 48px; padding-block: 8px">
    <div class="penna-dock-titles">
      <span class="penna-type" data-type="body" data-strong data-penna-text>Nearby</span>
      <span class="penna-type" data-type="small" data-muted>Drag onto the desk, or pick a side.</span>
    </div>
    <button class="penna-button" data-penna="button" data-size="small" type="button"><i data-icon="plus"></i><span class="penna-label" data-penna-text>By address…</span></button>
  </div>
  <div class="penna-dock-tiles" style="flex-wrap: wrap">
    <div class="penna-tile" data-penna="tile.card" data-variant="card" role="button" tabindex="0"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>xps</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Linux · 1 screen</span></div>
    <div class="penna-tile" data-penna="tile.card" data-variant="card" data-state="selected" role="button" tabindex="0"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>thinkpad</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Picked</span></div>
    <div class="penna-tile" data-penna="tile.card" data-variant="card" data-state="disabled" role="button" tabindex="-1"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>gaming-pc</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Needs 0.8</span></div>
  </div>
</div>
```

```rust
Dock::new("Nearby")
    .subtitle("Drag onto the desk, or pick a side.")
    .header_height(px(48.0))
    .action(Button::new("by-addr").icon(IconName::Plus).label("By address…").small())
    .child(
        h_flex()
            .px(Space::S16)
            .pb(Space::S16)
            .gap(Space::S8)
            .children(host_tiles),
    )
```

## API

| Rust (`penna::Dock`) | HTML (`uses=dock,label,tile`) | Role |
| --- | --- | --- |
| `Dock::new(title)` | `.penna-dock` | Top-ruled tray container |
| `.subtitle(...)` | `.penna-dock-titles > .penna-type[data-muted]` | Secondary prompt line |
| `.action(...)` | Trailing button in `.penna-dock-header` | Secondary header action |
| `.header_height(Pixels)` | `.penna-dock-header` (48 px default) | Explicit header bar height |
| `.child(...)` / `.children(...)` | `.penna-dock-tiles` | Row of `tile.card` items |

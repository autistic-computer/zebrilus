# Skeleton

Quiet loading placeholders that preserve layout geometry while data loads, avoiding sudden shifts when panes or lists hydrate.

## Shapes

Three primitives cover every surface: `line` for text runs, `block` for cards or thumbnails (following the theme's corner radius), and `circle` for avatars or status glyphs. Under `Motion::Off` or `prefers-reduced-motion`, the pulse stops cleanly.

```figure uses=skeleton,card
<div class="penna-card" data-penna="card" data-ground style="display: flex; flex-direction: column; gap: 12px; width: 100%; max-width: 340px; padding: 14px">
  <div style="display: flex; align-items: center; gap: 10px">
    <span class="penna-skeleton" data-shape="circle" style="width: 32px; height: 32px; flex: none"></span>
    <div style="display: flex; flex-direction: column; gap: 6px; flex: 1; min-width: 0">
      <span class="penna-skeleton" style="width: 58%"></span>
      <span class="penna-skeleton" style="width: 36%; height: 10px"></span>
    </div>
  </div>
  <span class="penna-skeleton" data-shape="block" style="height: 72px"></span>
  <span class="penna-skeleton" style="width: 84%"></span>
</div>
```

```rust
v_flex()
    .gap(Space::S8)
    .child(Skeleton::circle("avatar-sk", px(32.0)))
    .child(Skeleton::block("body-sk").height(px(72.0)))
    .child(Skeleton::line("caption-sk").width(relative(0.6)))
```

## API (`penna::Skeleton`)

| Rust | HTML (`uses=skeleton`) | Role |
| --- | --- | --- |
| `Skeleton::line(id)` | `.penna-skeleton` | Text-line placeholder (14 px) |
| `Skeleton::block(id)` | `.penna-skeleton[data-shape="block"]` | Card/panel placeholder using theme radius |
| `Skeleton::circle(id, size)` | `.penna-skeleton[data-shape="circle"]` | Circular avatar or icon placeholder |
| `.width(Length)` / `.height(Length)` | inline `width` / `height` | Match exact target layout box |

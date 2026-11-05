# Card

A box in the flow of content that holds one thing. To float over the view use a panel, for something to pick a tile.

## Looks

```figure uses=card,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-card" data-penna="card.bubble" role="group" style="max-width: 240px"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div><span class="docs-note">Bubble</span></div>
  <div class="docs-cell"><div class="penna-card penna-stack" data-penna="card.code" role="group" data-gap="tight"><span class="penna-type" data-type="caption" data-muted data-penna-text>rust</span><span class="penna-type" data-type="mono" data-penna-text>cargo test --all</span></div><span class="docs-note">Code</span></div>
  <div class="docs-cell"><div class="penna-card" data-penna="card.figure" role="figure"><span class="penna-type" data-type="small" data-penna-text>Name · Size · Kind</span></div><span class="docs-note">Figure</span></div>
  <div class="docs-cell"><div class="penna-card penna-inline" data-penna="card.image" role="figure" style="width: 72px; height: 72px; justify-content: center"><span class="penna-type" data-type="caption" data-penna-text>image</span></div><span class="docs-note">Image</span></div>
</div>
```

- **Bubble**: your messages in a chat.
- **Code**: code blocks and inline code.
- **Figure**: tables. No fill of its own, no outline.
- **Image**: image tiles in messages and the composer.

```rust
Card::new("snippet", CardLook::Code)
    .child(code)
```

## Anatomy

1. **Container**: a step above its ground, no outline, no shadow, the `radius` knob.
2. **Inset**: `inset-card` 12 all round, set by the card. A caller's padding replaces it.
3. **Ground**: what sits on it passes the card's ground, so muted text and badges follow its fill.

Cards in a column sit `gap-group` 16 apart.

## Ground

On a panel the card rises one step from the panel's fill, not the window's.

```figure uses=panel,card,label
<div class="penna-panel" data-penna="panel" data-ground role="group" style="width: 260px; padding: var(--penna-inset-card)"><div class="penna-card" data-penna="card.bubble" role="group"><span class="penna-type" data-type="body" data-penna-text>A step above the panel</span></div></div>
```

## Inset

Keep the card's own `inset-card`. Don't pick a padding by eye.

```figure uses=card,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="panel" data-ground role="group"><div class="penna-card" data-penna="card.bubble" role="group"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div></div><figcaption>inset-card 12</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="panel" data-ground role="group"><div class="penna-card" data-penna="card.bubble" role="group" style="padding: var(--penna-space-2) var(--penna-space-6)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div></div><figcaption>Don't squeeze it</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Card::new(id, CardLook::Bubble)` | `.penna-card`, `data-penna="card.bubble"`, `role="group"` | Bubble look |
| `CardLook::Code`, `Figure`, `Image` | `card.code`; `card.figure`, `card.image` with `role="figure"` | Other looks |
| `.p(space)` | `padding` | Replaces the inset |
| `.on(ground)` | `data-ground` on the container it sits in | Paints it a step above that ground |
| `.on(cx.theme().ground_of(look))` on children | `data-ground` on the card | Children paint on the card |

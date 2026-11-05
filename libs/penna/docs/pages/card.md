# Card

A box in the flow of content that holds one thing. To float over the view use a panel, for something to pick a tile.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>A card sets no inset, so callers pick their own. The HTML card can't take a ground and isn't announced as a figure.</span></div></div></div>
```

## Looks

```figure uses=card,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-card" data-penna="card.bubble" style="max-width: 240px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div><span class="docs-note">Bubble</span></div>
  <div class="docs-cell"><div class="penna-card penna-stack" data-penna="card.code" data-gap="tight" style="padding: var(--penna-inset-card)"><span class="penna-type" data-type="caption" data-muted data-penna-text>rust</span><span class="penna-type" data-type="mono" data-penna-text>cargo test --all</span></div><span class="docs-note">Code</span></div>
  <div class="docs-cell"><div class="penna-card" data-penna="card.figure" style="padding: var(--penna-inset-card)"><span class="penna-type" data-type="small" data-penna-text>Name · Size · Kind</span></div><span class="docs-note">Figure</span></div>
  <div class="docs-cell"><div class="penna-card penna-inline" data-penna="card.image" style="width: 72px; height: 72px; justify-content: center"><span class="penna-type" data-type="caption" data-penna-text>image</span></div><span class="docs-note">Image</span></div>
</div>
```

- **Bubble**: your messages in a chat.
- **Code**: code blocks and inline code.
- **Figure**: tables. No fill of its own, no outline.
- **Image**: image tiles in messages and the composer.

```rust
Card::new("snippet", CardLook::Code)
    .p(Space::INSET_CARD)
    .child(code)
```

## Anatomy

1. **Container**: a step above its ground, no outline, no shadow, the `radius` knob.
2. **Inset**: `inset-card` 12 all round, set by the caller.
3. **Ground**: what sits on it passes the card's ground, so muted text and badges follow its fill.

Cards in a column sit `gap-group` 16 apart.

## Inset

Pad every card with `inset-card`. Don't pick a padding by eye.

```figure uses=card,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-card" data-penna="card.bubble" style="padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div><figcaption>inset-card 12</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-card" data-penna="card.bubble" style="padding: var(--penna-space-2) var(--penna-space-6)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div><figcaption>Don't squeeze it</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Card::new(id, CardLook::Bubble)` | `.penna-card`, `data-penna="card.bubble"` | Bubble look |
| `CardLook::Code`, `Figure`, `Image` | `card.code`, `card.figure`, `card.image` | Other looks |
| `.on(ground)` | | Paints it on another ground |
| `.on(cx.theme().ground_of(look))` on children | `data-ground` | Children paint on the card |

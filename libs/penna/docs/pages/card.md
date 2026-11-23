# Card

A box in the flow of content that holds one thing. To float over the view use a [panel](panel.html), for something to pick a [tile](tile.html).

## Looks

```figure uses=card,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-card" data-penna="card.bubble" role="group" style="max-width: 220px"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div><span class="docs-note">Bubble</span></div>
  <div class="docs-cell"><div class="penna-card penna-stack" data-penna="card.code" role="group" data-gap="tight"><span class="penna-type" data-type="caption" data-muted data-penna-text>rust</span><span class="penna-type" data-type="mono" data-penna-text>cargo test --all</span></div><span class="docs-note">Code</span></div>
  <div class="docs-cell"><div class="penna-card penna-stack" data-penna="card.figure" role="figure" data-gap="tight"><span class="penna-type" data-type="caption" data-muted data-penna-text>Name · Kind</span><span class="penna-type" data-type="small" data-penna-text>docs.rs · Rust</span></div><span class="docs-note">Figure</span></div>
  <div class="docs-cell"><div class="penna-card penna-inline" data-penna="card.image" role="figure" style="width: 68px; height: 68px; justify-content: center; color: var(--penna-muted); --penna-icon-size: 20px"><i data-icon="image"></i></div><span class="docs-note">Image</span></div>
  <div class="docs-cell"><div class="penna-card penna-inline" data-penna="card.activity" role="group" data-gap="related" style="cursor: var(--penna-click-cursor)"><i data-icon="bot" style="color: var(--penna-muted)"></i><span class="penna-stack"><span class="penna-type" data-type="body" data-strong data-penna-text>Build docs</span><span class="penna-type" data-type="caption" data-muted data-penna-text>Done in 2.4 s</span></span></div><span class="docs-note">Activity</span></div>
</div>
```

- **Bubble**: your messages in an agent chat.
- **Code**: code blocks and inline code.
- **Figure**: tables and diagrams. No fill of its own, no outline.
- **Image**: image tiles in messages and the composer.
- **Activity**: subagents, background tasks, plans and answered questions; washes deeper on hover when given `on_click`.

```rust
Card::new("snippet", CardLook::Code)
    .child(code)
```

## Anatomy

1. **Container**: a step above its ground, no outline, no shadow, the `radius` knob.
2. **Inset**: `inset-card` 12 all round, set by the card. A caller's padding replaces it.
3. **Ground**: what sits on it passes `cx.theme().ground_of(look)`, so muted text and badges follow its fill.

Cards in a column sit `gap-group` 16 apart; a heading sits `gap-related` 8 above its card.

## Ground

On a panel the card rises one step from the panel's fill, not the window's. Marking the card as a ground lets badges and muted text inside follow its fill.

```figure uses=panel,card,badge,label
<div class="penna-panel" data-penna="panel" data-ground role="group" style="width: min(100%, 300px); padding: var(--penna-inset-card)">
  <div class="penna-card penna-stack" data-penna="card.bubble" data-ground role="group" data-gap="tight">
    <div class="penna-inline" data-gap="related" style="justify-content: space-between">
      <span class="penna-type" data-type="body" data-penna-text>A step above the panel</span>
      <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>2</span></span>
    </div>
    <span class="penna-type" data-type="small" data-muted data-penna-text>Muted text painted on the bubble's ground</span>
  </div>
</div>
```

## Inset

Keep the card's own `inset-card`. Don't pick a padding by eye.

```figure uses=panel,card,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="panel" data-ground role="group"><div class="penna-card" data-penna="card.bubble" role="group"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div></div><figcaption>inset-card 12</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="panel" data-ground role="group"><div class="penna-card" data-penna="card.bubble" role="group" style="padding: var(--penna-space-2) var(--penna-space-6)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div></div><figcaption>Don't squeeze it</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Card::new(id, CardLook::Bubble)` | `.penna-card`, `data-penna="card.bubble"`, `role="group"` | Bubble look |
| `CardLook::Code`, `Figure`, `Image`, `Activity` | `card.code`, `card.activity`; `card.figure`, `card.image` with `role="figure"` | Other looks |
| `.tone(tone)` | `data-tone` | Tints the card's wash |
| `.p(space)` | `padding` | Replaces the inset |
| `.on_click(handler)` | Click handler | Makes `Activity` wash deeper on hover |
| `.on(ground)` | `data-ground` on parent | Paints it a step above that ground |
| `.on(cx.theme().ground_of(look))` | `data-ground` on the card | Children paint on the card |

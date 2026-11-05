# Panel

A surface that holds chrome where the app puts it. For one that opens from a button use a popover, for content a card.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>The HTML panel sets no role, so a toast isn't announced. Its script also reads the layout inset's data-inset and squares those corners.</span></div></div></div>
```

## Looks

```figure uses=panel,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-panel" data-penna="popover" style="width: 200px"><div class="penna-inset penna-stack" data-inset="card" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Find in pane</span><span class="penna-type" data-type="small" data-muted data-penna-text>3 matches</span></div></div><span class="docs-note">Popover, Inline, Picker, Toast</span></div>
  <div class="docs-cell"><div class="penna-panel" data-penna="panel" style="width: 200px"><div class="penna-inset penna-stack" data-inset="card" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Sessions</span><span class="penna-type" data-type="small" data-muted data-penna-text>Covers what is under it</span></div></div><span class="docs-note">Plain</span></div>
</div>
```

- **Popover**: menus, the find bar, the palette.
- **Inline**: raised inside a view: a question, a failed step.
- **Picker**: searches a list and picks from it.
- **Toast**: notifications. Announced as a status.
- **Plain**: an opaque cover, such as a slide-over.

The four floating looks paint the same.

```rust
Panel::new("find", PanelLook::Popover)
    .p(Space::INSET_CARD)
    .child(find_bar)
```

## Anatomy

1. **Container**: glass or the popover fill, no outline, the float shadow. The `radius` knob; Plain is square.
2. **Inset**: none of its own. Pad with `inset-card` 12, or the row inset around rows.
3. **Corner**: `.inset(px)` grows a rounded corner by the inset its rows sit at.

## Corners

Grow the corner by the inset, so what sits inside stays concentric.

```figure uses=panel,well,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="picker" data-inset="8" style="width: 200px; padding: var(--penna-space-8)"><div class="penna-well" data-penna="well" style="height: 72px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="small" data-penna-text>Preview</span></div></div></div><figcaption>.inset(8) on the panel</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="picker" style="width: 200px; padding: var(--penna-space-8)"><div class="penna-well" data-penna="well" style="height: 72px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="small" data-penna-text>Preview</span></div></div></div><figcaption>Don't nest equal corners</figcaption></figure>
</div>
```

## Placement

Floating looks sit over the view, above cards. Content in the flow goes in a card.

```figure uses=panel,card,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-card" data-penna="card.bubble" style="width: 220px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div><figcaption>A card in the flow</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="popover" style="width: 220px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div><figcaption>Don't float content</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Panel::new(id, PanelLook::Popover)` | `.penna-panel`, `data-penna="popover"` | Popover look |
| `PanelLook::Inline`, `Picker`, `Toast`, `Plain` | `inline-panel`, `picker`, `toast`, `panel` | Other looks |
| `.inset(px)` | `data-inset` | Grows a rounded corner by that inset |
| `.p(Space::INSET_CARD)` | a `.penna-inset` child, `data-inset="card"` | Inset; the caller's |
| `.on(cx.theme().ground_of(look))` on children | `data-ground` | Children paint on the panel |

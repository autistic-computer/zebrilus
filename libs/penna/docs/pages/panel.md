# Panel

A surface that holds chrome where the app puts it. For one that opens from a button use a [popover](popover.html), for content a [card](card.html).

## Looks

```figure uses=panel,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-panel" data-penna="popover" data-ground role="group" style="width: 200px"><div class="penna-inset penna-stack" data-inset="card" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Find in pane</span><span class="penna-type" data-type="small" data-muted data-penna-text>3 matches</span></div></div><span class="docs-note">Popover, Inline, Picker, Toast</span></div>
  <div class="docs-cell"><div class="penna-panel" data-penna="panel" data-ground role="group" style="width: 200px"><div class="penna-inset penna-stack" data-inset="card" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Sessions</span><span class="penna-type" data-type="small" data-muted data-penna-text>Covers what is under it</span></div></div><span class="docs-note">Plain</span></div>
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

1. **Container**: glass and the float shadow, or where glass is off the popover fill under a hairline and a tighter float shadow. The `radius` knob; Plain is square.
2. **Inset**: none of its own. Pad with `inset-card` 12, or the row inset around rows.
3. **Corner**: `.inset(px)` computes the outer corner as the inner row's corner (at `.rows(height)`, 28 by default) plus that inset, so rows sit concentric inside.

## Corners

Grow the outer corner by the inset around its rows so inner and outer curves stay concentric.

```figure uses=panel,list
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="picker" data-ground role="group" data-inset="8" style="width: 200px; padding: var(--penna-space-8)"><div class="penna-list" role="listbox"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div></div></div><figcaption>.inset(8) on the panel</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="picker" data-ground role="group" style="width: 200px; padding: var(--penna-space-8)"><div class="penna-list" role="listbox"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div></div></div><figcaption>Don't nest mismatched corners</figcaption></figure>
</div>
```

## Placement

Floating looks sit over the view, above cards. Content in the flow goes in a [card](card.html).

```figure uses=panel,card,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="panel" data-ground role="group"><div class="penna-card" data-penna="card.bubble" role="group" style="width: min(100%, 220px)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div></div><figcaption>A card in the flow</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="popover" data-ground role="group" style="width: min(100%, 220px); padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span></div></div><figcaption>Don't float content</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Panel::new(id, PanelLook::Popover)` | `.penna-panel`, `data-penna="popover"`, `role="group"` | Popover look |
| `PanelLook::Inline`, `Picker`, `Plain` | `inline-panel`, `picker`, `panel` with `role="group"` | Other looks |
| `PanelLook::Toast` | `toast` with `role="status"` | Announced as a status |
| `.inset(px)` | `data-inset="8"` | Concentric outer corner around inset rows |
| `.rows(height)` | `data-rows` | Row height used by `.inset(px)` (28 by default) |
| `.p(Space::INSET_CARD)` | `.penna-inset[data-inset="card"]` | Caller-set padding |
| `.child(el)`, `.children(iter)` | Child elements | Panel content |
| `.on(cx.theme().ground_of(look))` on children | `data-ground` on the panel | Children paint on the panel's ground |

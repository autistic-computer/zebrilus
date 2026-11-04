# Row

The fill behind a row the app lays out itself: menu options, sidebar entries, the sidebar's cursor. For a ready list row use a list.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Only the sidebar look draws a focus ring; the others show focus as selection. The HTML row also fixes a size, text and icon that the native row leaves to you.</span></div></div></div>
```

## Looks

```figure uses=row
<div class="docs-tokens">
  <span class="docs-head">Look</span><span class="docs-head">Recipe</span><span class="docs-head">Use</span><span class="docs-head">Selected</span>
  <span>Menu</span><span><code>menu-item</code></span><span>Menus, pickers</span><span><div class="penna-row" data-penna="menu-item" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>MenuCard</span><span><code>menu-item.card</code></span><span>Picker cards</span><span><div class="penna-row" data-penna="menu-item.card" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>MenuTint</span><span><code>menu-item.tint</code></span><span>Suggestions</span><span><div class="penna-row" data-penna="menu-item.tint" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Sidebar</span><span><code>sidebar.row</code></span><span>Sidebar rows</span><span><div class="penna-row" data-penna="sidebar.row" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Nav</span><span><code>row.nav</code></span><span>Navigation</span><span><div class="penna-row" data-penna="row.nav" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Cursor</span><span><code>row.cursor</code></span><span>Sidebar cursor</span><span><div class="penna-row" data-penna="row.cursor" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
</div>
```

`.tone(Tone::Danger)` for a row that destroys something.

## Anatomy

```figure uses=row
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>Hosts</span></span><span class="penna-row-aside"><span class="penna-text" data-penna-text>3</span></span></div><span class="docs-note">Fill, icon, label, aside</span></div>
</div>
```

1. **Fill**: the look's fill, outline and corners.
2. **Content**: yours: icon, label, a muted aside.

`.row_height()` sizes the corners. The height is yours.

## States

```figure uses=row,tab
<div class="docs-show">
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover active" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="sidebar.row" data-state="focused" tabindex="0"><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Focused, sidebar</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="disabled" aria-disabled="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Off</span></div>
</div>
```

- Current paints like selected.
- `.interactive(false)`: the state alone, no hover or press.

## Spacing

In an inset list rows sit 2 apart, so highlights never merge.

```figure uses=row,list
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-list"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><div class="penna-row" data-penna="menu-item" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div><figcaption>Rows 2 apart</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-list" style="gap: 0"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><div class="penna-row" data-penna="menu-item" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div><figcaption>Don't let highlights touch</figcaption></figure>
</div>
```

```rust
Row::new("hosts", RowLook::Sidebar)
    .state(PaintState::Current)
    .row_height(height)
    .h(height)
    .px(Space::S12)
    .child(Label::new(TextStyle::Small, "Hosts"))
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Row::new(id, look)` | `.penna-row[data-penna]` | Fill in a look |
| `.state(state)` | `data-state`, `aria-selected`, `aria-disabled` | State from the app |
| `.tone(tone)` | `data-tone` | Tone |
| `.row_height(height)` | none | Corners for that height |
| `.radius(px)` | `.penna-row-fill[data-round]` | A round look's radius |
| `.hover_group(group)` | `.penna-row-fill` | A fill that lights with its row |
| `.press_highlight(true)` | none | Press highlight on touch |
| `.interactive(false)` | `data-still` | State only |
| `.on(ground)` | none | Paints on a ground |

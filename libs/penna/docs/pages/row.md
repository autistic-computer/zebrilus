# Row

The fill behind a row the app lays out itself: menu options, sidebar entries, the sidebar's cursor. For a ready list row use a [list](list.html); inside a menu, a [menu](menu.html) row.

## Looks

```figure uses=row
<div class="docs-tokens">
  <span class="docs-head">Look</span><span class="docs-head">Recipe</span><span class="docs-head">Use</span><span class="docs-head">Selected</span>
  <span>Menu</span><span><code>menu-item</code></span><span>Menus, pickers (default)</span><span><div class="penna-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>MenuCard</span><span><code>menu-item.card</code></span><span>Picker cards</span><span><div class="penna-row" data-penna="menu-item.card" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>MenuTint</span><span><code>menu-item.tint</code></span><span>Tinted rest, suggestions</span><span><div class="penna-row" data-penna="menu-item.tint" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Sidebar</span><span><code>sidebar.row</code></span><span>Sidebar rows (glass)</span><span><div class="penna-row" data-penna="sidebar.row" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Nav</span><span><code>row.nav</code></span><span>Navigation (glass)</span><span><div class="penna-row" data-penna="row.nav" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
  <span>Cursor</span><span><code>row.cursor</code></span><span>Outlined cursor</span><span><div class="penna-row" data-penna="row.cursor" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div></span>
</div>
```

`MenuTint` carries a tinted rest fill, `Cursor` draws a rest outline, and `Sidebar` and `Nav` select as glass. `.tone(Tone::Danger)` marks a destructive row.

## Anatomy

```figure uses=row
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>Hosts</span></span><span class="penna-row-aside"><span class="penna-text" data-penna-text>3</span></span></div><span class="docs-note">Fill, icon, label, aside</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-tone="danger" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="xmark"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>Delete session</span></span></div><span class="docs-note">Danger tone</span></div>
</div>
```

1. **Fill**: the look's fill, outline and corners; `.row_height()` sizes the corners, or `.radius(px)` sets a fixed corner on multi-line rows.
2. **Content**: yours — icon, label and muted `caption` aside spaced `space-8` apart.
3. **Fill row**: `.hover_group(group)` draws an inset fill (2 vertical, `space-4` horizontal) behind a taller parent row and stays out of the accessibility tree.

## States

```figure uses=row
<div class="docs-show">
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="hover active" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="focused" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" role="option" data-state="selected focused" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Selected, focused</span></div>
  <div class="docs-cell"><div class="penna-row" data-penna="menu-item" data-state="disabled" aria-disabled="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><span class="docs-note">Off</span></div>
</div>
```

- Current paints like selected.
- Focus is a ring, never a fill. The parent list owns focus and arrow keys; a row announces `Role::ListItem` with its id unless it is a fill row.
- `.interactive(false)` shows the state alone without hover or press; `.press_highlight(true)` adds a touch press wash on a fill row.

## Spacing

In an inset list rows sit 2 apart so highlights never merge; in a plain full-bleed list, square rows touch.

```figure uses=row,list
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-list" role="listbox"><div class="penna-row" data-penna="menu-item" role="option" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><div class="penna-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div><figcaption>Rows 2 apart</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-list" role="listbox" style="gap: 0"><div class="penna-row" data-penna="menu-item" role="option" data-state="hover" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>work</span></span></div><div class="penna-row" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-still><i data-icon="folder"></i><span class="penna-row-label"><span class="penna-text" data-penna-text>logs</span></span></div></div></div><figcaption>Don't let highlights touch</figcaption></figure>
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
| `Row::new(id, look)` | `.penna-row[data-penna]` | Fill in a look (`RowLook::Menu` default) |
| `.state(state)` | `data-state`, `aria-selected`, `aria-disabled` | State from the app |
| `.focused(true)` | `data-state="focused"`, `data-state="selected focused"` | Focus ring over any state |
| `.tone(tone)` | `data-tone` | Tone |
| `.row_height(height)` | none | Corners for that height |
| `.radius(px)` | `--penna-r`, `--penna-share: 0`, `--penna-k: var(--penna-style-k)` | Fixed corner radius |
| `.hover_group(group)` | `.penna-row-fill` | Inset fill lighting with its parent row |
| `.press_highlight(true)` | none | Press highlight on touch |
| `.interactive(false)` | `data-still` | State only, no hover or press |
| `.on(ground)` | `data-ground` on parent | Paints on a ground |

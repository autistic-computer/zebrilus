# List

A column of things to open or pick: hosts, sessions, files. For actions use a menu, for nested things a tree.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>A list row takes no focus yet, so a list has no keyboard path unless the app adds one.</span></div></div></div>
```

## Anatomy

```figure uses=list
<div class="penna-list" role="listbox" style="width: 320px"><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>3 panes</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>2 panes</span></span></div></div></div>
```

1. **Row**: 12 at the sides, the look's fill and corners.
2. **Icon**: optional, 14, muted.
3. **Label**: `lead`, one line.
4. **Detail**: optional, `small`, muted, at the end.

Rows sit 2 apart so their highlights never merge. Any row look works; menu is the default.

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Density</span><span class="docs-head">Row</span><span class="docs-head">Text</span><span class="docs-head">Icon</span>
  <span>compact</span><span><code>24</code></span><span><code>lead</code></span><span><code>14</code></span>
  <span>regular</span><span><code>28</code></span><span><code>lead</code></span><span><code>14</code></span>
  <span>comfortable</span><span><code>32</code></span><span><code>lead</code></span><span><code>14</code></span>
</div>
```

Only the height moves. `.row_height()` makes a taller row.

## States

```figure uses=list
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="hover"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="hover active"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="disabled" aria-disabled="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Off</span></div>
</div>
```

- Hover and press only at rest.
- Off: dimmed, ignores clicks.

## Content

The thing's name, then a short fact.

```figure uses=list
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-list" style="width: 220px"><div class="penna-list-item" data-penna="menu-item"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>3 panes</span></span></div></div><div class="penna-list-item" data-penna="menu-item"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>detached</span></span></div></div></div></div><figcaption>Name, then a word or two</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-list" style="width: 220px"><div class="penna-list-item" data-penna="menu-item"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>Session work, 3 panes</span></span></div></div><div class="penna-list-item" data-penna="menu-item"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>Session logs (detached)</span></span></div></div></div></div><figcaption>Don't pack it into the label</figcaption></figure>
</div>
```

```rust
ListItem::new("work").selected(true).on_click(open).child(
    h_flex()
        .gap(Space::S8)
        .child(Icon::new(IconName::SquareTerminal).medium())
        .child("work"),
)
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `ListItem::new(id)` | `.penna-list-item[data-penna="menu-item"]` | Row |
| `.look(look)` | `data-penna` | Row look |
| `.selected(true)` | `aria-selected="true"` | Selected |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.row_height(height)` | `--penna-list-row` | Minimum height and corners |
| `.on_click(handler)` | `click` | Runs on release |
| `.on_mouse_enter(handler)` | `pointerenter` | Follows the pointer |
| `.list_inset(cx)` on the column | `.penna-list` | Rows 2 apart |

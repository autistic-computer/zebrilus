# List

A column of things to open or pick: hosts, sessions, files. For actions use a menu, for nested things a tree.

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
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="focused"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><div class="penna-list" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" data-state="disabled" aria-disabled="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Off</span></div>
</div>
```

- Hover and press only at rest.
- Focused: a ring, over the selected fill when it is selected too. The list owns focus and the arrow keys.
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
| `.focused(true)` | `data-state="focused"` | Focus ring |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.row_height(height)` | `--penna-list-row` | Minimum height and corners |
| `.on_click(handler)` | `click` | Runs on release |
| `.on_mouse_enter(handler)` | `pointerenter` | Follows the pointer |
| `.list_inset(cx)` on the column | `.penna-list` | Rows 2 apart |

## Touch rows

On a phone, lists use touch rows: found machines, chats, settings.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Built inside sheets only</span><span class="penna-text penna-notice-detail" data-penna-text>One-line and two-line rows exist in sheets. Rows elsewhere, the group, the trailing kinds, the swipe and the section header are drawn with the HTML kit.</span></div></div></div>
```

```figure uses=touch
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="palette"></i><span class="penna-touch-row-lines"><span>Appearance</span></span><i data-icon="chevron-right"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>Machines</span></span><span class="penna-touch-row-value">3</span><i data-icon="chevron-right"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>3 chats running</span></span><span class="penna-touch-row-state"><span class="penna-tone-dot"></span>Connected</span></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>alienware.tail2c4e.ts.net</span></span><span class="penna-touch-row-state"><i class="penna-touch-spinner" data-icon="loader"></i>Connecting</span></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>xps</span><span>On your tailnet · runs zz</span></span><i data-icon="plus" data-add></i></div>
    <span class="docs-note">One line 52, two lines 64; chevron, value, dot, spinner, add</span>
  </div>
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>Rest</span><span>On your tailnet · runs zz</span></span><i data-icon="plus" data-add></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="active" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>Pressed</span><span>On your tailnet · runs zz</span></span><i data-icon="plus" data-add></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
    <span class="docs-note">Rest, pressed, disabled</span>
  </div>
</div>
```

- **Row**: inset 12, children 12 apart, a 20 icon, title `touch-body`, detail `touch-small` 2 under it.
- **Trailing**: a chevron when the tap zooms in, a value, a dot and its word, a spinner and its word, or the add mark when the tap adds the thing. Nothing in a row is a button.
- **Disabled**: the detail says why, and there is no trailing mark.

```figure uses=touch,button
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-header" style="width: 100%"><span>Found on your tailnet</span><span class="penna-touch-count">3</span></div>
    <div class="penna-touch-group" data-penna="settings.card" style="width: 100%">
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>macOS · runs zz</span></span><i data-icon="plus" data-add></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>Linux · runs zz</span></span><i data-icon="plus" data-add></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
    </div>
    <span class="docs-note">Section header and a group in an inset card</span>
  </div>
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-group" data-penna="settings.card" style="width: 100%">
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>Connected · 3 chats</span></span><i data-icon="chevron-right"></i></div>
      <div class="penna-touch-swipe" data-open><div class="penna-touch-row" data-penna="phone.row" data-lines="2"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>xps</span><span>Not connected · 2 days ago</span></span><i data-icon="chevron-right"></i></div><button class="penna-touch-swipe-action" data-penna="button" data-emphasis="solid" data-solid="danger" style="color: var(--penna-on-danger)">Forget</button></div>
    </div>
    <span class="docs-note">Swiped to Forget, 64 wide</span>
  </div>
</div>
```

The header is 28 tall, strong `touch-small`, muted, with an optional count. Rows in a card sit 2 apart with a hairline between them from the title's start.

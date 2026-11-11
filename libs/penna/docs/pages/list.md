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
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Built inside sheets only</span><span class="penna-text penna-notice-detail" data-penna-text>One-line and two-line rows exist in sheets. Rows elsewhere, the group, the trailing kinds, the danger row, the swipe and the section header are drawn with the HTML kit.</span></div></div></div>
```

```figure uses=touch
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="palette"></i><span class="penna-touch-row-lines"><span>Appearance</span></span><i data-icon="chevron-right"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>Machines</span></span><span class="penna-touch-row-value">3</span><i data-icon="chevron-right"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>3 chats running</span></span><span class="penna-touch-row-state"><span class="penna-tone-dot"></span>Connected</span></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>alienware.tail2c4e.ts.net</span></span><span class="penna-touch-row-state"><i class="penna-touch-spinner" data-icon="loader"></i>Connecting</span></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>xps</span><span>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
    <span class="docs-note">One line 52, two lines 64; chevron, value, dot, spinner, add</span>
  </div>
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>Rest</span><span>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="active" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>Pressed</span><span>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
    <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
    <span class="docs-note">Rest, pressed, disabled</span>
  </div>
</div>
```

- **Row**: inset 12, children 12 apart, a 20 icon, title `touch-body`, detail `touch-small` 2 under it. Its corner comes from its height, like a menu row's, and its fill is its own rounded shape.
- **Trailing**: a chevron when the tap zooms in, a plus when it adds the thing, a value, a dot and its word, or a spinner and its word. A trailing glyph is 20 in the muted ink, like the leading icon; never the accent. Nothing in a row is a button.
- **Disabled**: the detail says why, there is no trailing mark, and it takes no fill.

```figure uses=touch,button
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-header" style="width: 100%"><span>Found on your tailnet</span><span class="penna-touch-count">3</span></div>
    <div class="penna-touch-group" data-penna="settings.card" style="width: 100%">
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" data-state="hover"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>macOS · runs zz</span></span><i data-icon="plus"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>Linux · runs zz</span></span><i data-icon="plus"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
    </div>
    <span class="docs-note">Section header and a group, one row hovered</span>
  </div>
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-group" data-penna="settings.card" style="width: 100%">
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>Connected · 3 chats</span></span><i data-icon="chevron-right"></i></div>
      <div class="penna-touch-swipe" data-penna-clip data-open><div class="penna-touch-row" data-penna="phone.row" data-lines="2"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>xps</span><span>Not connected · 2 days ago</span></span><i data-icon="chevron-right"></i></div><button class="penna-touch-swipe-action" data-penna="button" data-emphasis="solid" data-solid="danger" style="color: var(--penna-on-danger)">Forget</button></div>
    </div>
    <span class="docs-note">Swiped to Forget, 64 wide</span>
  </div>
</div>
```

```figure uses=touch,compact
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-header" data-hold style="width: 100%"><span>zz</span><button class="penna-touch-icon" data-penna="phone.button" aria-label="New in zz"><i data-icon="plus"></i></button></div><div class="penna-bento" style="--rows: 2; --penna-bento-width: 370px; width: 370px"><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 0; --row: 0; --columns: 2; --span: 2"><span class="penna-machine-name"><span>cargo watch</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><pre class="penna-window-term">~/dev/zebrilus main&#10;<b>❯ cargo watch -x &#x27;check -p zz-mux&#x27;</b>&#10;[Running &#x27;cargo check -p zz-mux&#x27;]&#10;    Checking zz-mux v0.42.0&#10;<span data-ink="success">    Finished `dev` profile in 2.14s</span></pre></span></span><span class="penna-press"></span></div><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 2; --row: 0; --columns: 2; --span: 2"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="warning"></span><span>Pick a tailnet library</span></span><span class="penna-machine-state" data-tone="warning">Has a question</span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><span class="penna-window-chat"><span class="penna-window-bubble"><i style="--w: 0.86"></i><i style="--w: 0.3"></i></span><span class="penna-window-lines"><i style="--w: 1"></i><i style="--w: 0.92"></i><i style="--w: 0.6"></i></span></span></span></span><span class="penna-press"></span></div></div>
    <span class="docs-note">A session header you can hold, 44, with its plus</span>
  </div>
  <div class="docs-cell" style="width: 370px">
    <div class="penna-touch-group" data-penna="settings.card" style="width: 100%">
      <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename session</span></span></div>
      <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Close session</span></span></div>
    </div>
    <span class="docs-note">A danger row, last</span>
  </div>
</div>
```

The header is 28 tall, strong `touch-small`, muted, with an optional count of what is under it, lined up with the rows' icons. A header you can hold (a session over its windows) is 44. A session's header ends in a 44 plus, muted, its glyph 12 in from the grid's edge as the name is at the other end; it zooms into the New card for that session. The header and its plus are two targets side by side. A row that closes, removes or forgets takes the danger ink and comes last. A group has the phone container corner, twice `radius`. Its rows sit 4 in from its edges and 2 apart, with no separators, so a pressed or hovered row's fill never meets an edge or a line; a row's corner is never rounder than the group's less 4.

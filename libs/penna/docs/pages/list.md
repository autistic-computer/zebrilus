# List

A column of things to open or pick: hosts, sessions, files. For actions use a [menu](menu.html), for nested things a [tree](tree.html).

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">Desktop lists match. Touch row swipe actions are drawn in HTML and CSS, not built natively yet.</span>
    </div>
  </div>
</div>
```

## Anatomy

```figure uses=list
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: min(100%, 280px)">
    <div class="penna-list" role="listbox" style="width: 100%">
      <div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>3 panes</span></span></div></div>
      <div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>2 panes</span></span></div></div>
    </div>
    <span class="docs-note">Menu look (default)</span>
  </div>
  <div class="docs-cell" style="width: min(100%, 220px)">
    <div class="penna-list" role="listbox" style="width: 100%">
      <div class="penna-list-item" data-penna="sidebar.row" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="folder"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>penna</span></span></div></div>
      <div class="penna-list-item" data-penna="sidebar.row" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="folder"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>zebrilus</span></span></div></div>
    </div>
    <span class="docs-note">Sidebar look</span>
  </div>
</div>
```

1. **Row**: 12 at the sides, the look's fill and corners.
2. **Icon**: optional, 14, muted.
3. **Label**: `lead`, one line.
4. **Detail**: optional, `small`, muted, at the end.

Rows sit 2 apart so their highlights never merge. Any [row](row.html) look works (`menu-item` by default, `menu-item.tint`, `sidebar.row`).

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
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="hover"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="hover active"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="focused"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><div class="penna-list" role="listbox" style="width: 104px"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="disabled" aria-disabled="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></div></div></div><span class="docs-note">Off</span></div>
</div>
```

- Hover and press only at rest.
- Focused: a ring, over the selected fill when it is selected too. The list owns focus and the arrow keys.
- Off: dimmed, ignores clicks.

## Content

The thing's name, then a short fact.

```figure uses=list
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-list" role="listbox" style="width: min(100%, 220px)"><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>3 panes</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span><span class="penna-list-detail"><span class="penna-text" data-penna-text>detached</span></span></div></div></div></div><figcaption>Name, then a word or two</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-list" role="listbox" style="width: min(100%, 220px)"><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>Session work, 3 panes</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>Session logs (detached)</span></span></div></div></div></div><figcaption>Don't pack it into the label</figcaption></figure>
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
| `.look(look)` | `data-penna` | Row look (`menu-item`, `menu-item.tint`, `sidebar.row`) |
| `.selected(true)` | `data-state="selected"`, `aria-selected="true"` | Selected |
| `.focused(true)` | `data-state="focused"` | Focus ring |
| `.disabled(true)` | `data-state="disabled"`, `aria-disabled="true"` | Off |
| `.row_height(height)` | `--penna-list-row` | Minimum height and corners |
| `.role(role)` | `role` | Accessibility role (`option` by default) |
| `.on_click(handler)` | `click` | Runs on release |
| `.on_mouse_down(button, handler)` | `pointerdown` | Runs on press |
| `.on_mouse_enter(handler)` | `pointerenter` | Follows the pointer |
| `.child(el)`, `.children(iter)` | `.penna-list-content` | Row content |
| `.list_inset(cx)` on the column | `.penna-list` | Rows 2 apart |

## Touch rows

On a phone, lists use touch rows: found machines, chats, settings.

```figure uses=touch
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Trailing marks</span>
      <span class="docs-lk-mono">52 px · 64 px</span>
    </div>
    <div class="penna-stack" data-gap="tight">
      <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="palette"></i><span class="penna-touch-row-lines"><span data-penna-text>Appearance</span></span><i data-icon="chevron-right"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>Machines</span></span><span class="penna-touch-row-value" data-penna-text>3</span><i data-icon="chevron-right"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" role="button" style="width: 100%"><i data-icon="moon"></i><span class="penna-touch-row-lines"><span data-penna-text>Dark</span></span><span class="penna-touch-row-state"><i data-icon="check"></i></span></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>macbook</span><span data-penna-text>3 chats running</span></span><span class="penna-touch-row-state"><span class="penna-tone-dot"></span><span data-penna-text>Connected</span></span></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>alienware</span><span data-penna-text>alienware.tail2c4e.ts.net</span></span><span class="penna-touch-row-state"><i class="penna-touch-spinner" data-icon="loader"></i><span data-penna-text>Connecting</span></span></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>xps</span><span data-penna-text>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">States</span>
      <span class="docs-lk-mono">Rest · pressed · off</span>
    </div>
    <div class="penna-stack" data-gap="tight">
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>Rest</span><span data-penna-text>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="active" role="button" style="width: 100%"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>Pressed</span><span data-penna-text>On your tailnet · runs zz</span></span><i data-icon="plus"></i></div>
      <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true" style="width: 100%"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>homelab</span><span data-penna-text>No zz on it yet</span></span></div>
    </div>
  </div>
</div>
```

- **Row**: inset 12, children 12 apart, a 20 icon, title `touch-body`, detail `touch-small` 2 under it. Its corner comes from its height, like a menu row's, and its fill is its own rounded shape.
- **Trailing**: a chevron when the tap zooms in, a plus when it adds the thing, a check on the picked option, a value, a dot and its word, or a spinner and its word. A trailing glyph is 20 in muted ink, never the accent. Nothing in a row is a button.
- **Disabled**: the detail says why, there is no trailing mark, and it takes no fill.

```figure uses=touch,button
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Grouped section</span>
      <span class="docs-lk-mono">4 px inset</span>
    </div>
    <div class="penna-stack" data-gap="tight">
      <div class="penna-touch-header" style="width: 100%"><span data-penna-text>Found on your tailnet</span><span class="penna-touch-count" data-penna-text>3</span></div>
      <div class="penna-touch-group" data-penna="settings.card" data-ground style="width: 100%">
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" data-state="hover"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>macbook</span><span data-penna-text>macOS · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>alienware</span><span data-penna-text>Linux · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>homelab</span><span data-penna-text>No zz on it yet</span></span></div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Swipe &amp; danger</span>
      <span class="docs-lk-mono">64 px reveal</span>
    </div>
    <div class="penna-stack" data-gap="related">
      <div class="penna-touch-group" data-penna="settings.card" data-ground style="width: 100%">
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>macbook</span><span data-penna-text>Connected · 3 chats</span></span><i data-icon="chevron-right"></i></div>
        <div class="penna-touch-swipe" data-penna-clip data-open><div class="penna-touch-row" data-penna="phone.row" data-lines="2"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>xps</span><span data-penna-text>Not connected · 2 days ago</span></span><i data-icon="chevron-right"></i></div><button class="penna-touch-swipe-action" data-penna="button" data-tone="danger" data-prominent type="button"><span data-penna-text>Forget</span></button></div>
      </div>
      <div class="penna-touch-group" data-penna="settings.card" data-ground style="width: 100%">
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span data-penna-text>Rename session</span></span></div>
        <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span data-penna-text>Close session</span></span></div>
      </div>
    </div>
  </div>
</div>
```

```figure uses=touch,compact
<div class="docs-cell" style="width: min(100%, 358px)">
  <div class="penna-touch-header" data-hold style="width: 100%"><span data-penna-text>zz</span></div>
  <div class="penna-bento" style="--rows: 2; --penna-bento-width: min(358px, calc(100vw - 32px))">
    <div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 0; --row: 0; --columns: 2; --span: 2"><span class="penna-machine-name"><span data-penna-text>cargo watch</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><pre class="penna-window-term">~/dev/zebrilus main&#10;<b>❯ cargo watch -x &#x27;check -p zz-mux&#x27;</b>&#10;[Running &#x27;cargo check -p zz-mux&#x27;]&#10;    Checking zz-mux v0.42.0&#10;<span data-ink="success">    Finished `dev` profile in 2.14s</span></pre></span></span><span class="penna-press"></span></div>
    <div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 2; --row: 0; --columns: 2; --span: 2"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="warning"></span><span data-penna-text>Pick a tailnet library</span></span><span class="penna-machine-state" data-tone="warning" data-penna-text>Has a question</span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><span class="penna-window-chat"><span class="penna-window-bubble"><i style="--w: 0.86"></i><i style="--w: 0.3"></i></span><span class="penna-window-lines"><i style="--w: 1"></i><i style="--w: 0.92"></i><i style="--w: 0.6"></i></span></span></span></span><span class="penna-press"></span></div>
  </div>
  <span class="docs-note">A session header you can hold, 44 tall</span>
</div>
```

The header is 28 tall, strong `touch-small`, muted, with an optional count lined up with the rows' icons. A holdable header over a session's windows is 44. A header holds no button: new things come from the pill's menu, never from the header. A row that closes, removes or forgets takes the danger ink and comes last. A group has the phone container corner (twice `radius`) with rows 4 in and 2 apart; inside sheets and modals, `TouchGroup::ruled()` separates rows with hairlines instead.

```rust
TouchHeader::new("Found on your tailnet").count(3);
TouchGroup::new()
    .row(
        TouchRow::new("macbook", "macbook")
            .icon(IconName::Laptop)
            .detail("macOS · runs zz")
            .trailing(TouchTrailing::Glyph(IconName::Plus)),
    )
    .row(TouchRow::new("close", "Close session").icon(IconName::Xmark).danger());
```

| Rust | HTML | Does |
| --- | --- | --- |
| `TouchRow::new(id, title)` | `.penna-touch-row[data-penna="phone.row"]` | Row, 52 tall |
| `.icon(icon)`, `.lead(el, width)`, `.slot()`, `.checked()` | `<i data-icon>` first | Leading glyph 20, custom lead, empty slot, or check |
| `.detail(text)` | second span in `.penna-touch-row-lines`, `data-lines="2"` | Second line, 64 tall |
| `.trailing(TouchTrailing::Glyph(icon))` | `<i data-icon>` last | Chevron or plus |
| `.trailing(TouchTrailing::Value(text))` | `.penna-touch-row-value` | A value |
| `.trailing(TouchTrailing::Check)` | `.penna-touch-row-state > i[data-icon="check"]` | The picked row |
| `.trailing(TouchTrailing::State(mark, word))` | `.penna-touch-row-state` | Status dot or spinner and word |
| `.end(el)`, `.more()`, `.face(family)` | Trailing child / font | Custom trailing element, wrapping detail, or typeface |
| `.danger()`, `.ink(TouchInk)`, `.on(ground)`, `.in_sheet(cx)` | `data-tone="danger"`, `data-ground` | Tone and surface ground |
| `.disabled(true)` | `aria-disabled="true"` | Faint title, no fill |
| `.on_click(handler)` | `click` | Runs on release |
| `TouchGroup::new().row(row)`, `.rows(iter)`, `.ruled()` | `.penna-touch-group[data-penna="settings.card"][data-ground]` | Group with phone corner, or ruled inside a sheet |
| `TouchHeader::new(title)`, `.count(n)` | `.penna-touch-header`, `.penna-touch-count` | Section header and count |
| `.holdable()`, `.on_hold(handler)`, `.over_tiles()` | `data-hold` | 44 tall holdable header, or inset 12 over a grid |

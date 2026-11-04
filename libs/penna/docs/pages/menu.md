# Menu

Actions that open from a button, a right click or a long press. To pick a value use a select.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Natively a menu opens only from the pointer. The keyboard can't open a dropdown or a context menu yet.</span></div></div></div>
```

## Kinds

```figure uses=menu,button
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-caret data-state="current" aria-haspopup="menu" aria-expanded="true"><span class="penna-label" data-penna-text>Layout</span><i class="penna-caret" data-icon="chevron-down"></i></button><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="check"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Even horizontal</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Tiled</span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="panel-right"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-penna-text>⌘D</span></span></div></div></div><span class="docs-note">Dropdown</span></div>
</div>
```

- **Dropdown**: below a button with a caret.
- **Context**: at the pointer, on a right click or a long press.
- **Submenu**: beside its row, once the row is highlighted.

```rust
Button::new("layout")
    .label("Layout")
    .dropdown_caret(true)
    .dropdown_menu(|menu, _, _| {
        menu.item(PopupMenuItem::new("Tiled").checked(true))
            .separator()
            .menu("Split right", Box::new(SplitRight))
    })
```

## Anatomy

```figure uses=menu
<div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="label" role="presentation" data-disabled><span class="penna-popup-line"><span class="penna-popup-icon"></span><span class="penna-popup-text"><span class="penna-label" data-penna-text>Pane</span></span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="plus"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-penna-text>⌘T</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="panel-right"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-penna-text>⌘D</span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="check"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Synchronize input</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="submenu" role="menuitem" aria-haspopup="menu"><span class="penna-popup-line"><span class="penna-popup-icon"><i data-icon="layers"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Move to</span><i class="penna-popup-detail" data-icon="chevron-right"></i></span></span></div></div></div>
```

1. **Section label**: names a group. Muted.
2. **Icon**: optional, or a check. Labels line up.
3. **Label**: a verb in sentence case.
4. **Shortcut**: from the row's action.
5. **Separator**: between groups.
6. **Chevron**: opens a submenu.

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Pointer</span><span class="docs-head">Row</span><span class="docs-head">Text</span><span class="docs-head">Icon</span>
  <span>mouse</span><span><code>28</code></span><span><code>small</code></span><span><code>12</code></span>
  <span>touch</span><span><code>44</code></span><span><code>body</code></span><span><code>14</code></span>
</div>
```

Rows follow density: 24 compact, 32 comfortable.

## States

```figure uses=menu
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-state="selected" data-selected><span class="penna-popup-main"><span class="penna-label" data-penna-text>Detach</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-disabled><span class="penna-popup-main"><span class="penna-label" data-penna-text>Rename</span></span></div></div></div><span class="docs-note">Rest, highlighted, off</span></div>
  <div class="docs-cell"><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="submenu" role="menuitem" aria-haspopup="menu" data-state="hover" data-selected><span class="penna-popup-line"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Move to</span><i class="penna-popup-detail" data-icon="chevron-right"></i></span></span><div class="penna-popup penna-popup-sub" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-state="selected" data-selected><span class="penna-popup-main"><span class="penna-label" data-penna-text>work</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>logs</span></span></div></div></div></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Monitor activity</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div><span class="docs-note">Submenu open</span></div>
</div>
```

- Highlight: one row, from the pointer or the arrow keys.
- Off: muted, skipped by the keys.
- The menu keeps focus; the highlight shows where the keyboard is.

## Keys

- **Up**, **Down**: move the highlight, wrapping.
- **Right**, **Left**: into and out of a submenu.
- **Enter**: run the row and close.
- **Escape**: close every level.

## Labels

A verb in sentence case. Give the row its action and the shortcut shows.

```figure uses=menu
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-penna-text>⌘T</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span><span class="penna-label penna-popup-key" data-penna-text>⌘W</span></span></div></div></div></div><figcaption>Shortcut in its column</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab (⌘T)</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane (⌘W)</span></span></div></div></div></div><figcaption>Don't type keys into labels</figcaption></figure>
</div>
```

## Groups

Related rows together, a separator between groups.

```figure uses=menu
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><figcaption>Part groups with a separator</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><figcaption>Don't mix groups</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.dropdown_menu(build)` on a button | `pennaMenu.dropdown(trigger, items)` | Opens below the button |
| `.context_menu(build)` | `pennaMenu.context(region, items)` | Opens at the pointer |
| `PopupMenuItem::new(label)` | `{ label }` | Row |
| `.icon(name)` | `{ icon }` | Icon |
| `.action(action)` | `{ key }` | Runs an action; shows its key |
| `.on_click(handler)` | `penna-menu-action` | Runs on release or Enter |
| `.checked(true)` | `{ checked: true }` | Check |
| `.disabled(true)` | `{ disabled: true }` | Off |
| `.submenu_with_icon(icon, label, window, cx, build)` | `{ kind: "submenu", items }` | Submenu |
| `.link(label, href)` | `{ label, link }` | Opens a URL |
| `PopupMenuItem::stepper(label, value, down, reset, up)` | `{ kind: "stepper" }` | Steps a value in place |
| `.label(text)` | `{ kind: "label" }` | Section label |
| `.separator()` | `{ kind: "separator" }` | Separator |
| `.scrollable(true)`, `.max_h(height)` | none | Scrolls past a height |

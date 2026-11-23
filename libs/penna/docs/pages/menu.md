# Menu

Actions that open from a button, a right click or a long press. To pick a value use a [select](select.html).

## Kinds

```figure uses=menu,button
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-caret data-state="current" type="button" aria-haspopup="menu" aria-expanded="true"><span class="penna-label" data-penna-text>Layout</span><i class="penna-caret" data-icon="chevron-down"></i></button><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="check"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Even horizontal</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Tiled</span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="panel-right"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-kbd-text="cmd-d" data-penna-text></span></span></div></div></div><span class="docs-note">Dropdown</span></div>
</div>
```

- **Dropdown**: below a button with a caret, on a press or on Enter, Space or Down.
- **Context**: at the pointer, on a right click or a long press; at the region's top-left on Shift+F10 or the Menu key.
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
<div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="label" role="presentation" data-disabled><span class="penna-popup-line"><span class="penna-popup-icon"></span><span class="penna-popup-text"><span class="penna-label" data-penna-text>Pane</span></span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="plus"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-kbd-text="cmd-t" data-penna-text></span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="panel-right"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-kbd-text="cmd-d" data-penna-text></span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"><i data-icon="check"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Synchronize input</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Documentation</span><i class="penna-popup-detail" data-icon="external-link"></i></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="submenu" role="menuitem" aria-haspopup="menu"><span class="penna-popup-line"><span class="penna-popup-icon"><i data-icon="layers"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Move to</span><i class="penna-popup-detail" data-icon="chevron-right"></i></span></span></div></div></div>
```

1. **Surface**: the `popover` look with 6 px row inset, 128 to 500 wide.
2. **Section label**: names a group, muted.
3. **Icon**: optional, or a check; labels line up across the column.
4. **Label**: a verb in sentence case, or a noun for a checked option; end with `…` when more input comes first.
5. **Trailing mark**: a plain-text shortcut from the row's action, an `external-link` mark on links, a stepper control, or a chevron on submenus.
6. **Separator**: hairline between groups.

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
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell"><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-state="selected" data-selected><span class="penna-popup-main"><span class="penna-label" data-penna-text>Detach</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-disabled><span class="penna-popup-main"><span class="penna-label" data-penna-text>Rename…</span></span></div></div></div><span class="docs-note">Rest, highlighted, off</span></div>
  <div class="docs-cell" style="padding-right: 136px; max-width: 100%"><div class="penna-popup" data-penna="popover" data-ground style="min-width: 148px"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="submenu" role="menuitem" aria-haspopup="menu" data-state="hover" data-selected><span class="penna-popup-line"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Move to</span><i class="penna-popup-detail" data-icon="chevron-right"></i></span></span><div class="penna-popup penna-popup-sub" data-penna="popover" data-ground style="min-width: 112px"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-state="selected" data-selected><span class="penna-popup-main"><span class="penna-label" data-penna-text>work</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>logs</span></span></div></div></div></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Monitor activity</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div><span class="docs-note">Submenu open</span></div>
</div>
```

- Highlight: one row, from the pointer or the arrow keys.
- Off: muted, skipped by the keys.
- The menu keeps focus; the highlight shows where the keyboard is.

## Search and scopes

Long menus (like font pickers or command groups) can carry a search field at the top and a full-width segmented control below it to scope rows. Typing filters rows immediately; Tab and Shift+Tab cycle scopes without closing the menu.

```figure uses=menu,input,segmented
<div class="penna-popup" data-penna="popover" data-ground data-search data-scopes style="width: min(100%, 248px)">
  <div class="penna-popup-search"><label class="penna-input" data-penna="field"><span class="penna-input-prefix"><i data-icon="search"></i></span><input placeholder="Filter fonts" aria-label="Filter fonts" /></label></div>
  <div class="penna-popup-scopes"><div class="penna-segmented" data-penna="field" data-size="small" data-width="full" data-value="0" role="radiogroup" aria-label="Font family"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>All</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Mono</span></span><span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Sans</span></span></div></div></div>
  <div class="penna-popup-items" role="menu">
    <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem" data-state="selected" data-selected><span class="penna-popup-icon"><i data-icon="check"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>CommitMono</span></span></div>
    <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>JetBrains Mono</span></span></div>
    <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-icon"></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Inter</span></span></div>
  </div>
</div>
```

## Keys

- **Enter**, **Space**, **Down** on a dropdown's button, **Shift+F10** or **Menu** in a context region: open on the first enabled row.
- **Up**, **Down**: move the highlight, wrapping.
- **Right**, **Left**: into and out of a submenu; on a stepper row, step the value (`Enter` resets it).
- **Tab**, **Shift+Tab**: cycle scopes in a scoped menu; Left, Right and Space edit the search query when search is present.
- **Enter**: run the row and close.
- **Escape**: close every level.

## Labels and placement

A verb in sentence case, or a noun for a checked option; give the row its action and the shortcut shows in its own column. A dropdown opens `space-4` below its trigger and flips above when it would leave the window; a context menu opens at the pointer and clamps inside the window.

```figure uses=menu
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-kbd-text="cmd-t" data-penna-text></span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span><span class="penna-label penna-popup-key" data-kbd-text="cmd-w" data-penna-text></span></span></div></div></div></div><figcaption>Shortcut in its column</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab (⌘T)</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane (⌘W)</span></span></div></div></div></div><figcaption>Don't type keys into labels</figcaption></figure>
</div>
```

## Groups

Related rows together, a separator between groups.

```figure uses=menu
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div><div class="penna-popup-separator" data-kind="separator" role="separator"></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><figcaption>Part groups with a separator</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-popup" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Copy path</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><figcaption>Don't mix groups</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.dropdown_menu(build)` on a button | `pennaMenu.dropdown(trigger, items, opts)` | Opens below the button |
| `.context_menu(build)` | `pennaMenu.context(region, items, opts)` | Opens at the pointer, or top-left from keys |
| `PopupMenu::build(window, cx, f)` | `new pennaMenu.Menu(items, opts)` | Builds a standalone popup menu |
| `PopupMenu::menu(label, action)`, `.item(item)` | `{ label, key }` | Adds a row or `PopupMenuItem` |
| `PopupMenuItem::new(label)`, `.element(f)` | `{ label }` | Row item or custom row builder |
| `PopupMenuItem::icon(name)`, `.action(act)` | `{ icon }`, `{ key }` | Leading icon and action shortcut |
| `PopupMenuItem::on_click(f)`, `.checked(bool)`, `.disabled(bool)` | `penna-menu-action`, `{ checked, disabled }` | Handler, check mark, or off state |
| `PopupMenu::submenu_with_icon(..)` | `{ kind: "submenu", items }` | Nested submenu |
| `PopupMenuItem::submenu(label, menu)` | `{ kind: "submenu", items }` | Submenu from an item |
| `PopupMenu::link(label, href)` | `{ label, link }` | Opens a URL with `external-link` |
| `PopupMenuItem::stepper(label, val, down, reset, up)` | `{ kind: "stepper" }` | Steps a value in place |
| `PopupMenu::label(text)`, `.separator()` | `{ kind: "label" }`, `{ kind: "separator" }` | Section heading or divider |
| `PopupMenu::scrollable(true)`, `.max_h(h)`, `.min_w(w)`, `.max_w(w)` | `--penna-popup-max` | Scroll height and width bounds |
| `PopupMenu::search(placeholder, window, cx)`, `.set_query(q, ..)` | `{ search }` / `data-search`, `menu.filter(q)` | Search field filtering rows as you type |
| `PopupMenu::scopes(scopes, picked)`, `.set_scope(i, cx)` | `{ scopes, scope }` / `data-scopes`, `menu.setScope(i)` | Scopes via `MenuScope::new` / `MenuScope::all` |
| `PopupMenu::action_context(handle)` | | Focus handle actions dispatch to |

# Tree

Nested rows joined by guide lines: a host, its sessions, their windows. For a flat set use a [list](list.html).

## Anatomy

```figure uses=tree,panel
<div class="penna-panel" data-penna="card" data-ground data-inset="4" style="width: min(100%, 240px); padding: var(--penna-space-4)">
  <div class="penna-tree" role="tree">
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="hard-drive"></i><span class="penna-label" data-penna-text>workstation</span></div></div>
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div>
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div>
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="3" style="--depth: 3" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div>
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="3" style="--depth: 3" aria-selected="true"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div>
    <div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>notes</span></div></div>
  </div>
</div>
```

1. **Row**: a list row in `sidebar.row`, `control-md` 28 tall (follows density), `space-12` side inset, full width in a sidebar with rows touching.
2. **Indent**: 16 per level.
3. **Icon**: 14, muted, `space-8` before the label. One per kind of thing.
4. **Label**: `lead`, the thing's own name; truncates with an ellipsis when narrow.
5. **Guide**: a 1 device px hairline per level at 20 px in (`inset + half icon slot`) with 4 px end padding (omitted where it runs offscreen).

## States and behavior

A row's states match a list row's; neither native nor HTML folds levels. Selecting a row lights its parent's guide; in HTML, clicking or pressing Up and Down moves selection with roving `tabindex`. Natively, `.focused(true)` rings the focused row and padding the list `space-2` keeps the ring whole.

```figure uses=tree,panel
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-panel" data-penna="card" data-ground data-inset="4" style="width: 196px; padding: var(--penna-space-4)"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="true"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-panel" data-penna="card" data-ground data-inset="4" style="width: 196px; padding: var(--penna-space-4)"><div class="penna-tree" role="tree" data-still><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="true" data-state="focused"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Selected, focused</span></div>
  <div class="docs-cell"><div class="penna-panel" data-penna="card" data-ground data-inset="4" style="width: 196px; padding: var(--penna-space-4)"><div class="penna-tree" role="tree" data-still><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Nothing selected</span></div>
</div>
```

## Labels

The row's own name. The depth already says where it is.

```figure uses=tree
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div style="width: min(100%, 200px)"><div class="penna-tree" role="tree" data-still><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div></div></div></div><figcaption>Its own name</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div style="width: min(100%, 200px)"><div class="penna-tree" role="tree" data-still><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>main/editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>main/editor/lib.rs</span></div></div></div></div></div><figcaption>Don't repeat the path</figcaption></figure>
</div>
```

```rust
let guides = IndentGuides::new(
    depths,
    selected,
    px(16.0),
    px(20.0),
    px(4.0),
    IndentGuideColors::tree(cx),
);
uniform_list("files", rows.len(), |range, _, _| {
    range
        .map(|ix| {
            ListItem::new(("row", ix))
                .role(Role::TreeItem)
                .look(RowLook::Sidebar)
                .selected(selected == Some(ix))
        })
        .collect()
})
.p(Space::S2)
.with_decoration(guides)
```

## API (`penna::components::tree`, `penna::components::list`)

| Rust | HTML | Does |
| --- | --- | --- |
| `IndentGuides::new(depths, active, indent, offset, end, colors)` | `.penna-tree[role="tree"]`, `tree.js` | Guide layout (`--penna-tree-{row,indent,inset,offset,end}`) |
| `.with_decoration(guides)` on `uniform_list` | `.penna-tree-guide` | Draws guides under rows |
| `IndentGuideColors::tree(cx)` | `--penna-divider`, `--penna-tree-muted`, `data-active` | Guide inks |
| `ListItem::new(id).role(Role::TreeItem)` | `.penna-tree-row[role="treeitem"]` | Tree row |
| `.look(RowLook::Sidebar)` | `data-penna="sidebar.row"` | Sidebar look |
| `.pl(inset + indent × depth)` | `data-depth`, `--depth` | Level indent |
| `.selected(true)`, `.focused(true)` | `aria-selected="true"`, `data-state="focused"` | Selection (lights parent guide) and focus ring |
| `.disabled(true)`, `.on_click(handler)` | `aria-disabled="true"`, `click` | Off state and click handler |

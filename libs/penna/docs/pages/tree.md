# Tree

Nested rows joined by guide lines: a host, its sessions, their windows. For a flat set use a list.

## Anatomy

```figure uses=tree
<div style="width: 240px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="hard-drive"></i><span class="penna-label" data-penna-text>workstation</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="3" style="--depth: 3" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="3" style="--depth: 3" aria-selected="true"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>notes</span></div></div></div></div>
```

1. **Row**: a list row in the sidebar look.
2. **Indent**: 16 per level.
3. **Icon**: 14, muted. One per kind of thing.
4. **Label**: `lead`, the thing's own name.
5. **Guide**: a hairline per level.

## States

A row's states are a list row's. Selecting a row lights its parent's guide. The app marks the row the keyboard is on `.focused(true)`, and pads the list 2 on every side so the ring shows whole.

```figure uses=tree
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div style="width: 200px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="true"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div style="width: 200px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="true" data-state="focused"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Selected, focused</span></div>
  <div class="docs-cell"><div style="width: 200px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>theme.rs</span></div></div></div></div><span class="docs-note">Nothing selected</span></div>
</div>
```

## Labels

The row's own name. The depth already says where it is.

```figure uses=tree
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div style="width: 200px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>lib.rs</span></div></div></div></div></div><figcaption>Its own name</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div style="width: 200px"><div class="penna-tree" role="tree"><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="0" style="--depth: 0" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="layers"></i><span class="penna-label" data-penna-text>main</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="1" style="--depth: 1" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="app-window"></i><span class="penna-label" data-penna-text>main/editor</span></div></div><div class="penna-tree-row" data-penna="sidebar.row" role="treeitem" tabindex="-1" data-depth="2" style="--depth: 2" aria-selected="false"><div class="penna-tree-content"><i class="penna-tree-icon" data-icon="file"></i><span class="penna-label" data-penna-text>main/editor/lib.rs</span></div></div></div></div></div><figcaption>Don't repeat the path</figcaption></figure>
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
uniform_list("files", rows.len(), render_row)
    .p(Space::S2)
    .with_decoration(guides)
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `IndentGuides::new(depths, active_row, indent, left_offset, end_padding, colors)` | `.penna-tree[role="tree"]`, `tree.js` | Guides |
| `.with_decoration(guides)` on a `uniform_list` | `.penna-tree-guide` | Draws them under the rows |
| `IndentGuideColors::tree(cx)` | `--penna-divider`, `data-active` | Guide inks |
| `ListItem::new(id).role(Role::TreeItem)` | `.penna-tree-row[role="treeitem"]` | Row |
| `.look(RowLook::Sidebar)` | `data-penna="sidebar.row"` | Row look |
| `.pl(inset + indent × depth)` | `data-depth`, `--depth` | Indent |
| `.selected(true)` | `aria-selected="true"` | Selected; lights its guide |

# Breadcrumb

A horizontal path trail showing the current location inside nested workspaces, directories, or settings hierarchies.

## Slash and chevron trails

Parent segments stay muted until hovered; the current segment draws in full ink with `aria-current="page"`.

```figure uses=breadcrumb,icon
<div style="display: flex; flex-direction: column; gap: 14px; width: 100%">
  <nav class="penna-breadcrumb" aria-label="Workspace path">
    <a class="penna-breadcrumb-item" href="#slash-and-chevron-trails"><i data-icon="folder"></i><span>autistic-computer</span></a>
    <span class="penna-breadcrumb-sep" aria-hidden="true">/</span>
    <a class="penna-breadcrumb-item" href="#slash-and-chevron-trails"><span>penna</span></a>
    <span class="penna-breadcrumb-sep" aria-hidden="true">/</span>
    <span class="penna-breadcrumb-item" aria-current="page"><span>motion.rs</span></span>
  </nav>
  <nav class="penna-breadcrumb" aria-label="Settings path">
    <a class="penna-breadcrumb-item" href="#slash-and-chevron-trails"><i data-icon="settings"></i><span>Settings</span></a>
    <i class="penna-breadcrumb-sep" data-icon="chevron-right" aria-hidden="true"></i>
    <a class="penna-breadcrumb-item" href="#slash-and-chevron-trails"><span>Appearance</span></a>
    <i class="penna-breadcrumb-sep" data-icon="chevron-right" aria-hidden="true"></i>
    <span class="penna-breadcrumb-item" aria-current="page"><span>Glass</span></span>
  </nav>
</div>
```

```rust
Breadcrumb::new("file-crumb")
    .separator(BreadcrumbSeparator::Chevron)
    .item(BreadcrumbItem::new("penna").icon(IconName::Folder).on_click(open_root))
    .item(BreadcrumbItem::new("src").on_click(open_src))
    .item(BreadcrumbItem::new("motion.rs"))
```

## API (`penna::Breadcrumb`)

| Rust | HTML (`uses=breadcrumb`) | Role |
| --- | --- | --- |
| `Breadcrumb::new(id)` | `nav.penna-breadcrumb` | Path navigation trail (`Role::Navigation`) |
| `.item(BreadcrumbItem)` / `.items(...)` | `.penna-breadcrumb-item` | Parent links and final current segment |
| `.separator(Slash \| Chevron)` | `.penna-breadcrumb-sep` | Divider glyph between segments |
| `BreadcrumbItem::icon(IconName)` | Leading `<i data-icon>` | Optional 12 px cap-aligned leading glyph |
| `BreadcrumbItem::on_click(f)` | `<a>` or `<button>` | Interactive parent navigation |

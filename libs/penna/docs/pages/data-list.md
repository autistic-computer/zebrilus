# Data list

Structured key–value metadata rows for inspectors, host diagnostics, git commits, and session details.

## Horizontal and vertical layouts

Use `Horizontal` when labels are short and scannable; switch to `Vertical` inside narrow side panels or phone cards.

```figure uses=data_list,badge,button,icon
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Horizontal</span><span class="docs-lk-mono">default · 120 px label</span></div>
    <div class="penna-card" data-penna="card" data-ground style="padding: 4px 12px">
      <dl class="penna-data-list" style="--penna-data-list-label: 92px">
        <div class="penna-data-list-row"><dt class="penna-data-list-term">Host</dt><dd class="penna-data-list-val" data-mono>ubuntu.local</dd></div>
        <div class="penna-data-list-row"><dt class="penna-data-list-term">Transport</dt><dd class="penna-data-list-val"><span>QUIC</span><span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text">0.9 ms</span></span></dd></div>
        <div class="penna-data-list-row"><dt class="penna-data-list-term">Commit</dt><dd class="penna-data-list-val" data-mono>e5c2c7e</dd></div>
      </dl>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Vertical</span><span class="docs-lk-mono">narrow inspector</span></div>
    <div class="penna-card" data-penna="card" data-ground style="padding: 4px 12px">
      <dl class="penna-data-list" data-layout="vertical">
        <div class="penna-data-list-row"><dt class="penna-data-list-term">Displays</dt><dd class="penna-data-list-val">DP-1 27″ 4K · HDMI-1 24″</dd></div>
        <div class="penna-data-list-row"><dt class="penna-data-list-term">Binary</dt><dd class="penna-data-list-val" data-mono>/usr/local/bin/nidus</dd></div>
      </dl>
    </div>
  </div>
</div>
```

```rust
DataList::new()
    .item(DataListItem::new("Host", "ubuntu.local").mono())
    .item(DataListItem::new("Transport", "QUIC").trailing(Badge::new("0.9 ms").tone(Tone::Success)))
    .item(DataListItem::new("Commit", "e5c2c7e").mono())
```

## API (`penna::DataList`)

| Rust | HTML (`uses=data_list`) | Role |
| --- | --- | --- |
| `DataList::new()` | `dl.penna-data-list` | Key–value metadata list |
| `.layout(Horizontal \| Vertical)` | `data-layout="vertical"` | Two-column aligned vs stacked rows |
| `.label_width(Length)` | `--penna-data-list-label` | Fixed label column width (default 120 px) |
| `.dividers(bool)` | row borders | `RuleLook::List` hairlines between rows |
| `DataListItem::mono()` / `.trailing(el)` | `dd[data-mono]` | Monospace tabular values and inline badges/actions |

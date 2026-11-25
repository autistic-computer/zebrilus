# Table

A dense, sortable data table for sessions, hosts, processes, and parity matrices. Numeric columns align right with `tabular-nums`, row heights follow active density, and selection uses one neutral highlight.

## Sortable columns and row selection

Click any column header to cycle ascending → descending → unsorted; click a row or use `↑` / `↓` to move selection.

```figure uses=table,badge,icon
<div class="penna-table-wrap" data-penna="card" data-ground>
  <table class="penna-table">
    <thead>
      <tr>
        <th data-sort aria-sort="none"><span class="penna-table-th-inner"><span>Session</span><i data-icon="chevron-down" data-sort-icon></i></span></th>
        <th data-sort aria-sort="none"><span class="penna-table-th-inner"><span>Host</span><i data-icon="chevron-down" data-sort-icon></i></span></th>
        <th data-sort data-align="right" aria-sort="none"><span class="penna-table-th-inner"><span>Panes</span><i data-icon="chevron-down" data-sort-icon></i></span></th>
        <th data-sort data-align="right" aria-sort="none"><span class="penna-table-th-inner"><span>Latency</span><i data-icon="chevron-down" data-sort-icon></i></span></th>
        <th><span>State</span></th>
      </tr>
    </thead>
    <tbody>
      <tr tabindex="0" data-state="selected" aria-selected="true"><td>work</td><td>macbook</td><td data-align="right" data-mono>3</td><td data-align="right" data-mono>1.2 ms</td><td><span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text">Running</span></span></td></tr>
      <tr tabindex="0" aria-selected="false"><td>agents</td><td>ubuntu</td><td data-align="right" data-mono>11</td><td data-align="right" data-mono>0.9 ms</td><td><span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text">Running</span></span></td></tr>
      <tr tabindex="0" aria-selected="false"><td>build</td><td>alienware</td><td data-align="right" data-mono>2</td><td data-align="right" data-mono>12.4 ms</td><td><span class="penna-badge" data-penna="badge" data-tone="warning"><span class="badge-text">Compiling</span></span></td></tr>
      <tr tabindex="0" aria-selected="false"><td>logs</td><td>thinkpad</td><td data-align="right" data-mono>4</td><td data-align="right" data-mono>4.2 ms</td><td><span class="penna-badge" data-penna="badge"><span class="badge-text">Idle</span></span></td></tr>
    </tbody>
  </table>
</div>
```

```rust
Table::new(
    "sessions-table",
    vec![
        TableColumn::new("Session"),
        TableColumn::new("Host"),
        TableColumn::new("Panes").align(ColumnAlign::Right).mono().width(px(72.0)),
        TableColumn::new("Latency").align(ColumnAlign::Right).mono().width(px(84.0)),
    ],
    rows,
)
.on_select(|index, _, _| focus_session(index))
```

## API (`penna::Table`)

| Rust | HTML (`uses=table`) | Role |
| --- | --- | --- |
| `Table::new(id, columns, rows)` | `.penna-table-wrap > table.penna-table` | Density-aware sortable data table |
| `TableColumn::align(Right)` / `.mono()` | `data-align="right"` / `data-mono` | Numeric right alignment with tabular digits |
| `TableColumn::sortable(bool)` | `th[data-sort]` | Cycles `none → ascending → descending` |
| `.on_select(f)` | `tr[data-state="selected"]` | Row click and `↑` / `↓` keyboard selection |
| `SortState` | `window.pennaTable` | Unit-aware numeric & case-insensitive sort |

# Combobox

A filterable token picker for multi-facet filter bars (`host:ubuntu`, `status:open`) and searchable single-select fields.

## Token filter bar

Type to filter options, press `Enter` or click to toggle tokens, press `Backspace` on an empty query to pop the last token, or `Escape` to close.

```figure uses=combobox,icon
<div style="width: 100%; max-width: 440px; padding-bottom: 180px">
  <div class="penna-combobox" data-multiple="true">
    <div class="penna-combobox-box" data-penna="field" data-ground>
      <span class="penna-combobox-tokens"></span>
      <input class="penna-combobox-input" type="text" role="combobox" aria-autocomplete="list" aria-expanded="false" aria-label="Filter sessions" placeholder="Filter by host or status…" />
    </div>
    <div class="penna-combobox-menu" data-penna="popover" data-ground role="listbox" hidden>
      <button class="penna-combobox-opt" data-penna="menu-item" type="button" role="option" data-value="host:ubuntu" data-label="host:ubuntu" aria-selected="true"><i class="penna-combobox-check" data-icon="check"></i><span>host:ubuntu</span><span class="penna-combobox-opt-detail">Linux · 0.9 ms</span></button>
      <button class="penna-combobox-opt" data-penna="menu-item" type="button" role="option" data-value="host:macbook" data-label="host:macbook" aria-selected="false"><i class="penna-combobox-check" data-icon="check"></i><span>host:macbook</span><span class="penna-combobox-opt-detail">macOS · Primary</span></button>
      <button class="penna-combobox-opt" data-penna="menu-item" type="button" role="option" data-value="status:running" data-label="status:running" aria-selected="true"><i class="penna-combobox-check" data-icon="check"></i><span>status:running</span><span class="penna-combobox-opt-detail">Active panes</span></button>
      <button class="penna-combobox-opt" data-penna="menu-item" type="button" role="option" data-value="status:idle" data-label="status:idle" aria-selected="false"><i class="penna-combobox-check" data-icon="check"></i><span>status:idle</span><span class="penna-combobox-opt-detail">Detached</span></button>
      <button class="penna-combobox-opt" data-penna="menu-item" type="button" role="option" data-value="branch:main" data-label="branch:main" aria-selected="false"><i class="penna-combobox-check" data-icon="check"></i><span>branch:main</span><span class="penna-combobox-opt-detail">Git branch</span></button>
      <div class="penna-combobox-empty" hidden>No matching filters</div>
    </div>
  </div>
</div>
```

```rust
Combobox::new(
    "session-filter",
    vec![
        ComboboxOption::new("host:ubuntu", "host:ubuntu").detail("Linux"),
        ComboboxOption::new("status:running", "status:running").detail("Active"),
    ],
)
.selected(["host:ubuntu".into()])
.on_change(|values, _, _| apply_filters(values))
```

## API (`penna::Combobox`)

| Rust | HTML (`uses=combobox`) | Role |
| --- | --- | --- |
| `Combobox::new(id, options)` | `.penna-combobox` | Filterable token field + popup listbox |
| `.multiple(bool)` | `data-multiple="true \| false"` | Multi-token chips vs single selection |
| `.selected(values)` | `aria-selected="true"` | Initially selected option values |
| `.on_change(f)` | `change` event (`{ values }`) | Fires whenever selected tokens change |
| `ComboboxState` | `window.pennaCombobox` | Query filtering, toggle, and `remove_last` |

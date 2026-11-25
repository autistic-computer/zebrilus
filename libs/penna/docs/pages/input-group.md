# Input group

Combines a field with leading or trailing addons (protocol prefixes, port suffixes, icons, or inline buttons) inside one unified focus ring.

## Prefix, suffix, and inline actions

```figure uses=input_group,button,icon,kbd
<div style="display: flex; flex-direction: column; gap: 12px; width: 100%; max-width: 440px">
  <div class="penna-input-group" data-penna="field" data-ground>
    <span class="penna-input-addon" data-mono>zz://</span>
    <input type="text" value="studio.tailnet" aria-label="Host address" />
    <span class="penna-input-addon" data-mono>:7443</span>
  </div>
  <div class="penna-input-group" data-penna="field" data-ground>
    <span class="penna-input-addon"><i data-icon="search"></i></span>
    <input type="search" placeholder="Filter symbols…" aria-label="Filter symbols" />
    <button class="penna-button" data-penna="button.quiet" data-size="xsmall" type="button"><span class="penna-label" data-penna-text>Go</span></button>
  </div>
</div>
```

```rust
InputGroup::new()
    .prefix_text("zz://")
    .suffix_text(":7443")
    .child(host_input)
```

## API (`penna::InputGroup`)

| Rust | HTML (`uses=input_group`) | Role |
| --- | --- | --- |
| `InputGroup::new()` | `.penna-input-group[data-penna="field"]` | Unified field shell with `:focus-within` ring |
| `.prefix(el)` / `.prefix_text(str)` | `.penna-input-addon` (first child) | Leading static or icon addon |
| `.suffix(el)` / `.suffix_text(str)` | `.penna-input-addon` (last child) | Trailing unit, keycap, or action control |
| `.on(ground)` | `data-ground` | Ground propagation inside panels or cards |

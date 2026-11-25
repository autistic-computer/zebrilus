# Split

A resizable two-pane container with a 1:1 pointer-tracked sash. Pointer drags move without easing; double-clicking the sash resets to the initial ratio, and arrow keys nudge the split when focused.

## Resizable panes

Drag the center divider left or right, focus it and press `←` / `→` (`Shift` for 10% steps), or double-click to reset to 42%.

```figure uses=split,label,panel
<div class="penna-split-panes" data-ratio="42" data-min="22" data-max="78" style="height: 204px; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); background: var(--penna-bg)">
  <div class="penna-split-first" style="padding: 14px; background: var(--penna-panel)">
    <div style="display: flex; flex-direction: column; gap: 6px">
      <span class="penna-type" data-type="small" data-strong>Explorer</span>
      <span class="penna-type" data-type="caption" data-muted>src/components/split/mod.rs</span>
      <span class="penna-type" data-type="caption" data-muted>src/blocks/table/mod.rs</span>
      <span class="penna-type" data-type="caption" data-muted>src/blocks/diff/mod.rs</span>
    </div>
  </div>
  <div class="penna-split-sash" role="separator" tabindex="0" aria-orientation="vertical" aria-label="Resize panes" aria-valuemin="22" aria-valuemax="78" aria-valuenow="42"></div>
  <div class="penna-split-second" style="padding: 14px">
    <div style="display: flex; flex-direction: column; gap: 6px">
      <span class="penna-type" data-type="small" data-strong>Workspace pane</span>
      <span class="penna-type" data-type="small" data-muted>Drag the sash 1:1 with zero easing. Double-click the sash to return to 42%.</span>
    </div>
  </div>
</div>
```

```rust
Split::horizontal("workspace-split")
    .ratio(0.42)
    .limits(0.22, 0.78)
    .first(explorer_pane)
    .second(editor_pane)
```

## API (`penna::Split`)

| Rust | HTML (`uses=split`) | Role |
| --- | --- | --- |
| `Split::horizontal(id)` / `Split::vertical(id)` | `.penna-split-panes[data-axis="vertical"]` | Two-pane resizable host |
| `.ratio(0.0..1.0)` | `data-ratio="42"` | Initial first-pane percentage (also double-click reset target) |
| `.limits(min, max)` | `data-min` / `data-max` | Clamped ratio bounds |
| `.first(el)` / `.second(el)` | `.penna-split-first` / `.penna-split-second` | Pane contents |
| `.on_resize(f)` | `resize` event (`{ ratio }`) | Emits live ratio during drag or keyboard nudge |

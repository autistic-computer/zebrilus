# Color picker

A swatch trigger that opens a 2D saturation-brightness picker, hue slider, hex input, and clear button for optional color overrides. When cleared (`None`), the swatch shows the inherited fallback color.

## Override, inherited and bare swatches

Click any swatch below to open the live popover. Drag in the 2D field, adjust the hue rail, type a hex code, or clear back to the inherited color.

```figure uses=color_picker
<div style="display: flex; flex-direction: column; gap: 12px; width: 100%; max-width: 320px; min-height: 296px">
  <div class="penna-card" data-penna="card" data-ground style="display: flex; flex-direction: column; padding: 12px; gap: 10px">
    <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px">
      <span data-penna-text>Accent (override)</span>
      <div class="penna-color-picker" data-value="#3183ff" data-inherited="#a366f7" data-label="Accent">
        <button class="penna-button" data-penna="button.quiet" data-flat data-compact type="button" aria-label="Accent color" data-id="docs-cp-accent">
          <span class="penna-color-picker-swatch"></span>
        </button>
      </div>
    </div>
    <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px">
      <span data-penna-text>Selection (inherited)</span>
      <div class="penna-color-picker" data-inherited="#3fbf74" data-label="Selection">
        <button class="penna-button" data-penna="button.quiet" data-flat data-compact type="button" aria-label="Selection color" data-id="docs-cp-selection">
          <span class="penna-color-picker-swatch"></span>
        </button>
      </div>
    </div>
    <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px">
      <span data-penna-text>Bare swatch (18 px)</span>
      <div class="penna-color-picker" data-value="#f0a14a" data-inherited="#3183ff" data-label="Tint">
        <button class="penna-button" data-penna="button.quiet" data-flat data-swatch style="--penna-swatch-size: 18px" type="button" aria-label="Bare swatch" data-id="docs-cp-bare">
          <span class="penna-color-picker-swatch"></span>
        </button>
      </div>
    </div>
  </div>
</div>
```

```rust
let state = cx.new(|cx| ColorPickerState::new(Some(accent), window, cx));

ColorPicker::new(&state, inherited)
    .id("accent-picker")
    .label("Accent")

ColorPicker::new(&state, inherited)
    .id("swatch-picker")
    .swatch(px(18.0))
```

## Phone behavior

On coarse pointers (`CoarsePointer`), `.presets(chosen, color, presets)` turns the trigger into a full-width field displaying the hex value beside a 24 px swatch and opens the picker inside a [Sheet](sheet.html) with the preset row above the 2D field. The web twin currently renders the desktop popover only.

## API

| Rust (`penna::blocks::color_picker`) | HTML (`uses=color_picker`) | Role |
| --- | --- | --- |
| `ColorPickerState::new(color, w, cx)` | `.penna-color-picker[data-value][data-inherited]` | Optional color override state |
| `ColorPicker::new(&state, inherited)` | `button.penna-button[data-compact]` | Compact swatch popover trigger |
| `.swatch(Pixels)` | `button[data-swatch]` (`--penna-swatch-size`) | Bare circular/rounded swatch |
| `.label(...)` | `data-label="..."` | Popover title and accessible name |
| `.presets(chosen, color, el)` | — | Phone field trigger + sheet presets |
| `ColorPickerEvent::Change(Option<Hsla>)` | `change` event (`detail: "#rrggbb" \| null`) | Emitted on color pick or clear |

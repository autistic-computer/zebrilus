# Appearance

The standard preferences page for Penna's look, sizing, and accessibility knobs. Apps assemble the page from `appearance_page_items()` and `kit_row(...)`, inserting app-specific entries via `AppearancePageItem::App(T)` so every app exposes the same controls in the same order.

## Look controls

Three groups render in fixed order without folding: **Look** (mode, accent, tint hue and strength, corner radius, controls, glass, UI font, mono font), **Size** (text size, density), and **Accessibility** (increase contrast, motion, pointing hand). On a phone, Corner radius, Text size, and Density are omitted. Try the live controls below.

```figure uses=appearance,color_picker,select
<div class="appearance-frame" style="width: 100%; max-width: 640px">
  <div class="appearance-column">
    <div class="appearance-group" style="padding-top: 0"><span class="appearance-text" data-penna-text>Look</span></div>
    <div class="appearance-run" data-penna="settings.card" data-ground>
      <div class="appearance-entry">
        <div class="appearance-row">
          <span class="appearance-copy"><span class="appearance-title" data-penna-text>Mode</span></span>
          <div class="penna-segmented appearance-mode" data-penna="field" data-value="2" role="radiogroup" aria-label="Mode">
            <div class="penna-segments" data-penna="button.quiet" data-state="current">
              <span class="penna-segment-pill"></span>
              <span class="penna-segment" role="radio"><span class="penna-label" data-penna-text>System</span></span>
              <span class="penna-segment" role="radio"><span class="penna-label" data-penna-text>Light</span></span>
              <span class="penna-segment" role="radio"><span class="penna-label" data-penna-text>Dark</span></span>
            </div>
          </div>
        </div>
      </div>
      <div class="appearance-rule"><div data-penna="rule.list"></div></div>
      <div class="appearance-entry">
        <div class="appearance-row">
          <span class="appearance-copy"><span class="appearance-title" data-penna-text>Accent</span></span>
          <div class="appearance-swatches">
            <span class="appearance-ring"><span class="appearance-swatch" role="button" tabindex="0" aria-label="Blue" data-accent="blue" data-light="#0a64dd" data-dark="#3183ff"></span></span>
            <span class="appearance-ring"><span class="appearance-swatch" role="button" tabindex="0" aria-label="Purple" data-accent="purple" data-light="#8549d1" data-dark="#a366f7"></span></span>
            <span class="appearance-ring"><span class="appearance-swatch" role="button" tabindex="0" aria-label="Pink" data-accent="pink" data-light="#cb2c70" data-dark="#ec5390"></span></span>
            <span class="appearance-ring"><span class="appearance-swatch" role="button" tabindex="0" aria-label="Green" data-accent="green" data-light="#007b2c" data-dark="#009c3a"></span></span>
            <span class="appearance-ring"><div class="penna-color-picker" data-label="Accent"><button class="penna-button" data-penna="button.quiet" data-flat data-swatch style="--penna-swatch-size: 18px" type="button" aria-label="Custom accent" data-id="docs-app-accent"><span class="penna-color-picker-swatch"></span></button></div></span>
          </div>
        </div>
      </div>
      <div class="appearance-rule"><div data-penna="rule.list"></div></div>
      <div class="appearance-entry">
        <div class="appearance-row">
          <span class="appearance-copy"><span class="appearance-title" data-penna-text>Corner radius</span></span>
          <div class="appearance-slider-row">
            <div class="appearance-slider" role="slider" tabindex="0" aria-label="Corner radius" data-min="0" data-max="1" data-step="0.01" data-value="theme-radius" data-format="percent"><span class="appearance-rail"><span class="appearance-bar"><span class="appearance-fill"></span></span><span class="appearance-knob"></span></span></div>
            <div class="appearance-trailing"><span class="appearance-shown" data-penna-text></span></div>
          </div>
        </div>
      </div>
      <div class="appearance-rule"><div data-penna="rule.list"></div></div>
      <div class="appearance-entry">
        <div class="appearance-row">
          <span class="appearance-copy"><span class="appearance-title" data-penna-text>Glass</span></span>
          <button class="penna-toggle appearance-look-glass" role="switch" aria-checked="true" aria-label="Glass" type="button"></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

```rust
let controls = AppearanceControls::new(
    &appearance,
    |change, window, cx| apply_appearance_change(change, window, cx),
    window,
    cx,
);

let items = appearance_page_items::<()>();
appearance_page("Appearance", items.clone(), move |index, window, cx| {
    let item = items[index];
    kit_row(item, &appearance, &controls, false, window, cx)
        .map(|entry| page_row(entry.into_any_element(), run_position(&items, index, AppearancePageItem::is_entry).ends_run()))
})
```

## Individual controls and previews

Every row helper is also exported standalone (`theme_mode_control`, `accent_swatches`, `corner_radius_slider`, `density_control`, `motion_control`, `ui_font_select`, `mono_font_select`, `picker_tile`, `theme_preview`, `palette_preview`) so apps can embed a single control in a custom sheet or inspector without rebuilding the full page.

## API

| Rust (`penna::blocks::appearance`) | HTML (`uses=appearance,color_picker,select`) | Role |
| --- | --- | --- |
| `AppearanceControls::new(app, on_change, w, cx)` | `.appearance-frame` | State holder and live knob wiring |
| `appearance_page_items::<T>()` | `.appearance-column` | Canonical 3-group item sequence |
| `appearance_page(title, items, render)` | `.appearance-heading` + `.appearance-group` | Virtualized settings column |
| `kit_row(item, app, controls, stacked, w, cx)` | `.appearance-run[data-penna="settings.card"] > .appearance-entry` | Standard row for each kit knob |
| `accent_swatches(...)` | `.appearance-swatches > .appearance-ring > .appearance-swatch` | Preset swatches + custom color picker |
| `corner_radius_slider(...)` / `percent_slider(...)` | `.appearance-slider-row > .appearance-slider` | Continuous knob slider with readout |
| `AppearanceChange` | — | Typed delta enum emitted on edit |

# Switch

Turns a setting on or off, and the change applies at once. For one of a few modes use a segmented control.

## Anatomy

```figure uses=switch
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-toggle" data-size="xlarge" aria-checked="false" aria-label="Off"></button><span class="docs-note">Off</span></div>
  <div class="docs-cell"><button class="penna-toggle" data-size="xlarge" aria-checked="true" aria-label="On"></button><span class="docs-note">On</span></div>
</div>
```

1. **Track**: neutral when off, the accent when on, under a white knob (the ink while the tint is the ink).
2. **Thumb**: slides to the end when on.

The setting's name is the row title beside it.

```rust
Switch::new("share-clipboard").checked(on).on_click(toggle)
```

## Sizes

```figure uses=switch
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Track</span><span class="docs-head">Thumb</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>24 × 14</code></span><span><code>10</code></span><span><button class="penna-toggle" data-size="xsmall" aria-checked="true" aria-label="Xsmall"></button></span>
  <span>small</span><span><code>28 × 16</code></span><span><code>12</code></span><span><button class="penna-toggle" data-size="small" aria-checked="true" aria-label="Small"></button></span>
  <span>medium</span><span><code>36 × 20</code></span><span><code>16</code></span><span><button class="penna-toggle" aria-checked="true" aria-label="Medium"></button></span>
  <span>large</span><span><code>40 × 22</code></span><span><code>18</code></span><span><button class="penna-toggle" data-size="large" aria-checked="true" aria-label="Large"></button></span>
  <span>xlarge, touch</span><span><code>44 × 26</code></span><span><code>22</code></span><span><button class="penna-toggle" data-size="xlarge" aria-checked="true" aria-label="Xlarge"></button></span>
</div>
```

Medium is the default and stays put with density. Under touch, medium and up draw at 44 × 26.

## States

```figure uses=switch
<div class="penna-stack" data-gap="group">
  <div class="docs-show" data-gap="wide">
    <div class="docs-cell"><button class="penna-toggle" aria-checked="false" aria-label="Off"></button><span class="docs-note">Off</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="false" data-state="hover" aria-label="Hover"></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="false" data-state="hover active" aria-label="Pressed"></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="false" data-state="focused" aria-label="Focused"></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="false" disabled aria-label="Disabled"></button><span class="docs-note">Disabled</span></div>
  </div>
  <div class="docs-show" data-gap="wide">
    <div class="docs-cell"><button class="penna-toggle" aria-checked="true" aria-label="On"></button><span class="docs-note">On</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="true" data-state="hover" aria-label="Hover"></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="true" data-state="hover active" aria-label="Pressed"></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="true" data-state="focused" aria-label="Focused"></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-toggle" aria-checked="true" disabled aria-label="Disabled"></button><span class="docs-note">Disabled</span></div>
  </div>
</div>
```

- Hover and pressed: the track a step deeper, then another while held. The thumb stays.
- Focused: a ring outside the track, one ring width off it when on, from the keyboard only. Space or Enter flips it.
- Disabled: half strength, presses ignored, out of the focus order.
- The thumb holds still with reduced motion.

## Labels

Name the setting, never its state. One switch per row, at its end.

```figure uses=switch
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-settings" style="padding: 0; width: 100%"><div><div data-penna="settings.card"><div class="penna-setting"><div class="penna-copy">Share clipboard<small>Copy here, paste there.</small></div><button class="penna-toggle" aria-checked="true" aria-label="Share clipboard"></button></div></div></div></div></div><figcaption>Name what it turns on</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-settings" style="padding: 0; width: 100%"><div><div data-penna="settings.card"><div class="penna-setting"><div class="penna-copy">On<small>Clipboard sharing is on.</small></div><button class="penna-toggle" aria-checked="true" aria-label="On"></button></div></div></div></div></div><figcaption>Don't label it with its state</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Switch::new(id)` | `button.penna-toggle` | The switch |
| `.checked(true)` | `aria-checked="true"` | On |
| `.on_click(handler)` | `change` | Gets the new value |
| `.label(text)` | `aria-label` | Name for screen readers |
| `.tooltip(text)` | | Tooltip |
| `.xsmall()` … `.touch()` | `data-size` | Size; medium by default |
| `.disabled(true)` | `disabled` | Off |

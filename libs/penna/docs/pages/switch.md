# Switch

Turns a setting on or off, and the change applies at once. For one of a few modes use a [segmented control](segmented.html); to start an action, a [button](button.html).

## Anatomy

```figure uses=switch
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-toggle" data-size="xlarge" type="button" aria-checked="false" aria-label="Off"></button><span class="docs-note">Off</span></div>
  <div class="docs-cell"><button class="penna-toggle" data-size="xlarge" type="button" aria-checked="true" aria-label="On"></button><span class="docs-note">On</span></div>
</div>
```

1. **Track**: neutral control fill when off, the accent when on, with a transparent `space-2` border around the thumb.
2. **Thumb**: `--penna-knob` (the lighter of background and foreground) when off, `--penna-on-accent` when on, under `shadow-contact`; slides across in 150ms.

The setting's name is the row title beside it.

```rust
Switch::new("share-clipboard")
    .checked(on)
    .on_click(|&next, _, cx| set_sharing(next, cx))
```

## Sizes

```figure uses=switch
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Track</span><span class="docs-head">Thumb</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>24 × 14</code></span><span><code>10</code></span><span><button class="penna-toggle" data-size="xsmall" type="button" aria-checked="true" aria-label="Xsmall"></button></span>
  <span>small</span><span><code>28 × 16</code></span><span><code>12</code></span><span><button class="penna-toggle" data-size="small" type="button" aria-checked="true" aria-label="Small"></button></span>
  <span>medium</span><span><code>36 × 20</code></span><span><code>16</code></span><span><button class="penna-toggle" type="button" aria-checked="true" aria-label="Medium"></button></span>
  <span>large</span><span><code>40 × 22</code></span><span><code>18</code></span><span><button class="penna-toggle" data-size="large" type="button" aria-checked="true" aria-label="Large"></button></span>
  <span>xlarge, touch, touch-large</span><span><code>44 × 26</code></span><span><code>22</code></span><span><button class="penna-toggle" data-size="xlarge" type="button" aria-checked="true" aria-label="Xlarge"></button></span>
</div>
```

Medium is the default and stays put with density. Under a coarse pointer, medium and up draw at 44 × 26 with a 44 × 44 touch target.

## States

```figure uses=switch
<div class="penna-stack" data-gap="group">
  <div class="docs-show" data-gap="wide">
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="false" aria-label="Off"></button><span class="docs-note">Off</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="false" data-state="hover" aria-label="Hover"></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="false" data-state="hover active" aria-label="Pressed"></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="false" data-state="focused" aria-label="Focused"></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="false" disabled aria-label="Disabled"></button><span class="docs-note">Disabled</span></div>
  </div>
  <div class="docs-show" data-gap="wide">
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="true" aria-label="On"></button><span class="docs-note">On</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="true" data-state="hover" aria-label="Hover"></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="true" data-state="hover active" aria-label="Pressed"></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="true" data-state="focused" aria-label="Focused"></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-toggle" type="button" aria-checked="true" disabled aria-label="Disabled"></button><span class="docs-note">Disabled</span></div>
  </div>
</div>
```

- Hover and pressed: the track steps deeper, then another step while held; flips on release, and a click never focuses it.
- Focused: a ring outside the track (standing off by one ring width when the accent matches the ring's color), from the keyboard only (a tab stop only with `on_click`). Space or Enter flips it.
- Disabled: half strength, presses ignored, out of the focus order.
- Reduced motion: the thumb jumps without sliding.

## Labels

Name the setting, never its state. One switch per row, at its end.

```figure uses=switch,settings
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-settings" style="padding: 0; width: 100%"><div><div data-penna="settings.card" data-ground><div class="penna-setting"><div class="penna-copy">Share clipboard<small>Copy here, paste there.</small></div><button class="penna-toggle" type="button" aria-checked="true" aria-label="Share clipboard"></button></div></div></div></div></div><figcaption>Name what it turns on</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-settings" style="padding: 0; width: 100%"><div><div data-penna="settings.card" data-ground><div class="penna-setting"><div class="penna-copy">On<small>Clipboard sharing is on.</small></div><button class="penna-toggle" type="button" aria-checked="true" aria-label="On"></button></div></div></div></div></div><figcaption>Don't label it with its state</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Switch::new(id)` | `button.penna-toggle[type="button"][role="switch"]` | The switch |
| `.checked(true)` | `aria-checked="true"` | On |
| `.on_click(handler)` | `change` | Gets `&bool` with the new value |
| `.label(text)` | `aria-label` | Name for screen readers |
| `.tooltip(text)` | `data-tooltip` | Tooltip |
| `.xsmall()` … `.touch_large()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.disabled(true)` | `disabled` | Off |

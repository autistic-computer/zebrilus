# Slider

Sets a value on a range by dragging or with the arrow keys. For an exact number use a number input.

## Kinds

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 200px"><div class="penna-slider" data-value="40" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Continuous</span></div>
  <div class="docs-cell" style="width: 300px"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2"></div><span class="docs-note">Stepped</span></div>
</div>
```

- **Continuous**: any value on a range. A step snaps it.
- **Stepped**: one of a few named steps, in order.

```rust
DiscreteSlider::new("scrollback", "Scrollback", steps, picked, pick)
```

## Anatomy

1. **Rail**: filled with the highlight up to the value.
2. **Knob**: sits on the value.
3. **Label**: stepped only. Names the setting, muted.
4. **Pills**: stepped only. One per step, filled up to the pick.
5. **Value**: stepped only. The picked step's name.

## Sizes

```figure uses=slider
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Box</span><span class="docs-head">Rail, knob</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>4, 12</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" data-size="xsmall" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
  <span>small</span><span><code>24</code></span><span><code>4, 14</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" data-size="small" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
  <span>medium</span><span><code>28</code></span><span><code>6, 16</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
  <span>large</span><span><code>32</code></span><span><code>6, 18</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" data-size="large" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
  <span>xlarge</span><span><code>40</code></span><span><code>6, 22</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" data-size="xlarge" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
  <span>touch</span><span><code>44</code></span><span><code>6, 28</code></span><span style="width: 200px"><div class="penna-slider" data-value="40" data-size="touch" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div></span>
</div>
```

Medium is the default and follows density. Stepped pills take the same box and rail; their text is caption at xsmall and small. Under touch the continuous slider is 44 tall with a 28 knob at any size.

## States

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 180px"><div class="penna-slider" data-value="40" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: 180px"><div class="penna-slider" data-value="40" data-focused aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: 180px"><div class="penna-slider" data-value="40" aria-disabled="true" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Disabled</span></div>
</div>
```

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 290px"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2" data-focused></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: 290px"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2" aria-disabled="true"></div><span class="docs-note">Disabled</span></div>
</div>
```

- Focused: a ring outside the picked pill or the knob, from the keyboard only. A press focuses it without one.
- Hover: a stepped pill brightens.
- Keys: arrows step, Home and End go to the ends. Shift steps ten on a continuous slider.
- Disabled: rail, pills and knob at half strength, a stepped label and value faint. Presses and keys do nothing, and it leaves the focus order.

## Values

Show the value people know, with its unit.

```figure uses=slider
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2"></div></div><figcaption>Show the value with its unit</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-steps" data-label="Scrollback" data-steps="1, 2, 3, 4, 5" data-value="2"></div></div><figcaption>Don't show the step's number</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Slider::new(..)` | `.penna-slider`, `data-min`, `data-max`, `data-value` | Continuous |
| `.step(size)` | `data-step` | Snaps the value |
| `.display(format)` | | Formats the value |
| `.commit_on_release()` | | Changes once, on release |
| `.track(colors)` | | Paints a gradient rail |
| `DiscreteSlider::new(..)` | `.penna-steps`, `data-label`, `data-steps`, `data-value` | Stepped |
| `.xsmall()` … `.touch()` | `data-size` | Size, both kinds; medium by default |
| `.disabled(true)` | `aria-disabled="true"` | Off, both kinds |
| `on_change` | `change` | Gets the new value |

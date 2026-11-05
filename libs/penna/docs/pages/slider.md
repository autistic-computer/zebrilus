# Slider

Sets a value on a range by dragging or with the arrow keys. For an exact number use a number input.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Neither kind has a disabled state or sizes. On the web the continuous slider is a still picture.</span></div></div></div>
```

## Kinds

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 200px"><div class="penna-slider" style="--penna-value: 40%"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Continuous</span></div>
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

One size: medium height, which follows density. Under touch the continuous slider is 44 tall with a 28 knob.

## States

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 300px"><div class="penna-steps" data-label="Opacity" data-steps="0%, 10%, 20%, 30%, 40%, 50%, 60%, 70%, 80%, 90%, 100%" data-value="7"></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: 300px"><div class="penna-steps" data-label="Opacity" data-steps="0%, 10%, 20%, 30%, 40%, 50%, 60%, 70%, 80%, 90%, 100%" data-value="7" data-focused></div><span class="docs-note">Focused</span></div>
</div>
```

- Focused: a ring outside the picked pill or the knob, from the keyboard only. A press focuses it without one.
- Hover: a stepped pill brightens.
- Keys: arrows step, Home and End go to the ends. Shift steps ten on a continuous slider.

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
| `Slider::new(..)` | `.penna-slider`, `--penna-value` | Continuous |
| `.step(size)` | | Snaps the value |
| `.display(format)` | | Formats the value |
| `.commit_on_release()` | | Changes once, on release |
| `.track(colors)` | | Paints a gradient rail |
| `DiscreteSlider::new(..)` | `.penna-steps`, `data-label`, `data-steps`, `data-value` | Stepped |
| `on_change` | `change` | Gets the new value |

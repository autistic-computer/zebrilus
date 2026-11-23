# Slider

Sets a value on a range by dragging or with the arrow keys. For an exact number use a [number input](input.html); for a few named choices that read better side by side, a [segmented control](segmented.html).

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">Continuous and stepped sliders match. StepRail is drawn in ala only: no HTML twin, storybook section or docs figure yet.</span>
    </div>
  </div>
</div>
```

## Kinds

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 200px)"><div class="penna-slider" data-value="40" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Continuous</span></div>
  <div class="docs-cell" style="width: min(100%, 290px)"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2"></div><span class="docs-note">Stepped</span></div>
</div>
```

- **Continuous**: any value on a range; `.step()` snaps it. Under a coarse pointer (unless `.inline()`), tapping a 72 px preview field opens a centered modal with a full slider.
- **Stepped**: one of a few named steps in order, each pill carrying a tooltip with its step name.
- **Rail**: `StepRail`, a 26 px head (strong `caption` label and `small` value following drag) over a 26 px rail with 4 px step dots and a 30 px knob (such as Effort in a popover). `.open_start()` draws the first step as a 6 px open ring meaning "not set"; commits once on release.

```rust
Slider::new("volume", "Volume", level, 0.0..=100.0, set_level).step(5.0)
DiscreteSlider::new("scrollback", "Scrollback", steps, picked, pick)
```

## Anatomy

1. **Box**: the hit area, inset by half the knob on each side so the knob stays inside.
2. **Rail**: filled with the accent up to the value.
3. **Knob**: sits on the value.
4. **Stepped parts**: muted label, pills up to 24 wide spaced `space-4` apart (filled up to the pick), and a 52 px value column naming the picked step.

In settings, continuous and stepped sliders sit at the trailing end of a row; `StepRail` spans the full width of its popover or section.

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

Medium is the default and follows density. Stepped pills share the box and rail heights (`caption` text at xsmall and small). Under touch, the continuous slider draws 44 tall with a 28 knob at any size.

## States

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 180px)"><div class="penna-slider" data-value="40" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: min(100%, 180px)"><div class="penna-slider" data-value="40" data-focused aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: min(100%, 180px)"><div class="penna-slider" data-value="40" aria-disabled="true" aria-label="Volume"><div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div></div><span class="docs-note">Disabled</span></div>
</div>
```

```figure uses=slider
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 290px)"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2" data-focused></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: min(100%, 290px)"><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2" aria-disabled="true"></div><span class="docs-note">Disabled</span></div>
</div>
```

- Focused: a neutral focus ring outside the picked pill or knob (standing one ring width off when the fill matches the ring's color), from the keyboard only. A press focuses without a ring.
- Hover: a stepped pill takes its hover step.
- Keys: arrows step, Home and End jump to the ends; Shift steps ten on a continuous slider.
- Disabled: rail, pills and knob drop to half strength, labels and heads turn faint, input is ignored, and it leaves the focus order.

## Values

Show the value people know, with its unit.

```figure uses=slider
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-steps" data-label="Scrollback" data-steps="1k, 5k, 10k, 50k, 100k" data-value="2"></div></div><figcaption>Show the value with its unit</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-steps" data-label="Scrollback" data-steps="1, 2, 3, 4, 5" data-value="2"></div></div><figcaption>Don't show the step's number</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Slider::new(id, label, val, range, on_change)` | `.penna-slider`, `.penna-rail`, `.penna-fill`, `.penna-knob`, `aria-label`, `data-min`, `data-max`, `data-value` | Continuous slider |
| `.step(size)`, `.display(format)` | `data-step` | Snaps steps; formats value text |
| `.commit_on_release()`, `.inline()`, `.track(colors)` | | Release-only commit; inline on touch; gradient rail |
| `.track_focus(&focus)`, `.shown()` | `data-focused` | Focus handle and live displayed value |
| `DiscreteSlider::new(id, label, steps, picked, on_change)` | `.penna-steps`, `data-label`, `data-steps`, `data-value` | Stepped pills |
| `StepRail::new(id, label, steps, picked, on_change)` | none yet | Thick rail (`picked: Option<usize>`) |
| `StepRail::open_start()` | none yet | First step is an open ring ("not set") |
| `.xsmall()` … `.touch()`, `.with_size(size)` | `data-size` | Size on continuous and stepped; medium default |
| `.disabled(true)` | `aria-disabled="true"` | Off on all three kinds |
| `on_change` | `change` | Gets the new value |

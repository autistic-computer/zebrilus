# Progress

How far a task has come, or that it is under way with no measure yet. For an unknown wait inside a control use a [spinner](spinner.html); to pick a value, a [slider](slider.html).

## Kinds

```figure uses=progress
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 240px)"><div class="penna-progress" role="progressbar" aria-label="Downloading" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div><span class="docs-note">Determinate</span></div>
  <div class="docs-cell" style="width: min(100%, 240px)"><div class="penna-progress" role="progressbar" aria-label="Verifying" data-indeterminate><div class="penna-progress-fill"></div></div><span class="docs-note">Indeterminate</span></div>
  <div class="docs-cell" style="width: min(100%, 240px)"><div class="penna-progress" data-kind="line" role="progressbar" aria-label="Loading" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.62" aria-valuetext="62%" style="--penna-value: 62%"><div class="penna-progress-fill"></div></div><span class="docs-note">Line</span></div>
</div>
```

A determinate bar fills to a value from 0 to 1. An indeterminate one sweeps a third of the track across it over 2000ms with a 600ms pause; under reduced motion it holds still in the middle third. The line is a page loading: 2 tall along the page's top edge, no track, square ends.

```rust
Progress::new("update", 0.42).label("Downloading")
Progress::indeterminate("verify").label("Verifying")
```

## Anatomy

1. **Track**: a pill as thick as the slider's rail, in the control fill, as wide as its container, with no hit box.
2. **Fill**: the accent, from the start to the value; draws nothing at 0 and eases to a new value in 220ms.
3. **Segment (indeterminate)**: a third of the track in the accent, sweeping across on a 2600ms loop.

Progress takes no pointer or keyboard input and never enters the focus order.

## Sizes

```figure uses=progress
<div class="docs-tokens" data-cols="3">
  <span class="docs-head">Size</span><span class="docs-head">Track</span><span class="docs-head">Example</span>
  <span>xsmall, small</span><span><code>4</code></span><span><div class="penna-progress" data-size="small" role="progressbar" aria-label="Small progress" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%; width: 160px"><div class="penna-progress-fill"></div></div></span>
  <span>medium and up</span><span><code>6</code></span><span><div class="penna-progress" role="progressbar" aria-label="Medium progress" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%; width: 160px"><div class="penna-progress-fill"></div></div></span>
</div>
```

Medium 6 is the default and does not follow density; `xsmall` and `small` draw at 4, and `large`, `xlarge` and `touch` draw at 6.

## Content and placement

The bar doesn't say what is under way. Put a line `gap-tight` 4 above it (for instance inside a toast or settings row).

```figure uses=progress,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="tight" style="width: min(100%, 200px)"><span class="penna-type" data-type="small" data-penna-text>Downloading 42%</span><div class="penna-progress" role="progressbar" aria-label="Downloading" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div></div></div><figcaption>Say what and how far</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div style="width: min(100%, 200px)"><div class="penna-progress" role="progressbar" aria-label="Progress" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div></div></div><figcaption>Don't leave a bar alone</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Progress::new(id, value)` | `.penna-progress`, `.penna-progress-fill`, `--penna-value` | Determinate bar; clamps to `0.0..=1.0`, NaN reads as `0.0` |
| `Progress::indeterminate(id)` | `data-indeterminate` | Indeterminate bar |
| `.line()` | `data-kind="line"` | 2 px page-loading line |
| `.small()`, `.with_size(size)` | `data-size` | Track height: 4 at `xs`/`sm`, 6 from `md` up |
| `.label(text)` | `role="progressbar"`, `aria-label`, `aria-value*` | Accessible name ("Progress" by default) and percentage |

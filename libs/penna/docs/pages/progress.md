# Progress

How far a task has come, or that it is under way with no measure yet.

## Kinds

```figure uses=progress
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 240px"><div class="penna-progress" role="progressbar" aria-label="Downloading" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.42" aria-valuetext="42%" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div><span class="docs-note">Determinate</span></div>
  <div class="docs-cell" style="width: 240px"><div class="penna-progress" role="progressbar" aria-label="Verifying" data-indeterminate><div class="penna-progress-fill"></div></div><span class="docs-note">Indeterminate</span></div>
</div>
```

A determinate bar fills to a value from 0 to 1. An indeterminate one sweeps a third of the track across it, for a step with no measure. Both hold still under reduced motion.

```rust
Progress::new("update", 0.42).label("Downloading")
```

## Anatomy

1. **Track**: a pill as thick as the slider's rail, in the control fill, as wide as its container.
2. **Fill**: the accent, from the start to the value. It eases to a new value in 220ms.

## Sizes

```figure uses=progress
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Track</span><span class="docs-head">Example</span>
  <span>small</span><span><code>4</code></span><span><div class="penna-progress" data-size="small" style="--penna-value: 42%; width: 160px"><div class="penna-progress-fill"></div></div></span>
  <span>medium</span><span><code>6</code></span><span><div class="penna-progress" style="--penna-value: 42%; width: 160px"><div class="penna-progress-fill"></div></div></span>
</div>
```

## Content

The bar doesn't say what is under way. Put a line above it that does.

```figure uses=progress,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="tight" style="width: 200px"><span class="penna-type" data-type="small" data-penna-text>Downloading 42%</span><div class="penna-progress" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div></div></div><figcaption>Say what and how far</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div style="width: 200px"><div class="penna-progress" style="--penna-value: 42%"><div class="penna-progress-fill"></div></div></div></div><figcaption>Don't leave a bar alone</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Progress::new(id, value)` | `.penna-progress`, `--penna-value` | Determinate bar |
| `Progress::indeterminate(id)` | `data-indeterminate` | Indeterminate bar |
| `.small()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.label(text)` | `aria-label` | Names it for assistive tech |

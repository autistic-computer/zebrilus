# Spinner

Something is under way and how long it takes is unknown. When progress is known use a [progress bar](progress.html); when steps are known, use steps.

## Kinds

```figure uses=spinner,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-spinner" role="progressbar" aria-label="Loading"><i data-icon="loader"></i></div><span class="docs-note">Ink</span></div>
  <div class="docs-cell"><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div><span class="docs-note">Muted</span></div>
  <div class="docs-cell"><div class="docs-row"><div class="penna-spinner" role="progressbar" aria-label="Reconnecting" data-color="warning"><i data-icon="loader"></i></div><span class="penna-type" data-type="body" data-tone="warning" data-penna-text>Reconnecting</span></div><span class="docs-note">Status with word</span></div>
</div>
```

One kind: the loader icon, turning. It takes the ink of the text around it. A status color goes only beside a word in that status.

```rust
Spinner::new().small().color(cx.theme().foreground.muted())
```

## Anatomy

```figure uses=spinner,label
<div class="docs-row"><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div><span class="penna-type" data-type="body" data-penna-text>Connecting to devbox</span></div>
```

1. **Icon**: an open 293° arc at the size's icon rung.
2. **Turn**: once every 900ms at steady speed; holds still under reduced motion.

## Sizes

```figure uses=spinner
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Icon</span><span class="docs-head">Beside</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>12</code></span><span>caption</span><span><div class="penna-spinner" data-size="xsmall"><i data-icon="loader"></i></div></span>
  <span>small</span><span><code>12</code></span><span>small, caption</span><span><div class="penna-spinner" data-size="small"><i data-icon="loader"></i></div></span>
  <span>medium</span><span><code>14</code></span><span>body</span><span><div class="penna-spinner"><i data-icon="loader"></i></div></span>
  <span>large</span><span><code>16</code></span><span>lead, heading</span><span><div class="penna-spinner" data-size="large"><i data-icon="loader"></i></div></span>
  <span>xlarge</span><span><code>16</code></span><span>lead</span><span><div class="penna-spinner" data-size="xlarge"><i data-icon="loader"></i></div></span>
  <span>touch</span><span><code>16</code></span><span>touch-body</span><span><div class="penna-spinner" data-size="touch"><i data-icon="loader"></i></div></span>
  <span>touch-large</span><span><code>20</code></span><span>touch-body</span><span><div class="penna-spinner" data-size="touch-large"><i data-icon="loader"></i></div></span>
</div>
```

In a control it takes the size of the icon it stands in for.

## States and placement

```figure uses=spinner,input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading type="button"><span class="penna-spinner"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Busy button</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-spinner-cover" data-loading><span class="penna-spinner-content"><span class="penna-label" data-penna-text>Deploy release</span></span><span class="penna-spinner-overlay"><span class="penna-spinner" role="progressbar" aria-label="Loading"><i data-icon="loader"></i></span></span></span></button><span class="docs-note">Layout-preserving cover</span></div>
  <div class="docs-cell"><div class="docs-row"><label class="penna-input" data-penna="field"><input value="Resolving host" aria-label="Host lookup" /><span class="penna-input-suffix"><span class="penna-spinner"><i data-icon="loader"></i></span></span></label></div><span class="docs-note">Checking field</span></div>
</div>
```

A spinner is the loading state of whatever holds it. Beside text in a line it sits `gap-related` 8 away; in a button it takes the icon's place; in a field it sits in the suffix before the clear button, one rung smaller than the field from medium up.

## Content

On its own, a spinner doesn't say what is loading. Put a line beside it.

```figure uses=spinner,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div><span class="penna-type" data-type="body" data-penna-text>Connecting to devbox</span></div><figcaption>Say what is under way</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div></div><figcaption>Don't leave a spinner alone</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Spinner::new()` | `.penna-spinner`, `<i data-icon="loader">` | Spinner |
| `.xsmall()` … `.touch_large()` | `data-size` | Size; medium by default |
| `.with_size(px(32.))` | `--penna-icon-size` | Size off the ladder |
| `.color(color)` | `data-color` (`muted`, `accent`, `success`, `warning`, `danger`) | Tint |
| `.covering(loading, child)` | `.penna-spinner-cover[data-loading]` | Preserves child dimensions at zero opacity while centering the spinner |
| `.id(id)` | `role="progressbar"`, `aria-label` | Announces it ("Loading" by default, no value) |

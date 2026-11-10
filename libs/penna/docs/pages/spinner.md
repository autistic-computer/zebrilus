# Spinner

Something is under way and how long it takes is unknown.

## Kinds

```figure uses=spinner
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-spinner" role="progressbar" aria-label="Loading"><i data-icon="loader"></i></div><span class="docs-note">Ink</span></div>
  <div class="docs-cell"><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div><span class="docs-note">Muted</span></div>
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

1. **Icon**: the loader, at its size's icon size.
2. **Turn**: once every 900ms, at one steady speed. It holds still under reduced motion.

## Sizes

```figure uses=spinner
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Icon</span><span class="docs-head">Beside</span><span class="docs-head">Example</span>
  <span>small</span><span><code>12</code></span><span>small, caption</span><span><div class="penna-spinner" data-size="small"><i data-icon="loader"></i></div></span>
  <span>medium</span><span><code>14</code></span><span>body</span><span><div class="penna-spinner"><i data-icon="loader"></i></div></span>
  <span>large</span><span><code>16</code></span><span>lead, heading</span><span><div class="penna-spinner" data-size="large"><i data-icon="loader"></i></div></span>
</div>
```

In a control it takes the size of the icon it stands in for.

## States

```figure uses=spinner,input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading><span class="penna-spinner" data-size="small"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Busy button</span></div>
  <div class="docs-cell"><div class="docs-row"><label class="penna-input" data-penna="field"><input value="Resolving host" /><span class="penna-input-suffix"><span class="penna-spinner"><i data-icon="loader"></i></span></span></label></div><span class="docs-note">Checking field</span></div>
</div>
```

A spinner is the loading state of whatever holds it. In a button it takes the icon's place; in a field it sits at the end.

## Content

On its own, a spinner doesn't say what is loading. Put a line beside it.

```figure uses=spinner,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div><span class="penna-type" data-type="body" data-penna-text>Connecting to devbox</span></div><figcaption>Say what is under way</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-spinner" role="progressbar" aria-label="Loading" data-color="muted"><i data-icon="loader"></i></div></div><figcaption>Don't leave a spinner alone</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Spinner::new()` | `.penna-spinner`, `<i data-icon="loader">` | Spinner |
| `.small()` … `.touch()` | `data-size` | Size; medium by default |
| `.with_size(px(32.))` | `--penna-icon-size` | Size off the ladder |
| `.color(color)` | `data-color` | Tint |
| `.id(id)` | `role="progressbar"`, `aria-label` | Announces it as loading |

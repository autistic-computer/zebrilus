# Segmented control

Picks one of two to five short options, all in view, and applies the pick at once. For more use a select.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>It has no keyboard path, no disabled state and no hover look. It comes in one size.</span></div></div></div>
```

## Anatomy

```figure uses=segmented
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="2" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>System</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Light</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Dark</span></span></div></div><span class="docs-note">Track, pill, segments</span></div>
</div>
```

1. **Track**: a field, medium height.
2. **Pill**: behind the picked segment. Slides to a new pick.
3. **Segments**: each as wide as its label.

```rust
SegmentedControl::new("mode", modes).selected(picked).on_select(pick)
```

## Sizes

One size: medium, which follows density.

## States

```figure uses=segmented
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">First picked</span></div>
  <div class="docs-cell"><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Tiled</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Floating</span></span></div></div><span class="docs-note">Second picked</span></div>
</div>
```

- Picked: the pill under it. Always exactly one.
- Press one to slide the pill there.

## Labels

One or two words each, about the same length.

```figure uses=segmented
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-segmented" data-penna="field" data-value="1" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>System</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Light</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Dark</span></span></div></div></div><figcaption>Short, even labels</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Follow the system</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Dark</span></span></div></div></div><figcaption>Don't write a sentence in a segment</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `SegmentedControl::new(id, labels)` | `.penna-segmented`, `.penna-segment` | The options |
| `.selected(index)` | `data-value` | The picked one |
| `.on_select(handler)` | `change` | Gets the picked index |
| `.on(ground)` | | Paints for the surface under it |

# Badge

A count or a short tag beside text. To show a state with a dot, use a chip.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Badges don't grow with the interface font size. The HTML version has no solid or ghost badge.</span></div></div></div>
```

## Kinds

```figure uses=badge
<div class="docs-show">
  <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>2/3</span></span>
  <span class="penna-badge" data-penna="badge" data-tone="warning"><span class="badge-text" data-penna-text>Waiting</span></span>
  <span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span>
  <span class="penna-badge" data-penna="badge.outline"><span class="badge-text" data-penna-text>Base</span></span>
</div>
```

- **Soft**: the default. Counts, tags, status.
- **Solid**: the one badge in a view that must stand out.
- **Outline**: a quiet tag, such as where a value comes from.
- **Ghost**: ink only, no fill.

Any kind takes a tone: neutral, primary, accent, danger, warning, success.

```rust
Badge::new("3 failing").icon(IconName::CircleX).tone(Tone::Danger)
```

## Anatomy

```figure uses=badge
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span><span class="docs-note">Icon, label</span></div>
  <div class="docs-cell"><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span><span class="docs-note">Count</span></div>
</div>
```

1. **Container**: a pill as tall as the line it sits in.
2. **Icon**: optional, before the label.
3. **Label**: a count or a few words.

## Sizes

```figure uses=badge
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>small</span><span><code>16</code></span><span><code>micro</code></span><span><span class="penna-badge" data-penna="badge" data-size="small"><span class="badge-text" data-penna-text>New</span></span></span>
  <span>medium</span><span><code>18</code></span><span><code>caption</code></span><span><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>New</span></span></span>
</div>
```

Medium beside body text, small beside small text. Extra small draws like small, large and up like medium.

## States

One look. A badge is read-only: it never hovers, presses or takes focus.

## Content

A count, or one to three words in sentence case. A status tone always comes with a word.

```figure uses=badge
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span></div><figcaption>Say what the tone means</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span class="penna-badge" data-penna="badge" data-tone="danger"><span class="badge-text" data-penna-text>3</span></span></div><figcaption>Don't leave it to color</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Badge::new(label)` | `.penna-badge`, `.badge-text` | Label |
| `.icon(name)` | `<i data-icon>` | Icon before the label |
| `.tone(tone)` | `data-tone` | Tone |
| `.emphasis(Emphasis::Outline)` | `data-penna="badge.outline"` | Outline kind |
| `.emphasis(Emphasis::Solid)`, `Ghost` | none yet | Solid and ghost kinds |
| `.small()` | `data-size="small"` | Size; medium by default |
| `.on(ground)` | `data-ground` on the surface | Paint for the ground under it |
| `.opaque()` | none yet | Fills with the ground's own fill |

# Badge

A count or a short tag beside text. To show a state with a dot, use a chip.

## Kinds

```figure uses=badge
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span><span class="docs-note">Soft</span></div>
  <div class="docs-cell"><span class="penna-badge" data-penna="badge" data-tone="primary" data-emphasis="solid" data-solid="primary"><span class="penna-solid-ink" data-penna="button" data-prominent data-tone="primary"><span class="badge-text" data-penna-text>New</span></span></span><span class="docs-note">Solid</span></div>
  <div class="docs-cell"><span class="penna-badge" data-penna="badge.outline"><span class="badge-text" data-penna-text>Base</span></span><span class="docs-note">Outline</span></div>
  <div class="docs-cell"><span class="penna-badge" data-penna="badge" data-tone="warning" data-emphasis="ghost"><i data-icon="triangle-alert"></i><span class="badge-text" data-penna-text>Waiting</span></span><span class="docs-note">Ghost</span></div>
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

**Opaque** flattens its fill onto the ground, so it looks the same but hides what is under it.

```figure uses=badge,card,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-card" data-penna="card.bubble" style="position: relative; padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span><span class="penna-badge" data-penna="badge" data-tone="danger" style="position: absolute; top: calc(var(--penna-text-body-line) / -2); right: var(--penna-space-12)"><span class="badge-text" data-penna-text>2</span></span></div><span class="docs-note">Soft over the edge</span></div>
  <div class="docs-cell"><div class="penna-card" data-penna="card.bubble" style="position: relative; padding: var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Can you look at the test?</span><span class="penna-badge" data-penna="badge" data-tone="danger" data-opaque data-on="card.bubble" style="position: absolute; top: calc(var(--penna-text-body-line) / -2); right: var(--penna-space-12)"><span class="badge-text" data-penna-text>2</span></span></div><span class="docs-note">Opaque over the edge</span></div>
</div>
```

## Anatomy

```figure uses=badge
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span><span class="docs-note">Icon, label</span></div>
  <div class="docs-cell"><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span><span class="docs-note">Count</span></div>
</div>
```

1. **Container**: a pill as tall as the line of text it sits in.
2. **Icon**: optional, 12 at every size, before the label.
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
| `.emphasis(Emphasis::Solid)` | `data-emphasis="solid"`, `data-solid`, `.penna-solid-ink` | Solid kind |
| `.emphasis(Emphasis::Ghost)` | `data-emphasis="ghost"` | Ghost kind |
| `.small()` | `data-size="small"` | Size; medium by default |
| `.on(ground)` | `data-ground` on the surface | Paint for the ground under it |
| `.opaque()` | `data-opaque`, `data-on="card.bubble"` | Flattens its fill onto the ground |

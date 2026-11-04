# Checkbox and radio

Marks a choice: a checkbox for an option on its own, a radio for one picked in a set. For a setting that applies at once use a switch.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>It is a mark only: the row around it takes the press and the keys. No radio groups, mixed state or error. The checkbox's corners are too round, and the web version has no radio and fills with the accent.</span></div></div></div>
```

## Kinds

- **Checkbox**: on or off by itself. A check when on.
- **Radio**: one picked in a set. A dot when on. Native only for now.

```rust
Check::checkbox(done)
```

## Anatomy

1. **Box**: an edge when off, filled with the highlight when on.
2. **Mark**: a check, or a radio's dot, in the fill's ink.

It has no label of its own: the row that holds it shows one.

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Box</span><span class="docs-head">Beside</span><span class="docs-head">Example</span>
  <span>xsmall, small</span><span><code>14</code></span><span><code>caption, small</code></span><span><span class="penna-check" data-size="small" data-on><i data-icon="check"></i></span></span>
  <span>medium and up</span><span><code>16</code></span><span><code>body</code></span><span><span class="penna-check" data-on><i data-icon="check"></i></span></span>
</div>
```

Medium is the default. It sits in a line of text, so larger sizes draw like medium.

## States

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-check"></span><span class="docs-note">Off</span></div>
  <div class="docs-cell"><span class="penna-check" data-on><i data-icon="check"></i></span><span class="docs-note">On</span></div>
</div>
```

- Disabled: a fainter edge, fill and mark. Native only for now.
- Hover, press and focus belong to the row around it.

## Labels

Phrase the option so that on means yes.

```figure uses=label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="docs-row"><span class="penna-check" data-on><i data-icon="check"></i></span><span class="penna-type" data-type="body" data-penna-text>Restore sessions</span></div></div><figcaption>On turns it on</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="docs-row"><span class="penna-check" data-on><i data-icon="check"></i></span><span class="penna-type" data-type="body" data-penna-text>Don't restore sessions</span></div></div><figcaption>Don't phrase it as a negative</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Check::checkbox(checked)` | `.penna-check` | Checkbox |
| `Check::radio(checked)` | | Radio |
| `checked` | `data-on`, `<i data-icon="check">` | On |
| `.xsmall()`, `.small()` | `data-size` | The 14 box |
| `.tone(tone)` | | Color when on |
| `.on(ground)` | | Paints for the surface under it |
| `.disabled(true)` | | Fainter |

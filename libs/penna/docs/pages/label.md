# Label

One run of interface text in a text style. For documents and chat, use text.

## Kinds

```figure uses=label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-type" data-type="body" data-penna-text>Ready</span><span class="docs-note">Full</span></div>
  <div class="docs-cell"><span class="penna-type" data-type="body" data-muted data-penna-text>ubuntu is to the right</span><span class="docs-note">Muted</span></div>
  <div class="docs-cell"><span class="penna-type" data-type="body" data-faint data-penna-text>Last seen</span><span class="docs-note">Faint</span></div>
  <div class="docs-cell"><span class="penna-type" data-type="body" data-tone="danger" data-penna-text>Failed</span><span class="docs-note">Tone</span></div>
  <div class="docs-cell"><span class="penna-type" data-type="body" data-strong data-penna-text>Paired computers</span><span class="docs-note">Strong</span></div>
  <div class="docs-cell"><span class="penna-type" data-type="body" data-mono data-penna-text>git log --stat</span><span class="docs-note">Code</span></div>
</div>
```

- **Full**: the ink of the text around it. The default.
- **Muted**: descriptions and details.
- **Faint**: placeholders and disabled text only.
- **Tone**: a word that is a status.
- **Strong**: 500, or 600 in headings and reading text.
- **Code**: the code face at the same size.

```rust
Label::new(TextStyle::Small, "ubuntu is to the right").muted()
```

## Anatomy

Just the text, no box. Its style sets size, line height and weight together. It wraps when it runs out of room; truncated, it stays on one line and ends in an ellipsis.

```figure uses=label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 180px"><span class="penna-type" data-type="body" data-penna-text>Keyboard and pointer follow the cursor to the next computer</span><span class="docs-note">Wraps</span></div>
  <div class="docs-cell" style="width: 180px"><span class="penna-type" data-type="body" data-truncate data-penna-text>Keyboard and pointer follow the cursor to the next computer</span><span class="docs-note">Truncated</span></div>
</div>
```

## Sizes

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size</span><span class="docs-head">Weight</span><span class="docs-head">Example</span>
  <span>micro</span><span><code>10 / 14</code></span><span><code>400</code></span><span><span class="penna-type" data-type="micro" data-penna-text>12 new</span></span>
  <span>caption</span><span><code>11 / 16</code></span><span><code>400</code></span><span><span class="penna-type" data-type="caption" data-penna-text>Used by three windows</span></span>
  <span>small</span><span><code>12 / 16</code></span><span><code>400</code></span><span><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span></span>
  <span>body</span><span><code>13 / 18</code></span><span><code>400</code></span><span><span class="penna-type" data-type="body" data-penna-text>Split the pane to the right</span></span>
  <span>lead</span><span><code>14 / 20</code></span><span><code>400</code></span><span><span class="penna-type" data-type="lead" data-penna-text>Pick a theme</span></span>
  <span>heading</span><span><code>16 / 22</code></span><span><code>600</code></span><span><span class="penna-type" data-type="heading" data-penna-text>Appearance</span></span>
  <span>title</span><span><code>20 / 26</code></span><span><code>600</code></span><span><span class="penna-type" data-type="title" data-penna-text>Keyboard</span></span>
  <span>display</span><span><code>26 / 32</code></span><span><code>600</code></span><span><span class="penna-type" data-type="display" data-penna-text>No sessions</span></span>
  <span>mono</span><span><code>13 / 18</code></span><span><code>400</code></span><span><span class="penna-type" data-type="mono" data-penna-text>git log --stat</span></span>
</div>
```

Most text is body. The reading and touch styles are on Typography.

## States

A label takes no input. Inside a disabled control it is faint; an error is a danger word.

## Content

Sentence case. Two or three text styles in a view.

```figure uses=label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><span class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Paired computers</span><span class="penna-type" data-type="small" data-muted data-penna-text>Keyboard and pointer follow the cursor.</span></span></div><figcaption>A name and a muted line</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span class="penna-stack" data-gap="tight"><span class="penna-type" data-type="title" data-penna-text>Paired computers</span><span class="penna-type" data-type="lead" data-strong data-penna-text>Keyboard and pointer</span><span class="penna-type" data-type="caption" data-muted data-penna-text>follow the cursor.</span></span></div><figcaption>Don't stack a style per line</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Label::new(style, text)` | `.penna-type`, `data-type` | Text in a style |
| `.muted()` | `data-muted` | Muted ink |
| `.faint()` | `data-faint` | Faint ink |
| `.tone(tone)` | `data-tone` | Ink in a tone |
| `.strong()` | `data-strong` | Heavier weight |
| `.mono()` | `data-mono` | Code face at the style's size |
| `.truncate()` | `data-truncate` | One line, ends in an ellipsis |
| `.on(ground)` | the surface around it | Works out its ink for that ground |

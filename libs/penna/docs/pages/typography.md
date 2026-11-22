# Typography

One face for the interface, one for code. Most text on a screen is `body`.

## Faces

Native windows use `.SystemUIFont` and platform monospace; the web and storybook pair Inter Variable with CommitMono.

```figure uses=label
<div class="docs-type-faces">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Interface</span>
      <span class="docs-pr-col-meta">Inter Variable</span>
    </div>
    <div class="docs-pr-card" data-penna="panel" data-ground>
      <div class="docs-type-face-specimen">Aa Bb</div>
      <div class="docs-type-face-glyphs">Aa Bb Cc Dd Ee Ff · 0123456789</div>
      <div class="docs-type-face-foot">
        <span>400 Regular</span>
        <span>·</span>
        <span>500 Strong</span>
        <span>·</span>
        <span>600 Heading</span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Code</span>
      <span class="docs-pr-col-meta">CommitMono</span>
    </div>
    <div class="docs-pr-card" data-penna="panel" data-ground>
      <div class="docs-type-face-specimen" data-mono>Aa 01</div>
      <div class="docs-type-face-glyphs" data-mono>0O 1lI {} =&gt; != :: +- 0123456789</div>
      <div class="docs-type-face-foot">
        <span>400 Regular</span>
        <span>·</span>
        <span>700 Bold</span>
        <span>·</span>
        <span>Italic</span>
      </div>
    </div>
  </div>
</div>
```

## Interface

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Weight</span><span class="docs-head">Sample</span>
  <span><code>display</code></span><span><code>26 / 32</code></span><span><code>600</code></span><span class="penna-type" data-type="display" data-penna-text>No sessions yet</span>
  <span><code>title</code></span><span><code>20 / 26</code></span><span><code>600</code></span><span class="penna-type" data-type="title" data-penna-text>Keyboard shortcuts</span>
  <span><code>heading</code></span><span><code>16 / 22</code></span><span><code>600</code></span><span class="penna-type" data-type="heading" data-penna-text>Appearance</span>
  <span><code>lead</code></span><span><code>14 / 20</code></span><span><code>400</code></span><span class="penna-type" data-type="lead" data-penna-text>Keep every pane in one look</span>
  <span><code>body</code></span><span><code>13 / 18</code></span><span><code>400</code></span><span class="penna-type" data-type="body" data-penna-text>Split the pane to the right</span>
  <span><code>small</code></span><span><code>12 / 16</code></span><span><code>400</code></span><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span>
  <span><code>caption</code></span><span><code>11 / 16</code></span><span><code>400</code></span><span class="penna-type" data-type="caption" data-penna-text>Used by three windows</span>
  <span><code>micro</code></span><span><code>10 / 14</code></span><span><code>400</code></span><span class="penna-type" data-type="micro" data-penna-text>12 new</span>
  <span><code>mono</code></span><span><code>13 / 18</code></span><span><code>400</code></span><span class="penna-type" data-type="mono" data-penna-text>git log --stat</span>
</div>
```

Strong raises the small interface styles to `500`. Headings are `600`. No other weights.

```figure uses=label
<div class="docs-type-weights">
  <div class="docs-type-weight" data-penna="panel" data-ground>
    <span class="docs-type-weight-meta">400 · Regular</span>
    <span class="penna-type" data-type="body" data-penna-text>Split pane right</span>
  </div>
  <div class="docs-type-weight" data-penna="panel" data-ground>
    <span class="docs-type-weight-meta">500 · Strong</span>
    <span class="penna-type" data-type="body" data-strong data-penna-text>Split pane right</span>
  </div>
  <div class="docs-type-weight" data-penna="panel" data-ground>
    <span class="docs-type-weight-meta">600 · Heading</span>
    <span class="penna-type" data-type="heading" data-penna-text>Split pane right</span>
  </div>
</div>
```

## Hierarchy

Two or three styles per view. Weight and ink do the work before size, never decorative color.

```figure uses=label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Next pane</span><span class="penna-type" data-type="small" data-muted data-penna-text>Move to the pane on the right.</span></div></div><figcaption>Weight and ink do the work</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="heading" style="color: var(--penna-accent)" data-penna-text>Next pane</span><span class="penna-type" data-type="lead" data-penna-text>Move to the pane on the right.</span></div></div><figcaption>Don't size up or color a label</figcaption></figure>
</div>
```

## Reading

Documents and chat reuse the interface sizes with taller line heights, capped at a `640` measure. Headings sit `8` above their paragraph; blocks sit `12` apart.

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Weight</span><span class="docs-head">Sample</span>
  <span><code>read-h1</code></span><span><code>26 / 32</code></span><span><code>600</code></span><span class="penna-type" data-type="read-h1" data-penna-text>Sessions</span>
  <span><code>read-h2</code></span><span><code>20 / 28</code></span><span><code>600</code></span><span class="penna-type" data-type="read-h2" data-penna-text>Windows</span>
  <span><code>read-h3</code></span><span><code>16 / 24</code></span><span><code>600</code></span><span class="penna-type" data-type="read-h3" data-penna-text>Panes</span>
  <span><code>read-h4</code></span><span><code>14 / 22</code></span><span><code>600</code></span><span class="penna-type" data-type="read-h4" data-penna-text>Keys</span>
  <span><code>read-h5</code></span><span><code>13 / 20</code></span><span><code>600</code></span><span class="penna-type" data-type="read-h5" data-penna-text>Copy mode</span>
  <span><code>read-body</code></span><span><code>14 / 22</code></span><span><code>400</code></span><span class="penna-type" data-type="read-body" data-penna-text>The server owns every process.</span>
</div>
```

```figure uses=label
<div class="docs-type-read" data-penna="panel" data-ground>
  <div class="docs-type-read-head">
    <span class="penna-type" data-type="read-h3" data-penna-text>Detached sessions</span>
    <p class="penna-type" data-type="read-body">The server owns every process, so closing a window leaves running panes intact. Bold runs in reading text step straight to <strong style="font-weight: 600">600</strong>.</p>
  </div>
  <p class="penna-type" data-type="read-body">Paragraphs sit 12 apart under a 22 line height so long transcripts read without crowding.</p>
</div>
```

## Touch

On a phone and in sheets. Nothing on a phone drops below `13`.

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Weight</span><span class="docs-head">Sample</span>
  <span><code>touch-title</code></span><span><code>34 / 40</code></span><span><code>600</code></span><span class="penna-type" data-type="touch-title" data-penna-text>Hosts</span>
  <span><code>touch-body</code></span><span><code>17 / 22</code></span><span><code>400</code></span><span class="penna-type" data-type="touch-body" data-penna-text>Split right</span>
  <span><code>touch-small</code></span><span><code>15 / 20</code></span><span><code>400</code></span><span class="penna-type" data-type="touch-small" data-penna-text>Two panes</span>
</div>
```

## Code

Inline code inherits the size and line height of the text around it, in the code face.

```figure uses=label
<div class="docs-type-code-card" data-penna="panel" data-ground>
  <div class="docs-type-code-row">
    <span class="penna-type" data-type="body">Run <code class="docs-type-inline-code">just libs pull penna</code> in both apps.</span>
    <span class="docs-type-code-tag">body · 13 / 18</span>
  </div>
  <div class="docs-type-code-row">
    <span class="penna-type" data-type="read-body">Run <code class="docs-type-inline-code">just libs pull penna</code> in both apps.</span>
    <span class="docs-type-code-tag">read-body · 14 / 22</span>
  </div>
  <div class="docs-type-code-row">
    <span class="penna-type" data-type="touch-body">Run <code class="docs-type-inline-code">zz attach</code> to resume.</span>
    <span class="docs-type-code-tag">touch-body · 17 / 22</span>
  </div>
</div>
```

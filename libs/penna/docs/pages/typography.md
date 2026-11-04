# Typography

One face for the interface, one for code. Most text is body.

## Interface

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Weight</span><span class="docs-head">Sample</span>
  <span><code>display</code></span><span><code>26 / 32</code></span><span><code>600</code></span><span class="penna-type" data-type="display" data-penna-text>No sessions yet</span>
  <span><code>title</code></span><span><code>20 / 26</code></span><span><code>600</code></span><span class="penna-type" data-type="title" data-penna-text>Keyboard shortcuts</span>
  <span><code>heading</code></span><span><code>16 / 22</code></span><span><code>600</code></span><span class="penna-type" data-type="heading" data-penna-text>Appearance</span>
  <span><code>lead</code></span><span><code>14 / 20</code></span><span><code>400</code></span><span class="penna-type" data-type="lead" data-penna-text>Pick a theme for every window</span>
  <span><code>body</code></span><span><code>13 / 18</code></span><span><code>400</code></span><span class="penna-type" data-type="body" data-penna-text>Split the pane to the right</span>
  <span><code>small</code></span><span><code>12 / 16</code></span><span><code>400</code></span><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span>
  <span><code>caption</code></span><span><code>11 / 16</code></span><span><code>400</code></span><span class="penna-type" data-type="caption" data-penna-text>Used by three windows</span>
  <span><code>micro</code></span><span><code>10 / 14</code></span><span><code>400</code></span><span class="penna-type" data-type="micro" data-penna-text>12 new</span>
  <span><code>mono</code></span><span><code>13 / 18</code></span><span><code>400</code></span><span class="penna-type" data-type="mono" data-penna-text>git log --stat</span>
</div>
```

Strong raises the small styles to 500. Headings are 600. No other weights.

## Hierarchy

Two or three styles per view. Weight before size, never color.

```figure uses=label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-strong data-penna-text>Next pane</span><span class="penna-type" data-type="small" data-muted data-penna-text>Move to the pane on the right.</span></div></div><figcaption>Weight and ink do the work</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="heading" style="color: var(--penna-accent)" data-penna-text>Next pane</span><span class="penna-type" data-type="lead" data-penna-text>Move to the pane on the right.</span></div></div><figcaption>Don't size up or color a label</figcaption></figure>
</div>
```

## Reading

Documents and chat: larger and more open, capped at 640 wide.

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

## Touch

On a phone and in sheets.

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Weight</span><span class="docs-head">Sample</span>
  <span><code>touch-title</code></span><span><code>34 / 40</code></span><span><code>600</code></span><span class="penna-type" data-type="touch-title" data-penna-text>Language</span>
  <span><code>touch-body</code></span><span><code>17 / 22</code></span><span><code>400</code></span><span class="penna-type" data-type="touch-body" data-penna-text>Split right</span>
  <span><code>touch-small</code></span><span><code>15 / 20</code></span><span><code>400</code></span><span class="penna-type" data-type="touch-small" data-penna-text>Two panes, one window</span>
</div>
```

## Code

Code takes the size of the text around it, in the code face.

```figure uses=label
<p style="margin: 0; font-size: var(--penna-text-body-size); line-height: var(--penna-text-body-line)">Run <code style="font-family: var(--penna-mono); font-size: inherit">just libs pull penna</code> in both apps.</p>
```

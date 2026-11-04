# Spacing

The closer two things relate, the closer they sit.

## Scale

```figure
<div class="docs-tokens">
  <span class="docs-head">Token</span><span class="docs-head">Value</span><span class="docs-head">Use</span><span class="docs-head"></span>
  <span><code>space-2</code></span><span><code>2</code></span><span>Inside controls</span><span><i class="docs-bar" style="width: 2px"></i></span>
  <span><code>space-4</code></span><span><code>4</code></span><span>Tight</span><span><i class="docs-bar" style="width: 4px"></i></span>
  <span><code>space-6</code></span><span><code>6</code></span><span>Inside controls, chrome</span><span><i class="docs-bar" style="width: 6px"></i></span>
  <span><code>space-8</code></span><span><code>8</code></span><span>Related</span><span><i class="docs-bar" style="width: 8px"></i></span>
  <span><code>space-10</code></span><span><code>10</code></span><span>Inside controls</span><span><i class="docs-bar" style="width: 10px"></i></span>
  <span><code>space-12</code></span><span><code>12</code></span><span>Card inset</span><span><i class="docs-bar" style="width: 12px"></i></span>
  <span><code>space-16</code></span><span><code>16</code></span><span>Group, pane inset</span><span><i class="docs-bar" style="width: 16px"></i></span>
  <span><code>space-24</code></span><span><code>24</code></span><span>Heading above</span><span><i class="docs-bar" style="width: 24px"></i></span>
  <span><code>space-32</code></span><span><code>32</code></span><span>Section, page inset</span><span><i class="docs-bar" style="width: 32px"></i></span>
  <span><code>space-40</code></span><span><code>40</code></span><span>Page top</span><span><i class="docs-bar" style="width: 40px"></i></span>
  <span><code>space-64</code></span><span><code>64</code></span><span>Page bottom</span><span><i class="docs-bar" style="width: 64px"></i></span>
</div>
```

Nothing off the scale. 2, 6 and 10 stay inside controls.

## Gaps

Pick by relationship, not by eye. The parent sets it.

```figure
<div class="docs-tokens">
  <span class="docs-head">Gap</span><span class="docs-head">Value</span><span class="docs-head">Between</span><span class="docs-head"></span>
  <span><code>gap-tight</code></span><span><code>4</code></span><span>A label and what it names</span><span><i class="docs-bar" style="width: 4px"></i></span>
  <span><code>gap-related</code></span><span><code>8</code></span><span>Things that work together</span><span><i class="docs-bar" style="width: 8px"></i></span>
  <span><code>gap-group</code></span><span><code>16</code></span><span>Groups in a pane</span><span><i class="docs-bar" style="width: 16px"></i></span>
  <span><code>gap-section</code></span><span><code>32</code></span><span>Parts of a page</span><span><i class="docs-bar" style="width: 32px"></i></span>
</div>
```

```figure uses=label,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="docs-form" data-spacing="grouped"><div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" /></label></div><div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Owner</span><label class="penna-input" data-penna="field"><input value="autistic-computer" /></label></div></div></div><figcaption>Label tight to its field</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="docs-form" data-spacing="even"><div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" /></label></div><div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Owner</span><label class="penna-input" data-penna="field"><input value="autistic-computer" /></label></div></div></div><figcaption>Don't space everything evenly</figcaption></figure>
</div>
```

## Insets

The container sets the space inside it.

```figure
<div class="docs-tokens">
  <span class="docs-head">Inset</span><span class="docs-head">Value</span><span class="docs-head">Inside</span><span class="docs-head"></span>
  <span><code>inset-chrome</code></span><span><code>6</code></span><span>Bars and pane headers</span><span><i class="docs-bar" style="width: 6px"></i></span>
  <span><code>inset-card</code></span><span><code>12</code></span><span>Cards and popovers</span><span><i class="docs-bar" style="width: 12px"></i></span>
  <span><code>inset-pane</code></span><span><code>16</code></span><span>Panes and dialogs</span><span><i class="docs-bar" style="width: 16px"></i></span>
  <span><code>inset-page</code></span><span><code>32</code></span><span>Document pages</span><span><i class="docs-bar" style="width: 32px"></i></span>
</div>
```

Components never set an outer margin. The row, column or card that holds them does.

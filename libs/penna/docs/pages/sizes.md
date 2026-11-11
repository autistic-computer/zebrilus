# Sizes

One ladder of control sizes. Each step sets height, text, padding and icon together.

## Ladder

```figure uses=input
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span class="docs-show"><label class="penna-input" data-penna="field" data-size="xsmall" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="xsmall"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span class="docs-show"><label class="penna-input" data-penna="field" data-size="small" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="small"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span class="docs-show"><label class="penna-input" data-penna="field" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span class="docs-show"><label class="penna-input" data-penna="field" data-size="large" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="large"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span class="docs-show"><label class="penna-input" data-penna="field" data-size="xlarge" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="xlarge"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span class="docs-show"><label class="penna-input" data-penna="field" data-size="touch" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="touch"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span style="min-height: 72px">touch-large</span><span style="min-height: 72px"><code>56</code></span><span style="min-height: 72px"><code>touch-body</code></span><span class="docs-show" style="min-height: 72px"><label class="penna-input" data-penna="field" data-size="touch-large" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Save</span></button></span>
</div>
```

Small and xsmall for dense places. Large and xlarge for one main action. Touch under a finger: the smallest target. Touch-large, not built natively yet, for a phone's main actions and fields (see [Phone](phone.html)).

## One size per row

```figure uses=input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><label class="penna-input" data-penna="field" style="width: 150px"><input placeholder="Search panes" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>Tops, text and edges line up</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><label class="penna-input" data-penna="field" data-size="small" style="width: 150px"><input placeholder="Search panes" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>Don't mix sizes in a row</figcaption></figure>
</div>
```

## Density

A setting moves the default size and every row with it. Text and gaps stay.

```figure uses=input
<div class="docs-tokens">
  <span class="docs-head">Density</span><span class="docs-head">Medium</span><span class="docs-head"></span><span class="docs-head">Example</span>
  <span>Compact</span><span><code>24</code></span><span></span><span class="docs-show" style="--penna-control-md: 24px"><label class="penna-input" data-penna="field" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>Regular</span><span><code>28</code></span><span></span><span class="docs-show"><label class="penna-input" data-penna="field" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>Comfortable</span><span><code>32</code></span><span></span><span class="docs-show" style="--penna-control-md: 32px"><label class="penna-input" data-penna="field" style="width: 120px"><input placeholder="Search" /></label><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button></span>
</div>
```

## In a line of text

Keys and badges take the line's height, so the line never grows.

```figure uses=label,kbd
<p class="penna-type docs-text" data-type="body">Press <kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd> for the palette, <kbd class="penna-kbd" data-penna="keycap" data-kbd="escape"></kbd> to close it.</p>
```

# Sizes

Controls come in two sizes. Medium is the default, for settings, dialogs and forms. Small is for dense places: toolbars, lists and anything that repeats down the screen.

```figure story=buttons/sizes
```

## One size per row

Everything in a row shares one size, so tops, text and edges line up with no nudging.

```figure uses=input,segmented
<div class="docs-full" style="display: flex; flex-direction: column; gap: 28px">
  <div style="display: flex; flex-direction: column; gap: 12px">
    <div class="docs-row">
      <button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Back</span></button>
      <label class="penna-input" data-penna="field"><input placeholder="Search panes" /></label>
      <span class="docs-spacer"></span>
      <div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="View"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>List</span></span></div></div>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Share</span></button>
    </div>
    <div class="docs-verdict" data-verdict="do"><i data-icon="circle-check"></i><span class="penna-type" data-type="small" data-penna-text>One size: the button, the field and the switcher share a line.</span></div>
  </div>
  <div style="display: flex; flex-direction: column; gap: 12px">
    <div class="docs-row">
      <button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Back</span></button>
      <label class="penna-input" data-penna="field" data-size="small"><input placeholder="Search panes" /></label>
      <span class="docs-spacer"></span>
      <div class="penna-segmented" data-penna="field" data-value="0" role="radiogroup" aria-label="View"><div class="penna-segments" data-penna="button.quiet" data-state="current"><span class="penna-segment-pill"></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>Grid</span></span><span class="penna-segment" role="button"><span class="penna-label" data-penna-text>List</span></span></div></div>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small"><span class="penna-label" data-penna-text>Share</span></button>
    </div>
    <div class="docs-verdict" data-verdict="dont"><i data-icon="circle-x"></i><span class="penna-type" data-type="small" data-penna-text>Mixed sizes: the small field and the small button break the line.</span></div>
  </div>
</div>
```

## Fields

Fields take the same two sizes, small on the left and medium on the right, so a field always lines up with the buttons beside it.

```figure story=inputs/field-sizes
```

## Inside a line of text

Keys and badges inside a sentence take the height of the line, so the line never grows to fit them.

```figure uses=label,kbd
<p class="penna-type docs-text" data-type="body">Press <kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd> to open the palette, then <kbd class="penna-kbd" data-penna="keycap" data-kbd="escape"></kbd> to close it again.</p>
```

# Spacing

Space is how penna shows what belongs together. The whole idea fits in one sentence: the more two things are related, the closer they sit.

```figure uses=label,input
<div class="docs-compare docs-full">
  <div>
    <div class="docs-form" data-spacing="grouped">
      <div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" /></label></div>
      <div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Repository</span><label class="penna-input" data-penna="field"><input value="autistic-computer/penna" /></label></div>
      <div class="docs-actions"><button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Create</span></button></div>
    </div>
    <div class="docs-verdict" data-verdict="do"><i data-icon="circle-check"></i><span class="penna-type" data-type="small" data-penna-text>Each label sits right on its field, and the buttons stand apart.</span></div>
  </div>
  <div>
    <div class="docs-form" data-spacing="even">
      <div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" /></label></div>
      <div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Repository</span><label class="penna-input" data-penna="field"><input value="autistic-computer/penna" /></label></div>
      <div class="docs-actions"><button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Create</span></button></div>
    </div>
    <div class="docs-verdict" data-verdict="dont"><i data-icon="circle-x"></i><span class="penna-type" data-type="small" data-penna-text>Even spacing leaves each label floating between two fields.</span></div>
  </div>
</div>
```

## Four distances

penna uses four distances between things, from tight to wide:

- **Tight**: a label and the thing it names, like a title and the line under it.
- **Related**: things that work together, like the buttons in a bar or a heading and its card.
- **Group**: separate groups inside a pane.
- **Section**: the big parts of a page.

Here they are in one settings pane. Nothing is outlined, and you can still see the structure.

```figure uses=label,switch,button
<div class="docs-full" style="display: flex; flex-direction: column; gap: 32px; max-width: 480px">
  <div style="display: flex; flex-direction: column; gap: 4px">
    <span class="penna-type" data-type="title" data-penna-text>Notifications</span>
    <span class="penna-type docs-text" data-type="body" data-muted data-penna-text>Choose what reaches you while you work.</span>
  </div>
  <div style="display: flex; flex-direction: column; gap: 8px">
    <span class="penna-type" data-type="heading" data-penna-text>Alerts</span>
    <div data-penna="settings.card" style="display: flex; flex-direction: column; padding: 4px 12px">
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>Bell in background panes</span><span class="docs-spacer"></span><button class="penna-toggle" role="switch" aria-checked="true" aria-label="Bell in background panes"></button></div>
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>When an agent finishes</span><span class="docs-spacer"></span><button class="penna-toggle" role="switch" aria-checked="true" aria-label="When an agent finishes"></button></div>
    </div>
  </div>
  <div style="display: flex; flex-direction: column; gap: 8px">
    <span class="penna-type" data-type="heading" data-penna-text>Quiet hours</span>
    <div data-penna="settings.card" style="display: flex; flex-direction: column; padding: 4px 12px">
      <div class="docs-row" style="padding: 8px 0"><span class="penna-type" data-type="body" data-penna-text>Mute alerts at night</span><span class="docs-spacer"></span><button class="penna-toggle" role="switch" aria-checked="false" aria-label="Mute alerts at night"></button></div>
    </div>
    <div class="docs-actions"><button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Reset</span></button><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Done</span></button></div>
  </div>
</div>
```

## Who decides the space

A control never pushes its neighbors away. The row, column or card that holds it decides the space around it, so the same button sits right in a toolbar and in a dialog without changes.

Containers work the same way from the inside: a card keeps its content a set distance from its edge, whatever is in it. That is why every card in penna looks like it belongs to the same family.

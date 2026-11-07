# Color

Color separates surfaces, marks what is on, and warns. Never decoration.

## Surfaces

Each layer steps away from the one under it: darker in light mode, lighter in dark.

```figure
<div class="docs-tokens">
  <span class="docs-head">Role</span><span class="docs-head">Token</span><span class="docs-head">Swatch</span><span class="docs-head">Use</span>
  <span>Window</span><span><code>bg</code></span><span><i class="docs-swatch" style="background: var(--penna-bg)"></i></span><span>Window and page</span>
  <span>Panel</span><span><code>panel</code></span><span><i class="docs-swatch" style="background: var(--penna-panel)"></i></span><span>Sidebars and side panels</span>
  <span>Popover</span><span><code>popover</code></span><span><i class="docs-swatch" style="background: var(--penna-popover)"></i></span><span>Menus and popovers</span>
  <span>Divider</span><span><code>divider</code></span><span><i class="docs-swatch" style="background: linear-gradient(var(--penna-divider), var(--penna-divider)) center / 100% 1px no-repeat"></i></span><span>Rules, the ink at 8%</span>
</div>
```

On a blurred window each surface takes its step as a wash over the window, so the layers stay a step apart and the window stays see-through.

## Ink

Full ink for what people read, muted for what supports it, faint only for placeholders and things that are off.

```figure
<div class="docs-tokens">
  <span class="docs-head">Ink</span><span class="docs-head">Token</span><span class="docs-head">Sample</span><span class="docs-head">Use</span>
  <span>Full</span><span><code>fg</code></span><span style="color: var(--penna-fg)">Split right</span><span>Labels, values, body text</span>
  <span>Muted</span><span><code>fg-muted</code></span><span style="color: var(--penna-fg-muted)">Split right</span><span>Descriptions, captions</span>
  <span>Faint</span><span><code>fg-faint</code></span><span style="color: var(--penna-fg-faint)">Split right</span><span>Placeholders, off</span>
</div>
```

```figure uses=label,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><label class="penna-input" data-penna="field"><input placeholder="Search computers" /></label></div><figcaption>Faint for a placeholder</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span style="color: var(--penna-fg-faint)">Keyboard moves to the next screen.</span></div><figcaption>Don't set text people need in faint</figcaption></figure>
</div>
```

## Highlight

Picked and focused are marked in the ink, and the main action is a solid fill of it, whatever the tint. The accent is a detail: a switch that is on, a checked box, a slider's fill, the dot that asks for attention, links. With the default tint the accent is the ink too.

```figure uses=switch,input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-toggle" role="switch" aria-checked="true" aria-label="On"></button><span class="docs-note">On</span></div>
  <div class="docs-cell"><label class="penna-input" data-penna="field" data-state="focused" style="width: 180px"><input value="git switch -c build" /></label><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Main action</span></div>
</div>
```

## Status

Red, amber and green mean wrong, look at this, and done. Always with an icon or a word.

```figure
<div class="docs-tokens">
  <span class="docs-head">Status</span><span class="docs-head">Token</span><span class="docs-head">Swatch</span><span class="docs-head">Use</span>
  <span>Danger</span><span><code>danger</code></span><span><i class="docs-swatch" style="background: var(--penna-danger)"></i></span><span>Failed, destructive</span>
  <span>Warning</span><span><code>warning</code></span><span><i class="docs-swatch" style="background: var(--penna-warning)"></i></span><span>Needs a look</span>
  <span>Success</span><span><code>success</code></span><span><i class="docs-swatch" style="background: var(--penna-success)"></i></span><span>Done, connected</span>
</div>
```

```figure uses=notice
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-notice" data-penna="notice" data-tone="danger" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="danger"><span class="penna-notice-lead"><i data-icon="circle-x"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Connection lost</span></div></div></div></div><figcaption>Icon and words with the color</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span style="color: var(--penna-danger)">Connection lost</span></div><figcaption>Don't rely on color alone</figcaption></figure>
</div>
```

Every text color reads at 4.5:1 or more on its ground, in every look, light and dark.

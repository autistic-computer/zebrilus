# Spacing

The closer two things relate, the closer they sit. Every element answers three questions with tokens: its height, its inset (owned by the container), and the gap to its neighbors (owned by the parent).

## Anatomy of a pane

Toggle the measurements below to inspect how insets and relationship gaps layer on a real penna pane without shifting a single pixel.

```figure uses=field,input,list,switch
<div class="docs-sp" data-docs-spacing data-xray>
  <div class="docs-sp-toolbar" data-penna="panel" data-ground>
    <div class="docs-sp-toggle-row">
      <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Show measurements" data-spacing-xray type="button"></button>
      <span>Show measurements</span>
    </div>
    <div class="docs-sp-legend">
      <span class="docs-sp-key"><i class="docs-sp-swatch" data-kind="gap" aria-hidden="true"></i><span>Gaps (parent)</span></span>
      <span class="docs-sp-key"><i class="docs-sp-swatch" data-kind="inset" aria-hidden="true"></i><span>Insets (container)</span></span>
    </div>
  </div>
  <div class="docs-sp-stage" data-spacing-stage>
    <div class="docs-sp-bar" data-sp-bar>
      <span class="docs-sp-bar-title">studio.local</span>
      <div class="docs-sp-bar-tools">
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Split"><i data-icon="panel-right"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Search"><i data-icon="search"></i></button>
      </div>
    </div>
    <div class="docs-sp-body" data-sp-body>
      <div class="penna-field" data-sp-field>
        <span class="penna-field-label" data-penna-text>Host</span>
        <label class="penna-input" data-penna="field"><input value="studio.local" aria-label="Host" /></label>
      </div>
      <div class="docs-sp-list" data-penna="card" data-ground data-sp-list role="listbox" aria-label="Sessions">
        <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" data-state="selected" aria-selected="true" data-sp-row>
          <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span>
        </div>
        <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" aria-selected="false" data-sp-row>
          <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span>
        </div>
      </div>
      <div class="docs-sp-actions" data-sp-actions>
        <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
    </div>
    <div class="docs-sp-overlay" data-spacing-overlay aria-hidden="true"></div>
  </div>
</div>
```

## Scale

Eleven steps in design pixels (1/16 rem), scaling with window zoom and interface text size. 2, 6 and 10 stay inside controls; layout uses 4, 8, 12 and 16 upward.

```figure
<div class="docs-sp">
  <div class="docs-tokens" data-bars>
    <span class="docs-head">Token</span><span class="docs-head">px</span><span class="docs-head">Use</span><span class="docs-head"></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-2">space-2</button></span><span><code>2</code></span><span>Inside controls, inset list rows</span><span><i class="docs-bar" data-scope="control" style="width: 2px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-4">space-4</button></span><span><code>4</code></span><span>Tight gap</span><span><i class="docs-bar" style="width: 4px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-6">space-6</button></span><span><code>6</code></span><span>Inside controls, chrome inset</span><span><i class="docs-bar" data-scope="control" style="width: 6px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-8">space-8</button></span><span><code>8</code></span><span>Related gap</span><span><i class="docs-bar" style="width: 8px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-10">space-10</button></span><span><code>10</code></span><span>Inside controls</span><span><i class="docs-bar" data-scope="control" style="width: 10px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-12">space-12</button></span><span><code>12</code></span><span>Card inset, paragraph gap</span><span><i class="docs-bar" style="width: 12px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-16">space-16</button></span><span><code>16</code></span><span>Group gap, pane inset</span><span><i class="docs-bar" style="width: 16px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-24">space-24</button></span><span><code>24</code></span><span>Heading above</span><span><i class="docs-bar" style="width: 24px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-32">space-32</button></span><span><code>32</code></span><span>Section gap, page inset</span><span><i class="docs-bar" style="width: 32px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-40">space-40</button></span><span><code>40</code></span><span>Page top</span><span><i class="docs-bar" style="width: 40px"></i></span>
    <span><button class="docs-clr-token" type="button" data-copy-token="--penna-space-64">space-64</button></span><span><code>64</code></span><span>Page bottom</span><span><i class="docs-bar" style="width: 64px"></i></span>
  </div>
  <div class="docs-sp-scale-note">
    <span class="docs-sp-scale-key"><i aria-hidden="true"></i><span>Layout tokens (4, 8, 12, 16+)</span></span>
    <span class="docs-sp-scale-key"><i data-scope="control" aria-hidden="true"></i><span>Inside controls (2, 6, 10)</span></span>
  </div>
</div>
```

Nothing off the scale. If a length is not listed here, fold to a neighbor or add a group.

## Gaps

Pick by relationship, not by eye. The parent sets the gap; components never carry an outer margin.

```figure
<div class="docs-tokens" data-bars>
  <span class="docs-head">Gap</span><span class="docs-head">px</span><span class="docs-head">Between</span><span class="docs-head"></span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-gap-tight">gap-tight</button></span><span><code>4</code></span><span>A label and what it names</span><span><i class="docs-bar" style="width: 4px"></i></span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-gap-related">gap-related</button></span><span><code>8</code></span><span>Siblings that work together</span><span><i class="docs-bar" style="width: 8px"></i></span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-gap-group">gap-group</button></span><span><code>16</code></span><span>Groups inside a pane</span><span><i class="docs-bar" style="width: 16px"></i></span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-gap-section">gap-section</button></span><span><code>32</code></span><span>Sections of a page</span><span><i class="docs-bar" style="width: 32px"></i></span>
</div>
```

```figure uses=field,input
<div class="docs-dodont">
  <figure data-verdict="do">
    <div class="docs-dodont-panel" data-penna="panel" data-ground>
      <i class="docs-mark" data-icon="circle-check"></i>
      <div class="docs-form" data-spacing="grouped">
        <div class="penna-field"><span class="penna-field-label" data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" aria-label="Do name" /></label></div>
        <div class="penna-field"><span class="penna-field-label" data-penna-text>Owner</span><label class="penna-input" data-penna="field"><input value="autistic-computer" aria-label="Do owner" /></label></div>
        <div class="docs-actions">
          <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Create</span></button>
        </div>
      </div>
    </div>
    <figcaption>4 tight, 16 between groups, 8 in actions</figcaption>
  </figure>
  <figure data-verdict="dont">
    <div class="docs-dodont-panel" data-penna="panel" data-ground>
      <i class="docs-mark" data-icon="circle-x"></i>
      <div class="docs-form" data-spacing="even">
        <div class="penna-field"><span class="penna-field-label" data-penna-text>Name</span><label class="penna-input" data-penna="field"><input value="penna" aria-label="Don't name" /></label></div>
        <div class="penna-field"><span class="penna-field-label" data-penna-text>Owner</span><label class="penna-input" data-penna="field"><input value="autistic-computer" aria-label="Don't owner" /></label></div>
        <div class="docs-actions">
          <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Create</span></button>
        </div>
      </div>
    </div>
    <figcaption>Don't space everything evenly at 12</figcaption>
  </figure>
</div>
```

## Insets

The container sets the space inside its edges. Insets reach the text's line box rather than its capitals, so `inset-card` (12) reads optically as roughly 16 to the letters. Keep the token; never manually nudge text padding.

```figure
<div class="docs-sp-insets">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <button class="docs-clr-token" type="button" data-copy-token="--penna-inset-chrome">inset-chrome</button>
      <span class="docs-pr-col-meta">6 px</span>
    </div>
    <div class="docs-sp-inset-box" data-pad="chrome">
      <div class="docs-sp-inset-inner">
        <span class="docs-sp-inset-title">Bars and pane headers</span>
        <span class="docs-sp-inset-sub">Around toolbar controls</span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <button class="docs-clr-token" type="button" data-copy-token="--penna-inset-card">inset-card</button>
      <span class="docs-pr-col-meta">12 px</span>
    </div>
    <div class="docs-sp-inset-box" data-pad="card">
      <div class="docs-sp-inset-inner">
        <span class="docs-sp-inset-title">Cards and popovers</span>
        <span class="docs-sp-inset-sub">Settings cards, notices, tooltips</span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <button class="docs-clr-token" type="button" data-copy-token="--penna-inset-pane">inset-pane</button>
      <span class="docs-pr-col-meta">16 px</span>
    </div>
    <div class="docs-sp-inset-box" data-pad="pane">
      <div class="docs-sp-inset-inner">
        <span class="docs-sp-inset-title">Panes and dialogs</span>
        <span class="docs-sp-inset-sub">Settings panes, modals, sheets</span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <button class="docs-clr-token" type="button" data-copy-token="--penna-inset-page">inset-page</button>
      <span class="docs-pr-col-meta">32 px</span>
    </div>
    <div class="docs-sp-inset-box" data-pad="page">
      <div class="docs-sp-inset-inner">
        <span class="docs-sp-inset-title">Document pages</span>
        <span class="docs-sp-inset-sub">Reading surfaces and docs</span>
      </div>
    </div>
  </div>
</div>
```

## Inset lists

Rows in a plain list touch (0) so row height carries the rhythm. In an inset list — menus, sidebars, pickers and settings navigation — rows sit `space-2` (2) apart inside `row-inset` (6), so rounded selection washes never merge.

```figure uses=list
<div class="docs-sp-list-compare">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Plain list</span>
      <span class="docs-pr-col-meta">0 inset · 0 gap</span>
    </div>
    <div class="docs-sp-plain-list" data-penna="panel" data-ground role="listbox" aria-label="Plain list">
      <div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>main.rs</span></span></span>
      </div>
      <div class="penna-list-item" data-penna="menu-item" role="option" data-state="hover" aria-selected="false">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>surface.rs</span></span></span>
      </div>
      <div class="penna-list-item" data-penna="menu-item" role="option" aria-selected="false">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>theme.rs</span></span></span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Inset list</span>
      <span class="docs-pr-col-meta">6 inset · 2 gap</span>
    </div>
    <div class="docs-sp-inset-list" data-penna="panel" data-ground role="listbox" aria-label="Inset list">
      <div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>main.rs</span></span></span>
      </div>
      <div class="penna-list-item" data-penna="menu-item" role="option" data-state="hover" aria-selected="false">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>surface.rs</span></span></span>
      </div>
      <div class="penna-list-item" data-penna="menu-item" role="option" aria-selected="false">
        <span class="penna-list-content"><span class="penna-list-label"><span class="penna-text" data-penna-text>theme.rs</span></span></span>
      </div>
    </div>
  </div>
</div>
```

## Layout primitives

Six primitives lay out screens with tokens and draw nothing of their own. In Rust, use `Space` (`Space::S8` is 8 px) rather than `ala`'s 4-pixel-step `gap_2()` helpers.

| Rust | HTML | Does |
| --- | --- | --- |
| `stack(gap)` | `.penna-stack[data-gap]` | Column of children, one gap apart |
| `inline(gap)` | `.penna-inline[data-gap]` | Row on one line, wraps with `data-wrap` |
| `inset(pad)` | `.penna-inset[data-inset]` | Container padding around one child |
| `split(w, gap, side, main)` | `.penna-split[data-gap]` | Fixed side beside flexible main |
| `center(measure)` | `.penna-center` | Centered column capped at a measure |
| `tiles(cols, gap)` | `.penna-tiles[data-gap]` | Fixed count of equal-width columns |

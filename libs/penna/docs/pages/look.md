# Look

penna draws one look. It starts from the platform's knobs, a person sets every knob in Appearance, and everything else stays the kit's.

## Kinds

Steps of fill carry the structure, so a dense screen holds few lines. Only what floats over content casts a shadow. A press replaces the hover wash, never stacks on it.

```figure uses=input,separator
<div class="docs-lk-kinds" data-docs-look>
  <div class="docs-lk-kind-row" data-head><span>Kind</span><span>Rest</span><span>Picked</span><span>Example</span></div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Button</span>
    <span class="docs-lk-kind-meta">A wash</span>
    <span class="docs-lk-kind-meta">A deeper wash</span>
    <span class="docs-lk-kind-ex"><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button></span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Item</span>
    <span class="docs-lk-kind-meta">No fill</span>
    <span class="docs-lk-kind-meta">The highlight</span>
    <span class="docs-lk-kind-ex" data-look-kind-group data-look-kind-attr="selected">
      <span class="penna-list docs-lk-kind-list"><span class="penna-list-item" data-penna="menu-item" role="option" aria-selected="false" data-look-kind-item><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></span></span>
      <span class="penna-list docs-lk-kind-list"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-look-kind-item><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span></span></span>
    </span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Chrome item</span>
    <span class="docs-lk-kind-meta">No fill</span>
    <span class="docs-lk-kind-meta">Glass</span>
    <span class="docs-lk-kind-ex" data-look-kind-group data-look-kind-attr="current">
      <button class="penna-button penna-sidebar-row docs-lk-kind-chrome" data-penna="sidebar.row" type="button" data-look-kind-item><span class="penna-label" data-penna-text>Spaces</span></button>
      <button class="penna-button penna-sidebar-row docs-lk-kind-chrome" data-penna="sidebar.row" type="button" data-state="current" aria-current="page" data-look-kind-item><span class="penna-label" data-penna-text>Look</span></button>
    </span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Field</span>
    <span class="docs-lk-kind-meta">A wash</span>
    <span class="docs-lk-kind-meta">Highlight, typing</span>
    <span class="docs-lk-kind-ex"><label class="penna-input docs-lk-kind-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label><label class="penna-input docs-lk-kind-input" data-penna="field" data-state="focused"><input value="studio" aria-label="Focused host" /></label></span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Card</span>
    <span class="docs-lk-kind-meta">A step of fill</span>
    <span class="docs-lk-kind-meta">A ring</span>
    <span class="docs-lk-kind-ex" data-look-kind-group data-look-kind-attr="selected">
      <button class="penna-card docs-lk-kind-card" data-penna="card" type="button" aria-selected="false" data-look-kind-item><span class="penna-type" data-type="small" data-penna-text>3 panes</span></button>
      <button class="penna-card docs-lk-kind-card" data-penna="card" type="button" data-state="selected" aria-selected="true" data-look-kind-item><span class="penna-type" data-type="small" data-penna-text>1 active</span></button>
    </span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Floating</span>
    <span class="docs-lk-kind-meta">Glass and shadow</span>
    <span class="docs-lk-kind-meta"></span>
    <span class="docs-lk-kind-ex"><span class="penna-tooltip docs-lk-tooltip" data-penna="tooltip" role="tooltip"><span data-penna-text>Split right</span></span></span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Label</span>
    <span class="docs-lk-kind-meta">Wash in its tone</span>
    <span class="docs-lk-kind-meta"></span>
    <span class="docs-lk-kind-ex"><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span></span>
  </div>
  <div class="docs-lk-kind-row">
    <span class="docs-lk-kind-name">Rule</span>
    <span class="docs-lk-kind-meta">A hairline</span>
    <span class="docs-lk-kind-meta"></span>
    <span class="docs-lk-kind-ex"><span class="penna-separator-rule docs-lk-kind-rule" role="separator"><span data-penna="rule.bar"></span></span></span>
  </div>
</div>
```

Hairlines mean something: a rule, an outline variant, a field in error. Increase contrast adds none.

```figure uses=input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><label class="penna-input" data-penna="field" style="width: 140px"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Connect</span></button></div><figcaption>Washes, no edges</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><label class="penna-input" data-penna="field.outline" style="width: 140px"><input placeholder="Host" aria-label="Host edged" /></label><button class="penna-button" data-penna="button.outline" type="button"><span class="penna-label" data-penna-text>Connect</span></button></div><figcaption>Don't edge every control</figcaption></figure>
</div>
```

## Tint

`tint` colors the details: a switch that is on, a checked box, a slider's fill, the dot that asks for attention, links. Selection and the main action stay in the ink with any tint. It is `blue` unless an app sets another; a person's accent replaces it.

```figure uses=check,slider looks=default,purple,pink,red,orange,yellow,green,graphite
<div class="docs-lk-stack" data-look-tint>
  <div class="docs-lk-tint-bar">
    <div class="docs-ov-swatches" role="group" aria-label="Accent">
      <span class="docs-ov-ring" data-selected><button class="docs-ov-swatch" type="button" aria-label="Blue" aria-pressed="true" data-look-accent="default"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Purple" aria-pressed="false" data-look-accent="purple"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Pink" aria-pressed="false" data-look-accent="pink"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Red" aria-pressed="false" data-look-accent="red"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Orange" aria-pressed="false" data-look-accent="orange"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Yellow" aria-pressed="false" data-look-accent="yellow"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Green" aria-pressed="false" data-look-accent="green"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Graphite" aria-pressed="false" data-look-accent="graphite"></button></span>
    </div>
    <span class="docs-lk-mono" data-look-tint-readout>blue · #3183ff</span>
  </div>
  <div class="docs-lk-tint-grid" data-look="default" data-look-tint-stage>
    <div class="docs-pr-col">
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Tinted details</span>
        <span class="docs-pr-col-meta">Takes accent</span>
      </div>
      <div class="docs-lk-panel" data-penna="panel" data-ground>
        <div class="docs-lk-tint-row">
          <span>Auto-reconnect</span>
          <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Auto-reconnect" type="button"></button>
        </div>
        <div class="docs-lk-tint-row">
          <label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore panes</span></label>
        </div>
        <div class="docs-lk-tint-row">
          <span>Opacity</span>
          <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="68" aria-label="Opacity">
            <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
          </div>
        </div>
        <div class="docs-lk-tint-meta">
          <span class="docs-lk-attn"><span class="docs-lk-dot"></span><span>3 unread</span></span>
          <a class="docs-lk-link" href="#what-a-person-sets">Appearance</a>
        </div>
      </div>
    </div>
    <div class="docs-pr-col">
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Always in the ink</span>
        <span class="docs-pr-col-meta">Stays neutral</span>
      </div>
      <div class="docs-lk-panel" data-penna="panel" data-ground>
        <div class="penna-list docs-lk-ink-list" role="listbox" aria-label="Sessions">
          <div class="penna-list-item" data-penna="menu-item" role="option" aria-selected="false" data-look-tint-row><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></div>
          <div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true" data-look-tint-row><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span></div>
        </div>
        <div class="docs-lk-ink-actions">
          <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Neutral

`neutral` tints the window, the text and every surface above them: a hue and an amount, warm 60 at 0.15 by default. Status colors and contrast hold at every value.

```figure looks=neutral-pure,default,neutral-cool
<div class="docs-lk-neutral-grid">
  <div class="docs-pr-col" data-look="neutral-pure">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Pure</span>
      <span class="docs-lk-mono">amount 0</span>
    </div>
    <div class="docs-lk-win">
      <div class="docs-lk-win-panel" data-penna="panel" data-ground>
        <div class="docs-lk-win-card" data-penna="card" data-ground>
          <span class="docs-lk-win-title">Workspace</span>
          <span class="docs-lk-win-sub">3 panes · 1 window</span>
        </div>
        <div class="docs-lk-win-foot">
          <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Ready</span></span>
          <button class="penna-button" data-penna="button" data-size="small" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Open</span></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="default">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Warm</span>
      <span class="docs-lk-mono">60° · 0.15</span>
    </div>
    <div class="docs-lk-win">
      <div class="docs-lk-win-panel" data-penna="panel" data-ground>
        <div class="docs-lk-win-card" data-penna="card" data-ground>
          <span class="docs-lk-win-title">Workspace</span>
          <span class="docs-lk-win-sub">3 panes · 1 window</span>
        </div>
        <div class="docs-lk-win-foot">
          <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Ready</span></span>
          <button class="penna-button" data-penna="button" data-size="small" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Open</span></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="neutral-cool">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Cool</span>
      <span class="docs-lk-mono">250° · 0.50</span>
    </div>
    <div class="docs-lk-win">
      <div class="docs-lk-win-panel" data-penna="panel" data-ground>
        <div class="docs-lk-win-card" data-penna="card" data-ground>
          <span class="docs-lk-win-title">Workspace</span>
          <span class="docs-lk-win-sub">3 panes · 1 window</span>
        </div>
        <div class="docs-lk-win-foot">
          <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Ready</span></span>
          <button class="penna-button" data-penna="button" data-size="small" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Open</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Colors

`colors` swaps the window, the text and the status colors for a theme's own, per mode, so an app can wear the colors of what it shows. Surfaces, washes and hairlines follow from them; accent and status colors move in lightness until they read at 4.5:1.

```figure looks=default,colors-slate,colors-warm
<div class="docs-lk-stack" data-look-colors>
  <div class="docs-lk-colors-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Base colors" data-look-colors-picker>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Default</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Slate</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Warm</span></span>
      </div>
    </div>
    <span class="docs-lk-mono" data-look-colors-meta>bg #1a1a19 · fg #e9e7e3</span>
  </div>
  <div class="docs-lk-colors-stage" data-look="default" data-look-colors-stage>
    <div class="docs-lk-colors-panel" data-penna="panel" data-ground>
      <div class="docs-lk-colors-inner">
        <div class="docs-lk-colors-badges">
          <span class="penna-badge" data-penna="badge" data-tone="success"><i data-icon="circle-check"></i><span class="badge-text" data-penna-text>Passing</span></span>
          <span class="penna-badge" data-penna="badge" data-tone="warning"><i data-icon="triangle-alert"></i><span class="badge-text" data-penna-text>Slow</span></span>
          <span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>Failed</span></span>
        </div>
        <div class="docs-row">
          <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Live" type="button"></button>
          <button class="penna-button" data-penna="button" data-size="small" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Deploy</span></button>
        </div>
      </div>
      <div class="docs-lk-colors-foot">
        <span data-look-colors-note>Default kit palette; neutral warm 60 at 0.15.</span>
        <span class="docs-lk-win-swatches" aria-hidden="true"><i style="background: var(--penna-bg)"></i><i style="background: var(--penna-panel)"></i><i style="background: var(--penna-popover)"></i><i style="background: var(--penna-fg)"></i></span>
      </div>
    </div>
  </div>
</div>
```

## Corners

Every corner follows Corner radius. A control (button, field, tab, chip, badge or row) rounds to a share of its own height, so a small badge and a tall sidebar entry read equally round: 31% of the height at the default, 45% at 100%. Containers (cards, panels, popovers, dialogs, panes) keep `radius` in pixels, a pane no rounder than 16. A corner inside another is concentric, like a segmented control's pill in its track; a menu's rows keep their control corner, never rounder than the menu, and only `full` makes pills.

```figure uses=input looks=radius-4,default,radius-20,pill
<div class="docs-lk-corners-grid">
  <div class="docs-pr-col" data-look="radius-4">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title"><code>radius</code> 4</span>
      <span class="docs-lk-mono">card 4 · controls 18%</span>
    </div>
    <div class="penna-card docs-lk-corner-card" data-penna="card" data-ground>
      <div class="docs-lk-corner-row">
        <label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host at radius 4" /></label>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
      <div class="docs-lk-corner-sub">
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Range at radius 4">
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Day</span></span>
            <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Week</span></span>
          </div>
        </div>
        <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="default">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title"><code>radius</code> 12</span>
      <span class="docs-lk-mono">default · controls 31%</span>
    </div>
    <div class="penna-card docs-lk-corner-card" data-penna="card" data-ground>
      <div class="docs-lk-corner-row">
        <label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host at radius 12" /></label>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
      <div class="docs-lk-corner-sub">
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Range at radius 12">
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Day</span></span>
            <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Week</span></span>
          </div>
        </div>
        <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="radius-20">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title"><code>radius</code> 20</span>
      <span class="docs-lk-mono">card 20 · controls 40%</span>
    </div>
    <div class="penna-card docs-lk-corner-card" data-penna="card" data-ground>
      <div class="docs-lk-corner-row">
        <label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host at radius 20" /></label>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
      <div class="docs-lk-corner-sub">
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Range at radius 20">
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Day</span></span>
            <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Week</span></span>
          </div>
        </div>
        <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="pill">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title"><code>control-radius</code> full</span>
      <span class="docs-lk-mono">card 12 · controls pill</span>
    </div>
    <div class="penna-card docs-lk-corner-card" data-penna="card" data-ground>
      <div class="docs-lk-corner-row">
        <label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host with pill controls" /></label>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
      <div class="docs-lk-corner-sub">
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Range with pill controls">
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Day</span></span>
            <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Week</span></span>
          </div>
        </div>
        <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span>
      </div>
    </div>
  </div>
</div>
```

## Smoothing

`smoothing` shapes every corner, from round arcs at 2 to a squircle at 5, without changing how round it looks: a smoother corner is drawn in a larger box, so its middle sits where a round arc of the same radius would.

```figure looks=smoothing-2,smoothing-3,smoothing-4,smoothing-5
<div class="docs-lk-stack" data-look-smooth>
  <div class="docs-lk-smooth-grid">
    <button class="docs-lk-smooth-btn" type="button" data-look="smoothing-2" data-look-smooth-card="2">
      <span class="penna-card docs-lk-smooth-box" data-penna="card"><span class="docs-lk-smooth-num">2</span><span class="docs-lk-smooth-grow">1.00×</span></span>
      <span class="docs-lk-mono">round arc</span>
    </button>
    <button class="docs-lk-smooth-btn" type="button" data-look="smoothing-3" data-look-smooth-card="3">
      <span class="penna-card docs-lk-smooth-box" data-penna="card"><span class="docs-lk-smooth-num">3</span><span class="docs-lk-smooth-grow">1.42×</span></span>
      <span class="docs-lk-mono">default</span>
    </button>
    <button class="docs-lk-smooth-btn" type="button" data-look="smoothing-4" data-look-smooth-card="4">
      <span class="penna-card docs-lk-smooth-box" data-penna="card"><span class="docs-lk-smooth-num">4</span><span class="docs-lk-smooth-grow">1.84×</span></span>
      <span class="docs-lk-mono">Apple</span>
    </button>
    <button class="docs-lk-smooth-btn" type="button" data-look="smoothing-5" data-look-smooth-card="5">
      <span class="penna-card docs-lk-smooth-box" data-penna="card"><span class="docs-lk-smooth-num">5</span><span class="docs-lk-smooth-grow">2.26×</span></span>
      <span class="docs-lk-mono">squircle</span>
    </button>
  </div>
  <div class="docs-lk-smooth-diagram" data-penna="panel" data-ground>
    <div class="docs-lk-smooth-head">
      <span class="docs-pr-col-title">Corner geometry at <code>radius</code> 24</span>
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Highlight smoothing exponent" data-look-smooth-seg>
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>All</span></span>
          <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>2</span></span>
          <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>3</span></span>
          <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>4</span></span>
          <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>5</span></span>
        </div>
      </div>
    </div>
    <div class="docs-lk-smooth-canvas">
      <svg class="docs-lk-smooth-svg" viewBox="0 0 260 184" role="img" aria-label="Superellipse curves for smoothing 2, 3, 4 and 5 meeting at one midpoint" data-look-smooth-svg></svg>
      <div class="docs-lk-smooth-legend">
        <p class="docs-lk-smooth-copy" data-look-smooth-note>All four pass through the same 45° midpoint; smoother corners start earlier along each edge.</p>
        <span class="docs-lk-mono">Cards above are 112 px tall so smoothing 5 draws unclamped at 54.3 px.</span>
      </div>
    </div>
  </div>
</div>
```

## Glass

Glass is for what sits over something: what floats over the content (menus, popovers, tooltips, toasts, a button over a transcript, the composer), panes when they have gaps, and the selected row or tab in a sidebar or title bar. It is a switch: on, `liquid` blurs and tones what is under it with a perimeter rim tuned separately for light and dark; off, `none`, draws the popover fill.

```figure uses=menu looks=glass-none,glass-liquid
<div class="docs-lk-glass-grid">
  <div class="docs-pr-col" data-look="glass-none">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Glass off</span>
      <span class="docs-lk-mono">none · popover + hairline</span>
    </div>
    <div class="docs-lk-glass-stage">
      <div class="docs-lk-glass-under" aria-hidden="true">
        <span class="docs-glass-text"><b>fn</b> split(pane: Pane) {<br />&nbsp;&nbsp;<i>let</i> half = pane.width / 2;<br />&nbsp;&nbsp;tabs.push(<em>"logs"</em>);<br />}</span>
        <div class="docs-lk-glass-bars"><i></i><i></i><i></i></div>
      </div>
      <div class="penna-popup docs-lk-menu" data-penna="popover" data-ground>
        <div class="penna-popup-items" role="menu">
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div>
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div>
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="glass-liquid">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Liquid glass</span>
      <span class="docs-lk-mono">liquid · blur + rim</span>
    </div>
    <div class="docs-lk-glass-stage">
      <div class="docs-lk-glass-under" aria-hidden="true">
        <span class="docs-glass-text"><b>fn</b> split(pane: Pane) {<br />&nbsp;&nbsp;<i>let</i> half = pane.width / 2;<br />&nbsp;&nbsp;tabs.push(<em>"logs"</em>);<br />}</span>
        <div class="docs-lk-glass-bars"><i></i><i></i><i></i></div>
      </div>
      <div class="penna-popup docs-lk-menu" data-penna="popover" data-ground>
        <div class="penna-popup-items" role="menu">
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div>
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span></span></div>
          <div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div>
        </div>
      </div>
    </div>
  </div>
</div>
```

Text on any glass reads at 4.5:1 over the worst content, and `prefers-reduced-transparency` draws it as the popover fill. Where the renderer can't bend light, `liquid` keeps its blur, tint and rim and drops the lens. Glass is neutral: a floating chip with a tone draws its tone opaque. A selected sidebar row or tab reads as strong as the highlight it replaces, and a pane with gaps, or a PiP floating over the panes, keeps a hairline edge wherever its glass draws no rim. With glass off a PiP is its pane's own color, opaque.

```figure uses=menu looks=glass-liquid
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="docs-lk-dodont-glass" data-look="glass-liquid"><div class="docs-lk-glass-under" aria-hidden="true"><span class="docs-glass-text"><b>fn</b> split(pane: Pane) {<br />&nbsp;&nbsp;tabs.push(<em>"logs"</em>);</span><div class="docs-lk-glass-bars"><i></i><i></i><i></i></div></div><div class="penna-popup docs-lk-menu" data-penna="popover" data-ground><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div></div><figcaption>Glass on what floats over content</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-card docs-lk-dodont-card-bad" data-penna="card"><span class="penna-type" data-type="small" data-penna-text>build · cargo test</span></div></div><figcaption>Don't make a card, or a control in content, glass</figcaption></figure>
</div>
```

## What a person sets

The same in every app, in Appearance, in three groups: Look, Size and Accessibility. There are no presets to pick: every knob is a row, starting at the platform's value.

| Knob | Does |
| --- | --- |
| Mode | System, light or dark, unless a theme locks it |
| Accent | The tint: a named accent or a color |
| Tint, Tint strength | The neutral's hue and amount |
| Corner radius | The radius, 0 to 100% on a square curve: 69% is the default 12, 100% is 25 and never a pill. None on a phone |
| Controls | Rounded follows the radius; Pill makes controls pills |
| Glass | A switch: on draws liquid glass, off draws none |
| UI font | The face |
| Mono font | The code face; its list opens on Monospaced |
| Text size | The root size; everything scales with it. None on a phone, which follows the device from 100% to 135% |
| Density | Compact, regular or comfortable rows and controls. None on a phone, which is comfortable |
| Increase contrast | 0 to 100%: surfaces and fills further apart, muted text closer to the ink; no edges |
| Motion | Off, default, or fast (every animation at half its time) |
| Pointing hand | Shows a pointing hand or the default arrow on clickable items; desktop only |

## The kit's theme files

The look starts from `macos` on Apple platforms and `default` elsewhere. `yaru` is a third look the storybook and the tests draw. None is a choice a person makes.

| Theme file | Tint | Radius | Control radius | Smoothing | Glass |
| --- | --- | --- | --- | --- | --- |
| `default` | blue | 12 | 12 | 3 | liquid |
| `macos` | blue | 12 | full | 4 | liquid |
| `yaru` | orange | 8 | 8 | 2 | none |

## Theme builder

Dial any combination of knobs below to preview the resulting surface tokens live and export a ready-to-import `theme.json` or shareable URL.

```figure uses=badge,button,input,menu,segmented,slider,switch looks=default,neutral-pure,neutral-cool,glass-none,glass-liquid
<div class="docs-lk-builder" data-look-builder>
  <div class="docs-lk-builder-grid">
    <div class="docs-lk-builder-knobs" data-penna="settings.card" data-ground>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Name</span>
        <label class="penna-input docs-lk-b-name" data-penna="field" data-size="small"><input type="text" value="Studio" aria-label="Theme name" data-lb-name /></label>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Tint</span>
        <div class="docs-ov-swatches" role="group" aria-label="Builder accent">
          <span class="docs-ov-ring" data-selected><button class="docs-ov-swatch" type="button" aria-label="Blue" data-lb-tint="blue"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Purple" data-lb-tint="purple"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Pink" data-lb-tint="pink"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Red" data-lb-tint="red"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Orange" data-lb-tint="orange"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Yellow" data-lb-tint="yellow"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Green" data-lb-tint="green"></button></span>
          <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" aria-label="Graphite" data-lb-tint="graphite"></button></span>
        </div>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Neutral</span>
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" role="radiogroup" aria-label="Builder neutral" data-lb-neutral>
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Pure</span></span>
            <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Warm</span></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Cool</span></span>
          </div>
        </div>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Radius</span>
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" role="radiogroup" aria-label="Builder radius" data-lb-radius>
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>4</span></span>
            <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>12</span></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>20</span></span>
          </div>
        </div>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Controls</span>
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Builder controls" data-lb-controls>
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Rounded</span></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Pill</span></span>
          </div>
        </div>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Smoothing</span>
        <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" role="radiogroup" aria-label="Builder smoothing" data-lb-smooth>
          <div class="penna-segments" data-penna="button.quiet" data-state="current">
            <span class="penna-segment-pill"></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>2</span></span>
            <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>3</span></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>4</span></span>
            <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>5</span></span>
          </div>
        </div>
      </div>
      <div class="docs-lk-b-row">
        <span class="docs-ov-knob-label">Liquid glass</span>
        <button class="penna-toggle" role="switch" data-size="small" aria-checked="true" aria-label="Builder liquid glass" type="button" data-lb-glass></button>
      </div>
    </div>

    <div class="docs-lk-builder-out">
      <div class="docs-lk-b-preview-scope" data-look="default" data-lb-stage>
        <div class="docs-lk-b-stage" data-penna="panel" data-ground>
          <div class="docs-lk-b-card" data-penna="card" data-ground>
            <div class="docs-row">
              <span class="penna-type" data-type="small" data-strong data-lb-preview-name>Studio</span>
              <span class="docs-spacer"></span>
              <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text">Live</span></span>
            </div>
            <div class="docs-row">
              <label class="penna-input" data-penna="field" data-size="small" style="flex: 1; min-width: 0"><input value="studio.local" aria-label="Preview host" /></label>
              <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" type="button"><span class="penna-label">Save</span></button>
            </div>
            <div class="docs-row">
              <button class="penna-toggle" role="switch" data-size="small" aria-checked="true" aria-label="Preview toggle" type="button"></button>
              <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="72" style="flex: 1" aria-label="Preview slider">
                <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
              </div>
            </div>
          </div>
          <div class="docs-lk-b-glass-wrap" data-look="glass-liquid" data-lb-glass-wrap>
            <div class="docs-lk-b-float" data-penna="popover" data-ground>
              <span class="penna-type" data-type="caption" data-strong>Floating surface</span>
              <span class="penna-type" data-type="caption" data-muted data-lb-glass-note>liquid · blur + rim</span>
            </div>
          </div>
        </div>
      </div>

      <div class="docs-lk-b-code-wrap">
        <pre class="docs-lk-b-code" data-penna="card.code" data-ground><code data-lb-json></code></pre>
        <div class="docs-lk-b-actions">
          <button class="penna-button" data-penna="button" data-size="small" type="button" data-lb-copy><i data-icon="copy"></i><span class="penna-label" data-penna-text>Copy theme.json</span></button>
          <button class="penna-button" data-penna="button.quiet" data-size="small" type="button" data-lb-share><i data-icon="external-link"></i><span class="penna-label" data-penna-text>Copy URL</span></button>
          <button class="penna-button" data-penna="button.quiet" data-size="small" type="button" data-lb-reset><span class="penna-label" data-penna-text>Reset</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Theme file

A theme file holds only knobs. Keys it leaves out keep their defaults; keys it doesn't know are ignored. An app that accepts a file through `AppearanceChange::Import` sets every knob a person has from it.

```json example
{
  "name": "Studio",
  "tint": "pink",
  "neutral": { "hue": 250, "amount": 0.3 },
  "radius": 8,
  "control-radius": "full",
  "smoothing": 4,
  "glass": "none"
}
```

```rust example
let studio = interface_theme::parse_theme(include_str!("studio.json"))?;
studio.activate(None, Some(window), cx);
```

## API

| Theme file | Rust | Does |
| --- | --- | --- |
| `name` | `.name` | A label for the file |
| `tint` | `.tint` | An accent, a hex color, or a light and dark pair |
| `neutral` | `.neutral` | Hue 0 to 360, amount 0 to 1 |
| `radius` | `.radius` | Container corners, 0 to 25; 25 is round, not a pill |
| `control-radius` | `.control_radius` | Control corners, 0 to 25 or full; the radius when left out |
| `smoothing` | `.smoothing` | Corner shape, 2 to 5 |
| `glass` | `.glass` | None, liquid or a material; liquid when left out |
| `face` | `.face` | Interface face; the platform's when left out |
| `mode` | `.mode` | Follow, light or dark; beats the person's Mode |
| `colors` | `.colors` | `bg`, `fg`, `success`, `warning`, `danger` per mode; the Color table when left out |
| | `parse_theme(json)` | Reads a theme file |
| | `.activate(mode, window, cx)` | Draws the app in it |

The HTML twin reads the same knobs: `cargo run --example theme_css` writes its tokens.

## Live token inspection

Press `F12` (or `Alt+Shift+I`, or pick **Token inspector** from `⌘K`) on any docs page to open the live Token Inspector HUD. Hovering a surface highlights its bounds and prints its `data-penna` role, enclosing ground, computed `--penna-fill`, `--penna-ink`, radius, smoothing, glass, and tone; clicking pins the surface and copies the JSON snapshot.

In Rust apps, `TokenSnapshot::capture(cx)` captures the active look, density, contrast, data palette, and drawn surface roles as JSON, and `handle_inspect_line(ndjson, cx)` processes NDJSON inspection and live look-switching commands (`snapshot`, `roles`, `set-mode`, `set-density`, `set-motion`, `set-contrast`). Secondary window chrome can dim when unfocused with `.dim_when_inactive(window)` (`INACTIVE_WINDOW_OPACITY`).

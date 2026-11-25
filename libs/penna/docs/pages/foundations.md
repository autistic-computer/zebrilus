# Overview

Every surface, type run, gap and corner in penna derives from one set of tokens. Change a knob once and native `ala` windows, phone sheets and HTML pages follow together.

## One system, live

Try the four knobs below. Accent colors details while primary actions and selection stay neutral ink; corners reshape every surface with superellipse math; density scales control heights; glass switches floating surfaces between blurred tint and solid fill.

```figure uses=badge,input,list,slider,switch looks=default,radius-4,radius-20,pill,glass-none,glass-liquid
<div class="docs-ov" data-docs-overview>
  <div class="docs-ov-scope" data-look="default" data-overview-stage>
  <div class="docs-ov-stage" data-penna="panel" data-ground>
    <div class="docs-ov-card" data-penna="card" data-ground>
      <div class="docs-ov-head">
        <div class="docs-ov-title-group">
          <span class="docs-ov-title">Workspace</span>
          <span class="docs-ov-sub">3 panes · 1 window</span>
        </div>
        <div class="docs-ov-badges">
          <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Running</span></span>
          <span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span>
        </div>
      </div>

      <div class="docs-ov-controls">
        <label class="penna-input" data-penna="field"><input value="studio.local" aria-label="Host" /></label>
        <div class="docs-ov-btns">
          <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Connect</span></button>
        </div>
      </div>

      <div class="docs-ov-split">
        <div class="docs-ov-list" role="listbox" aria-label="Sessions">
          <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" aria-selected="false" data-overview-row>
            <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span>
          </div>
          <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" data-state="selected" aria-selected="true" data-overview-row>
            <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span>
          </div>
        </div>
        <div class="docs-ov-details">
          <div class="docs-ov-detail-row">
            <span>Auto-reconnect</span>
            <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Auto-reconnect"></button>
          </div>
          <div class="docs-ov-detail-row">
            <span>Opacity</span>
            <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="72" aria-label="Opacity">
              <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
            </div>
          </div>
          <span class="docs-ov-accent-note">Accent tints <a href="color.html">links</a>, switches and rails.</span>
        </div>
      </div>
    </div>

    <div class="docs-ov-glass-strip" data-penna="card">
      <div class="docs-ov-bg" aria-hidden="true">
        <div class="docs-ov-bg-code">
          <span><b>fn</b> connect(host: &amp;str, tint: Accent) -&gt; Session {</span>
          <span>&nbsp;&nbsp;<i>let</i> surface = theme.layer(Surface::popover(), PaintState::Rest);</span>
          <span>&nbsp;&nbsp;Session::open(host, <em>"studio.local"</em>, tint, surface)</span>
        </div>
        <div class="docs-ov-bg-bars"><i></i><i></i><i></i><i></i></div>
      </div>
      <div class="docs-ov-float-anchor" data-look="glass-liquid" data-overview-float-wrap>
        <div class="docs-ov-float" data-penna="popover" data-ground>
          <div class="docs-ov-float-head">
            <span class="docs-ov-float-dot" aria-hidden="true"></span>
            <span class="docs-ov-float-title" data-overview-float-label>Liquid glass</span>
          </div>
          <span class="docs-ov-float-sub" data-overview-float-sub>Blur, tint and perimeter rim</span>
        </div>
      </div>
    </div>
  </div>
  </div>

  <div class="docs-ov-knobs" data-penna="settings.card" data-ground>
    <div class="docs-ov-knob">
      <span class="docs-ov-knob-label">Accent</span>
      <div class="docs-ov-swatches" role="group" aria-label="Accent">
        <span class="docs-ov-ring" data-selected><button class="docs-ov-swatch" type="button" data-overview-accent="blue" aria-label="Blue" aria-pressed="true"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="purple" aria-label="Purple" aria-pressed="false"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="pink" aria-label="Pink" aria-pressed="false"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="red" aria-label="Red" aria-pressed="false"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="orange" aria-label="Orange" aria-pressed="false"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="green" aria-label="Green" aria-pressed="false"></button></span>
        <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-overview-accent="graphite" aria-label="Graphite" aria-pressed="false"></button></span>
      </div>
    </div>
    <div class="docs-ov-knob">
      <span class="docs-ov-knob-label">Corners</span>
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" data-overview-corners role="radiogroup" aria-label="Corners">
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>4px</span></span>
          <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>12px</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>20px</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Pill</span></span>
        </div>
      </div>
    </div>
    <div class="docs-ov-knob">
      <span class="docs-ov-knob-label">Density</span>
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" data-overview-density role="radiogroup" aria-label="Density">
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Compact</span></span>
          <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Regular</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Comfort</span></span>
        </div>
      </div>
    </div>
    <div class="docs-ov-knob">
      <span class="docs-ov-knob-label">Floating glass</span>
      <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Floating glass" data-overview-glass></button>
    </div>
  </div>
</div>
```

## How tokens flow

```figure
<div class="docs-pipeline" data-penna="card" data-ground>
  <div class="docs-pipe-step">
    <span class="docs-pipe-name">Knobs</span>
    <span class="docs-pipe-sub">Mode, accent, corners, glass</span>
  </div>
  <i class="docs-pipe-arrow" data-icon="arrow-right" aria-hidden="true"></i>
  <div class="docs-pipe-step">
    <span class="docs-pipe-name">Tokens</span>
    <span class="docs-pipe-sub">Contrast-checked fills and rims</span>
  </div>
  <i class="docs-pipe-arrow" data-icon="arrow-right" aria-hidden="true"></i>
  <div class="docs-pipe-step">
    <span class="docs-pipe-name">Surfaces</span>
    <span class="docs-pipe-sub">Named roles, never raw colors</span>
  </div>
</div>
```

## Guides

```figure
<div class="docs-guides">
  <a class="docs-guide-row" href="principles.html">
    <span class="docs-guide-name">Principles</span>
    <span class="docs-guide-viz"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></span>
    <span class="docs-guide-copy">Six rules every screen follows, from keyboard shortcuts to one highlight.</span>
  </a>
  <a class="docs-guide-row" href="color.html">
    <span class="docs-guide-name">Color</span>
    <span class="docs-guide-viz"><i class="docs-viz-chip" style="background: var(--penna-bg)"></i><i class="docs-viz-chip" style="background: var(--penna-panel)"></i><i class="docs-viz-chip" style="background: var(--penna-accent)"></i><i class="docs-viz-chip" style="background: var(--penna-danger)"></i></span>
    <span class="docs-guide-copy">Neutral surface steps, ink contrast, the accent and semantic status tones.</span>
  </a>
  <a class="docs-guide-row" href="typography.html">
    <span class="docs-guide-name">Typography</span>
    <span class="docs-guide-viz"><span class="docs-viz-type"><span class="docs-viz-aa">Aa</span><span class="docs-viz-mono">13 / 18</span></span></span>
    <span class="docs-guide-copy">Interface, reading and touch scales across Inter and CommitMono.</span>
  </a>
  <a class="docs-guide-row" href="spacing.html">
    <span class="docs-guide-name">Spacing</span>
    <span class="docs-guide-viz"><span class="docs-viz-space"><i style="width: 4px"></i><i style="width: 8px"></i><i style="width: 14px"></i><i style="width: 22px"></i></span></span>
    <span class="docs-guide-copy">Four semantic gaps and four container insets on one rem scale.</span>
  </a>
  <a class="docs-guide-row" href="sizes.html">
    <span class="docs-guide-name">Sizes</span>
    <span class="docs-guide-viz"><span class="docs-viz-rungs"><i style="height: 8px"></i><i style="height: 12px"></i><i style="height: 16px"></i><i style="height: 20px"></i></span></span>
    <span class="docs-guide-copy">Seven control rungs from 20px chips to 56px touch controls, plus density.</span>
  </a>
  <a class="docs-guide-row" href="motion.html">
    <span class="docs-guide-name">Motion</span>
    <span class="docs-guide-viz"><span class="docs-viz-pill">160 ms</span></span>
    <span class="docs-guide-copy">Surface presence, disclosure turns, speed modes, and reduced motion.</span>
  </a>
  <a class="docs-guide-row" href="phone.html">
    <span class="docs-guide-name">Phone</span>
    <span class="docs-guide-viz"><span class="docs-viz-pill">44px</span></span>
    <span class="docs-guide-copy">Coarse-pointer targets, floating sheets, stack menus and edge gestures.</span>
  </a>
  <a class="docs-guide-row" href="look.html">
    <span class="docs-guide-name">Look</span>
    <span class="docs-guide-viz"><span class="docs-viz-glass"></span></span>
    <span class="docs-guide-copy">Accent tint, neutral warmth, superellipse smoothing and liquid glass.</span>
  </a>
</div>
```

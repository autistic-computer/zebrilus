# Color

Color separates surfaces, marks what is on, and warns. Never decoration.

## Surfaces

Each layer steps away from the one under it: darker in light mode, lighter in dark. Steps of fill separate surfaces — panels, cards and controls draw no shadow.

```figure
<div class="docs-clr-stack" data-docs-color>
  <div class="docs-clr-layer-head">
    <span class="docs-clr-layer-name">Window</span>
    <span class="docs-clr-layer-meta">
      <button class="docs-clr-token" type="button" data-copy-token="--penna-bg">--penna-bg</button>
      <span class="docs-clr-hex" data-color-hex="--penna-bg"></span>
    </span>
  </div>
  <div class="docs-clr-layer" data-penna="panel" data-ground>
    <div class="docs-clr-layer-head">
      <span class="docs-clr-layer-name">Panel</span>
      <span class="docs-clr-layer-meta">
        <button class="docs-clr-token" type="button" data-copy-token="--penna-panel">--penna-panel</button>
        <span class="docs-clr-hex" data-color-hex="--penna-panel"></span>
      </span>
    </div>
    <div class="docs-clr-layer" data-penna="card" data-ground data-color-source>
      <div class="docs-clr-layer-head">
        <span class="docs-clr-layer-name">Card on panel</span>
        <span class="docs-clr-layer-meta">
          <button class="docs-clr-token" type="button" data-copy-token="data-penna=&quot;card&quot;">card</button>
          <span class="docs-clr-hex" data-color-hex="--penna-fill"></span>
        </span>
      </div>
      <div class="docs-clr-popover" data-penna="popover" data-ground>
        <span class="docs-clr-layer-name">Popover</span>
        <span class="docs-clr-layer-meta">
          <button class="docs-clr-token" type="button" data-copy-token="--penna-popover">--penna-popover</button>
          <span class="docs-clr-hex" data-color-hex="--penna-popover"></span>
        </span>
      </div>
    </div>
  </div>
</div>
```

```figure
<div class="docs-tokens">
  <span class="docs-head">Role</span><span class="docs-head">Token</span><span class="docs-head">Swatch</span><span class="docs-head">Use</span>
  <span>Window</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-bg">bg</button></span><span class="docs-clr-swatch-cell"><i class="docs-swatch" style="background: var(--penna-bg)"></i><span class="docs-clr-hex" data-color-hex="--penna-bg"></span></span><span>Window and page</span>
  <span>Panel</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-panel">panel</button></span><span class="docs-clr-swatch-cell"><i class="docs-swatch" style="background: var(--penna-panel)"></i><span class="docs-clr-hex" data-color-hex="--penna-panel"></span></span><span>Sidebars and side panels</span>
  <span>Popover</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-popover">popover</button></span><span class="docs-clr-swatch-cell"><i class="docs-swatch" style="background: var(--penna-popover)"></i><span class="docs-clr-hex" data-color-hex="--penna-popover"></span></span><span>Menus, popovers and glass floor</span>
  <span>Divider</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-divider">divider</button></span><span class="docs-clr-swatch-cell"><i class="docs-swatch" style="background: linear-gradient(var(--penna-divider), var(--penna-divider)) center / 100% 1px no-repeat"></i><span class="docs-clr-hex">fg · 8%</span></span><span>Rules and hairlines</span>
</div>
```

On a blurred window each surface takes its step as a wash over the window, so the layers stay a step apart and the window stays see-through. Muted and faint text there are alpha shares of the ink, so they adopt the desktop's hue while holding contrast.

## Ink

Full ink for what people read, muted for what supports it, faint only for placeholders and disabled controls.

```figure
<div class="docs-tokens">
  <span class="docs-head">Ink</span><span class="docs-head">Token</span><span class="docs-head">Sample</span><span class="docs-head">Contrast &amp; use</span>
  <span>Full</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-fg">fg</button></span><span style="color: var(--penna-fg)">Split right</span><span class="docs-clr-meta-cell"><span class="docs-clr-hex" data-color-hex="--penna-fg"></span><span class="docs-clr-ratio" data-color-ratio="--penna-fg" data-on="--penna-bg"></span><span class="docs-clr-use">Labels, values, body</span></span>
  <span>Muted</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-fg-muted">fg-muted</button></span><span style="color: var(--penna-fg-muted)">Split right</span><span class="docs-clr-meta-cell"><span class="docs-clr-hex" data-color-hex="--penna-fg-muted"></span><span class="docs-clr-ratio" data-color-ratio="--penna-fg-muted" data-on="--penna-bg"></span><span class="docs-clr-use">Descriptions, captions</span></span>
  <span>Ghost</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-muted">muted</button></span><span style="color: var(--penna-muted)">Split right</span><span class="docs-clr-meta-cell"><span class="docs-clr-hex" data-color-hex="--penna-muted"></span><span class="docs-clr-ratio" data-color-ratio="--penna-muted" data-on="--penna-bg"></span><span class="docs-clr-use">Muted labels in surfaces</span></span>
  <span>Faint</span><span><button class="docs-clr-token" type="button" data-copy-token="--penna-fg-faint">fg-faint</button></span><span style="color: var(--penna-fg-faint)">Split right</span><span class="docs-clr-meta-cell"><span class="docs-clr-hex" data-color-hex="--penna-fg-faint"></span><span class="docs-clr-ratio" data-color-ratio="--penna-fg-faint" data-on="--penna-bg"></span><span class="docs-clr-use">Placeholders, off</span></span>
</div>
```

```figure uses=label,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><label class="penna-input" data-penna="field"><input placeholder="Search computers" aria-label="Search computers" /></label></div><figcaption>Faint for a placeholder</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><span style="color: var(--penna-fg-faint)">Keyboard moves to the next screen.</span></div><figcaption>Don't set text people need in faint</figcaption></figure>
</div>
```

## Highlight and accent

Picked, focused and the main action stay in the ink itself, whatever the tint. The accent only colors details: a switch that is on, a checked box, a slider's fill, the dot that asks for attention, and links.

```figure uses=field,input,switch,check,slider
<div class="docs-clr-accent">
  <div class="docs-clr-accent-bar" data-penna="panel" data-ground>
    <div class="docs-ov-swatches" role="group" aria-label="Named accents">
      <span class="docs-ov-ring" data-selected><button class="docs-ov-swatch" type="button" data-color-accent="blue" aria-label="Blue" aria-pressed="true"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="purple" aria-label="Purple" aria-pressed="false"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="pink" aria-label="Pink" aria-pressed="false"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="red" aria-label="Red" aria-pressed="false"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="orange" aria-label="Orange" aria-pressed="false"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="green" aria-label="Green" aria-pressed="false"></button></span>
      <span class="docs-ov-ring"><button class="docs-ov-swatch" type="button" data-color-accent="graphite" aria-label="Graphite" aria-pressed="false"></button></span>
    </div>
    <div class="docs-clr-accent-info">
      <span class="docs-clr-accent-name" data-color-accent-name>Blue</span>
      <span class="docs-clr-hex" data-color-accent-hex></span>
      <span class="docs-clr-ratio" data-color-accent-ratio></span>
    </div>
  </div>
  <div class="docs-clr-accent-stage" data-color-accent-stage>
    <div class="docs-pr-split">
      <div class="docs-pr-col">
        <div class="docs-pr-col-head">
          <span class="docs-pr-col-title">Accent</span>
          <span class="docs-pr-col-meta">Tints details only</span>
        </div>
        <div class="docs-pr-card" data-penna="panel" data-ground>
          <div class="docs-pr-rows">
            <div class="docs-pr-detail-row">
              <span>On</span>
              <button class="penna-toggle" role="switch" aria-checked="true" aria-label="On" type="button"></button>
            </div>
            <div class="docs-pr-detail-row">
              <span>Opacity</span>
              <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="68" aria-label="Opacity">
                <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
              </div>
            </div>
            <div class="docs-pr-detail-row">
              <label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Checked</span></label>
              <a class="docs-pr-link" href="look.html">Look knobs</a>
            </div>
          </div>
        </div>
      </div>
      <div class="docs-pr-col">
        <div class="docs-pr-col-head">
          <span class="docs-pr-col-title">Ink</span>
          <span class="docs-pr-col-meta">Focus and main action</span>
        </div>
        <div class="docs-pr-card" data-penna="panel" data-ground>
          <div class="penna-field">
            <span class="penna-field-label" data-penna-text>Branch</span>
            <label class="penna-input" data-penna="field" data-state="focused"><input value="git switch -c build" aria-label="Branch" /></label>
          </div>
          <div class="docs-pr-btn-row">
            <button class="penna-button docs-pr-static" data-penna="button" data-state="focused" tabindex="-1" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
            <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Status

Red, amber and green mean failed or destructive, needs a look, and done or connected. Always pair status color with an icon or a word.

```figure uses=badge
<div class="docs-clr-status">
  <div class="docs-clr-status-card" data-penna="panel" data-ground>
    <div class="docs-clr-status-chip" data-tone="danger">
      <div class="docs-clr-status-chip-top">
        <span class="docs-clr-status-chip-name">Danger</span>
        <span class="docs-clr-status-chip-meta" data-color-hex="--penna-danger"></span>
      </div>
      <span class="docs-clr-status-chip-meta" data-color-ratio="--penna-on-danger" data-on="--penna-danger"></span>
    </div>
    <div class="docs-clr-status-body">
      <span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span>
      <p class="docs-clr-status-copy">Failed checks, errors, destructive actions.</p>
    </div>
  </div>
  <div class="docs-clr-status-card" data-penna="panel" data-ground>
    <div class="docs-clr-status-chip" data-tone="warning">
      <div class="docs-clr-status-chip-top">
        <span class="docs-clr-status-chip-name">Warning</span>
        <span class="docs-clr-status-chip-meta" data-color-hex="--penna-warning"></span>
      </div>
      <span class="docs-clr-status-chip-meta" data-color-ratio="--penna-on-warning" data-on="--penna-warning"></span>
    </div>
    <div class="docs-clr-status-body">
      <span class="penna-badge" data-penna="badge" data-tone="warning"><i data-icon="triangle-alert"></i><span class="badge-text" data-penna-text>Needs approval</span></span>
      <p class="docs-clr-status-copy">Waiting on input, non-fatal caution.</p>
    </div>
  </div>
  <div class="docs-clr-status-card" data-penna="panel" data-ground>
    <div class="docs-clr-status-chip" data-tone="success">
      <div class="docs-clr-status-chip-top">
        <span class="docs-clr-status-chip-name">Success</span>
        <span class="docs-clr-status-chip-meta" data-color-hex="--penna-success"></span>
      </div>
      <span class="docs-clr-status-chip-meta" data-color-ratio="--penna-on-success" data-on="--penna-success"></span>
    </div>
    <div class="docs-clr-status-body">
      <span class="penna-badge" data-penna="badge" data-tone="success"><i data-icon="circle-check"></i><span class="badge-text" data-penna-text>Connected</span></span>
      <p class="docs-clr-status-copy">Live connection, completed work.</p>
    </div>
  </div>
</div>
```

```figure uses=notice
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-notice" data-penna="notice" data-tone="danger"><div class="penna-notice-tone" data-penna="chip" data-tone="danger"><span class="penna-notice-lead"><i data-icon="circle-x"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Connection lost</span><span class="penna-notice-detail">Retrying in 5 s.</span></div></div></div></div><figcaption>Icon and words with the color</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><span class="docs-clr-bare-dot"><i aria-hidden="true"></i><span>studio.local</span></span></div><figcaption>Don't rely on color alone</figcaption></figure>
</div>
```

## Contrast

Every readable text token meets WCAG 2 floors on `bg` in light and dark: `7:1` for `fg`, `4.5:1` for `fg-muted`, `accent` and status text, and `3:1` for marks on accent fills. Custom base or accent colors shift in OKLCH lightness until they clear those floors; `fg-faint` stays under `4.5:1` on purpose.

```figure
<div class="docs-clr-contrast">
  <div class="docs-clr-floor" data-penna="panel" data-ground>
    <span class="docs-clr-floor-pair">fg / bg</span>
    <span class="docs-clr-floor-val" data-color-ratio="--penna-fg" data-on="--penna-bg" data-ratio-bare></span>
    <span class="docs-clr-floor-rule">Floor ≥ 7:1</span>
  </div>
  <div class="docs-clr-floor" data-penna="panel" data-ground>
    <span class="docs-clr-floor-pair">fg-muted / bg</span>
    <span class="docs-clr-floor-val" data-color-ratio="--penna-fg-muted" data-on="--penna-bg" data-ratio-bare></span>
    <span class="docs-clr-floor-rule">Floor ≥ 4.5:1</span>
  </div>
  <div class="docs-clr-floor" data-penna="panel" data-ground>
    <span class="docs-clr-floor-pair">accent / bg</span>
    <span class="docs-clr-floor-val" data-color-ratio="--penna-accent" data-on="--penna-bg" data-ratio-bare></span>
    <span class="docs-clr-floor-rule">Floor ≥ 4.5:1</span>
  </div>
  <div class="docs-clr-floor" data-penna="panel" data-ground>
    <span class="docs-clr-floor-pair">danger / bg</span>
    <span class="docs-clr-floor-val" data-color-ratio="--penna-danger" data-on="--penna-bg" data-ratio-bare></span>
    <span class="docs-clr-floor-rule">Floor ≥ 4.5:1</span>
  </div>
</div>
```

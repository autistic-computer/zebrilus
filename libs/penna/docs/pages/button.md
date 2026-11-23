# Button

Starts an action. To go somewhere use a link button, in a menu a menu row, for a setting a switch, to pick one of a few views a segmented control.

## Kinds

```figure
<div class="docs-show" data-docs-button>
  <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Primary</span></button>
  <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Secondary</span></button>
  <button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Ghost</span></button>
  <button class="penna-button" data-penna="button.outline" type="button"><span class="penna-label" data-penna-text>Outline</span></button>
  <button class="penna-button" data-penna="button" data-tone="danger" type="button"><span class="penna-label" data-penna-text>Danger</span></button>
  <button class="penna-button" data-variant="text" type="button"><span class="penna-label" data-penna-text>Text</span></button>
  <a class="penna-button" data-variant="link" href="#api" data-btn-demo-link><span class="penna-label" data-penna-text>Link</span></a>
</div>
```

- **Primary**: the main action. One per view.
- **Secondary**: everything else. The default.
- **Ghost**: bars, toolbars and rows.
- **Outline**: on a ground with its own fill.
- **Danger**: destroys, or can't be undone.
- **Text**: an inline action in foreground ink, no padding, muted while pressed.
- **Link**: underlined foreground ink, no padding; goes somewhere in text or at a row's end.

```rust
Button::new("save").label("Save").primary()
Button::new("notify").icon(IconName::Bell).ghost().tooltip("Notifications")
```

## Anatomy

```figure
<div class="docs-btn-anatomy">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Icon, label, caret</span>
      <span class="docs-lk-mono">40 px</span>
    </div>
    <div class="docs-btn-anat-box" data-penna="panel" data-ground>
      <button class="penna-button" data-penna="button" data-size="xlarge" data-caret type="button"><i data-icon="plus"></i><span class="penna-label" data-penna-text>Add host</span><i class="penna-caret" data-icon="chevron-down"></i></button>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Icon only</span>
      <span class="docs-lk-mono">40 × 40</span>
    </div>
    <div class="docs-btn-anat-box" data-penna="panel" data-ground>
      <button class="penna-button" data-penna="button" data-shape="icon" data-size="xlarge" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Inside a row</span>
      <span class="docs-lk-mono">4 px inset</span>
    </div>
    <div class="docs-btn-anat-box" data-penna="panel" data-ground>
      <div class="docs-btn-inside-pill" data-penna="menu-item" data-state="selected">
        <span data-penna-text>logs</span>
        <button class="penna-button" data-penna="icon-button" data-inside data-state="hover" aria-label="Close" data-tooltip="Close" type="button"><i data-icon="window-close"></i></button>
      </div>
    </div>
  </div>
</div>
```

1. **Container**: a wash, or a solid fill for primary, and corners. The size sets its height and side padding.
2. **Icon**: optional, before the label. A spinner while busy.
3. **Label**: a verb in sentence case.
4. **Caret**: optional. Opens a menu.

Icon only: square, with a tooltip naming the action. Trailing row actions use `Button::compact_icon` at 24; a button set inside a tab or row takes `.inside(around, height)` for a concentric corner 4 in from the edge.

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="xsmall" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="xsmall" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="small" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="small" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="large" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="large" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="xlarge" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="xlarge" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="touch" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="touch" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
  <span>touch-large</span><span><code>56</code></span><span><code>touch-body</code></span><span class="docs-btn-sizes-ex"><button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" data-shape="icon" data-size="touch-large" aria-label="Notifications" data-tooltip="Notifications" type="button"><i data-icon="bell"></i></button></span>
</div>
```

Medium is the default and follows density. One size per row.

## States

```figure uses=spinner
<div class="docs-lk-stack" data-btn-states>
  <div class="docs-btn-states-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Button kind" data-btn-kind-picker>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Secondary</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Primary</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Ghost</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Outline</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Danger</span></span>
      </div>
    </div>
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" type="button" data-btn-try-busy><span class="penna-label" data-penna-text>Save</span></button>
  </div>
  <div class="docs-btn-states-row">
    <div class="docs-cell"><button class="penna-button" data-penna="button" type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Rest</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover" type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover active" type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="focused" type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="current" type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Selected</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" disabled type="button" data-btn-state-item><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Off</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading type="button" data-btn-state-item><span class="penna-spinner"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Busy</span></div>
  </div>
</div>
```

- Focus ring: keyboard only, one ring width off a solid fill.
- Off: leaves the keyboard path, keeps its tooltip.
- Busy: a spinner, the label stays, clicks are ignored.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-state="focused" type="button"><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>The ring stands off a fill of its color</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-state="focused" style="--penna-ring-gap: 0px" type="button"><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>Don't let it touch and blend in</figcaption></figure>
</div>
```

## Labels

A verb, sentence case, no period. End with an ellipsis when more input comes first.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Keep</span></button><button class="penna-button" data-penna="button" data-tone="danger" type="button"><span class="penna-label" data-penna-text>Delete branch</span></button></div><figcaption>Name what happens</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>No</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>OK</span></button></div><figcaption>Don't use OK and No</figcaption></figure>
</div>
```

## Placement

Actions at the end, main action last. Destructive confirms are danger, not primary.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-align="end"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button></div><figcaption>Main action last</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-align="start"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button></div><figcaption>Don't lead with the main action</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Button::new(id)` | `button.penna-button` | Button container |
| `.label(text)` | `.penna-label` | Label |
| `.icon(name)` | `<i data-icon>` | Icon before the label; `data-shape="icon"` when icon only |
| `Button::compact_icon(id, icon)` | `data-shape="compact-icon"` | 24 px ghost icon button for trailing row actions |
| `.primary()`, `.ghost()`, `.danger()`, `.text()`, `.link()` | `data-tone`, `data-prominent`, `data-penna="button.quiet"`, `data-variant` | Kind |
| `.emphasis(Emphasis::Outline)` | `data-penna="button.outline"` | Outline kind |
| `.look(ButtonLook)` | `data-penna` | Surface role: `Floating`, `Send`, `Chrome`, `Row`, … |
| `.on(ground)` | `data-ground` on parent | Paint for the ground under it |
| `.inside(around, height)` | `data-inside`, `--penna-inside-rows` | Concentric corner 4 in from a rounded parent |
| `.xsmall()` … `.touch_large()` | `data-size` | Size; medium by default |
| `.selected(true)` | `data-state="current"` | Selected |
| `.disabled(true)` | `disabled` | Off |
| `.loading(true)` | `data-loading` | Busy; keeps label, ignores clicks |
| `.dropdown_caret(true)` | `data-caret`, `.penna-caret` | Caret |
| `.tooltip(text)`, `.tooltip_keys(keys)` | `data-tooltip` | Tooltip and keycaps; required with only an icon |
| `.on_click(handler)` | `click` | Runs on release |

## Phone

```figure uses=button,spinner,touch,compact
<div class="docs-btn-phone-grid">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Kinds</span>
      <span class="docs-lk-mono">56 px · full width</span>
    </div>
    <div class="docs-btn-phone-col" data-penna="panel" data-ground>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      <button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Scan QR</span></button>
      <button class="penna-button" data-penna="button.quiet" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Type an address instead</span></button>
      <button class="penna-button" data-penna="button" data-tone="danger" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Forget xps</span></button>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">States</span>
      <span class="docs-lk-mono">Rest · pressed · off · busy</span>
    </div>
    <div class="docs-btn-phone-col" data-penna="panel" data-ground>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Rest</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" data-state="active" type="button"><span class="penna-label" data-penna-text>Pressed</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" disabled aria-disabled="true" type="button"><span class="penna-label" data-penna-text>Disabled</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" data-loading type="button"><span class="penna-spinner" data-size="touch-large"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Connecting</span></button>
    </div>
  </div>
</div>
```

- **Main action**: `touch-large` 56, full width, one per screen, in the action bar.
- **Pressed**: the press highlight, a wash that rises after a short delay and holds long enough to see.
- **Loading**: the spinner before the label; the label stays, so the button keeps its width and its name.

```figure uses=compact looks=default,pill
<div class="docs-lk-stack" data-btn-phone-corners>
  <div class="docs-btn-states-bar">
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Phone controls" data-btn-phone-picker>
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-selected="true" aria-checked="true"><span class="penna-label" data-penna-text>Rounded</span></span>
        <span class="penna-segment" role="radio" aria-selected="false" aria-checked="false"><span class="penna-label" data-penna-text>Pill</span></span>
      </div>
    </div>
    <span class="docs-lk-mono">Disc corner − 6 px inset</span>
  </div>
  <div class="docs-btn-disc-grid" data-look="default" data-btn-phone-stage>
    <div class="docs-pr-col">
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Bar icon button</span>
        <span class="docs-lk-mono" data-btn-bar-meta>44 × 44 · 13.7 px</span>
      </div>
      <div class="docs-btn-disc-card" data-penna="panel" data-ground>
        <button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings" data-tooltip="Settings" type="button"><i data-icon="settings"></i></button>
      </div>
    </div>
    <div class="docs-pr-col">
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Over content</span>
        <span class="docs-lk-mono" data-btn-disc-meta>56 disc · 17.5 px</span>
      </div>
      <div class="docs-btn-disc-stage">
        <div class="docs-btn-disc-under" aria-hidden="true">
          <span class="docs-glass-text"><b>fn</b> connect(host: &amp;str) {<br />&nbsp;&nbsp;<i>let</i> link = dial(host);</span>
          <div class="docs-lk-glass-bars"><i></i><i></i><i></i></div>
        </div>
        <span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings" data-tooltip="Settings" type="button"><i data-icon="settings"></i></button></span>
      </div>
    </div>
  </div>
</div>
```

The disc rounds like a control of its height, a circle under Pill controls, never like a container; the button in it is the disc's corner less 6.

# Button

Starts an action. To go somewhere use a link, in a menu a menu row, for a setting a switch.

## Kinds

```figure
<div class="docs-show">
  <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Primary</span></button>
  <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Secondary</span></button>
  <button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Ghost</span></button>
  <button class="penna-button" data-penna="button.outline"><span class="penna-label" data-penna-text>Outline</span></button>
  <button class="penna-button" data-penna="button" data-tone="danger"><span class="penna-label" data-penna-text>Danger</span></button>
  <a class="penna-button" data-variant="link" href="#"><span class="penna-label" data-penna-text>Link</span></a>
</div>
```

- **Primary**: the main action. One per view.
- **Secondary**: everything else. The default.
- **Ghost**: bars, toolbars and rows.
- **Outline**: on a ground with its own fill.
- **Danger**: destroys, or can't be undone.
- **Link**: goes somewhere.

```rust
Button::new("save").label("Save").primary()
```

## Anatomy

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-size="xlarge" data-caret><i data-icon="plus"></i><span class="penna-label" data-penna-text>Add host</span><i class="penna-caret" data-icon="chevron-down"></i></button><span class="docs-note">Icon, label, caret</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-size="xlarge" aria-label="Notifications"><i data-icon="bell"></i></button><span class="docs-note">Icon only</span></div>
</div>
```

1. **Container**: a wash, or a solid fill for primary, and corners. The size sets its height and side padding.
2. **Icon**: optional, before the label. A spinner while busy.
3. **Label**: a verb in sentence case.
4. **Caret**: optional. Opens a menu.

Icon only: square, with a tooltip naming the action.

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><button class="penna-button" data-penna="button" data-size="xsmall"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><button class="penna-button" data-penna="button" data-size="small"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><button class="penna-button" data-penna="button" data-size="large"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><button class="penna-button" data-penna="button" data-size="xlarge"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><button class="penna-button" data-penna="button" data-size="touch"><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>touch-large</span><span><code>56</code></span><span><code>touch-body</code></span><span><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Save</span></button></span>
</div>
```

Medium is the default and follows density. One size per row.

## States

```figure uses=spinner
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover active"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="focused"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button.quiet" data-state="current"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" disabled><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Off</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading><span class="penna-spinner" data-size="small"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Busy</span></div>
</div>
```

- Focus ring: keyboard only, one ring width off a solid fill.
- Off: leaves the keyboard path, keeps its tooltip.
- Busy: a spinner, the label stays, clicks are ignored.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-state="focused"><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>The ring stands off a fill of its color</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-state="focused" style="--penna-ring-gap: 0px"><span class="penna-label" data-penna-text>Share</span></button></div><figcaption>Don't let it touch and blend in</figcaption></figure>
</div>
```

## Labels

A verb, sentence case, no period. End with an ellipsis when more input comes first.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Keep</span></button><button class="penna-button" data-penna="button" data-tone="danger"><span class="penna-label" data-penna-text>Delete branch</span></button></div><figcaption>Name what happens</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>No</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>OK</span></button></div><figcaption>Don't use OK and No</figcaption></figure>
</div>
```

## Placement

Actions at the end, main action last. Destructive confirms are danger, not primary.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-align="end"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button></div><figcaption>Main action last</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-align="start"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button></div><figcaption>Don't lead with the main action</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.label(text)` | `.penna-label` | Label |
| `.icon(name)` | `<i data-icon>` | Icon before the label |
| `.primary()`, `.ghost()`, `.danger()`, `.link()` | `data-tone`, `data-prominent`, `data-penna="button.quiet"`, `data-variant` | Kind |
| `.emphasis(Emphasis::Outline)` | `data-penna="button.outline"` | Outline kind |
| `.xsmall()` … `.touch_large()` | `data-size` | Size; medium by default |
| `.selected(true)` | `data-state="current"` | Selected |
| `.disabled(true)` | `disabled` | Off |
| `.loading(true)` | `data-loading` | Busy |
| `.dropdown_caret(true)` | `data-caret`, `.penna-caret` | Caret |
| `.tooltip(text)` | `data-tooltip` | Tooltip; required with only an icon |
| `.on_click(handler)` | `click` | Runs on release |

## Phone

```figure uses=button,touch,compact
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px; gap: 12px">
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" style="width: 100%"><span class="penna-label" data-penna-text>Connect</span></button>
    <button class="penna-button" data-penna="button" data-size="touch-large" style="width: 100%"><span class="penna-label" data-penna-text>Scan QR</span></button>
    <button class="penna-button" data-penna="button.quiet" data-size="touch-large" style="width: 100%"><span class="penna-label" data-penna-text>Type an address instead</span></button>
    <button class="penna-button" data-penna="button" data-tone="danger" data-size="touch-large" style="width: 100%"><span class="penna-label" data-penna-text>Forget xps</span></button>
    <span class="docs-note">Primary, secondary, ghost, danger</span>
  </div>
  <div class="docs-cell" style="width: 370px; gap: 12px">
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" style="width: 100%"><span class="penna-label" data-penna-text>Rest</span></button>
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" data-state="active" style="width: 100%"><span class="penna-label" data-penna-text>Pressed</span></button>
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" disabled aria-disabled="true" style="width: 100%"><span class="penna-label" data-penna-text>Disabled</span></button>
    <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" data-loading style="width: 100%"><i class="penna-touch-spinner" data-icon="loader"></i><span class="penna-label" data-penna-text>Connecting</span></button>
    <span class="docs-note">States</span>
  </div>
</div>
```

- **Main action**: `touch-large` 56, full width, one per screen, in the action bar.
- **Pressed**: the press highlight, a wash that rises after a short delay and holds long enough to see.
- **Loading**: the spinner before the label; the label stays, so the button keeps its width and its name.

```figure uses=compact
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button><span class="docs-note">Icon button, 44</span></div>
  <div class="docs-cell"><span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button></span><span class="docs-note">Over the content, in a 56 disc</span></div>
</div>
```

The disc rounds like a control of its height, a circle under Pill controls, never like a container; the button in it is the disc's corner less 6.

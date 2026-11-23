# Chip

Names the state of a thing at control size: a dot or an icon, a word, a detail. For a tag inside a line of text, use a [badge](badge.html); to start an action, a [button](button.html); to pick one of a few options, a [segmented control](segmented.html).

## Kinds

```figure uses=chip
<div class="docs-show">
  <span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Soft</span></span>
  <span class="penna-chip" data-penna="chip" data-emphasis="solid" data-solid="primary"><span class="penna-solid-ink" data-penna="button" data-prominent><span class="penna-text penna-chip-label" data-penna-text>Solid</span></span></span>
  <span class="penna-chip" data-penna="chip" data-tone="accent" data-emphasis="solid" data-solid="accent"><span class="penna-solid-ink" data-solid-ink="accent"><span class="penna-text penna-chip-label" data-penna-text>Solid accent</span></span></span>
  <span class="penna-chip" data-penna="chip.outline"><span class="penna-text penna-chip-label" data-penna-text>Outline</span></span>
  <span class="penna-chip" data-penna="chip" data-emphasis="ghost"><span class="penna-text penna-chip-label" data-penna-text>Ghost</span></span>
</div>
```

- **Soft**: the default.
- **Solid**: a full fill in its tone, such as an editor's mode.
- **Outline**: an edge, no fill.
- **Ghost**: ink only.

```rust
Chip::new("host", "Ready").dot(Tone::Success).detail("this Mac")
```

## Looks

```figure uses=chip
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-chip" data-penna="chip"><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Plain</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.bar"><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Bar</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.floating"><span class="penna-text penna-chip-label" data-penna-text>%3</span></span><span class="docs-note">Floating</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.marker"><span class="penna-text penna-chip-label" data-penna-text>1</span></span><span class="docs-note">Marker</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.drag"><span class="penna-text penna-chip-label" data-penna-text>vim</span></span><span class="docs-note">Drag</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="badge.icon" data-look="icon-disc"><i data-icon="terminal"></i></span><span class="docs-note">Icon</span></div>
</div>
```

- **Plain**: panes, lists, dialogs.
- **Bar**: title bars and pane headers.
- **Floating**: over a pane's content. Glass when neutral; with a tone it draws the tone opaque.
- **Marker**: display-panes markers. An accent ring lights it.
- **Drag**: follows a dragged pane.
- **Icon**: the disc behind a blank page's icon.

## Anatomy

```figure uses=chip
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-chip" data-penna="chip" data-detail><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span><span class="penna-text penna-chip-detail">· ubuntu</span></span><span class="docs-note">Dot, label, detail</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip" data-detail><i data-icon="laptop"></i><span class="penna-text penna-chip-label" data-penna-text>macbook</span><span class="penna-text penna-chip-detail">· this Mac</span></span><span class="docs-note">Icon, label, detail</span></div>
  <div class="docs-cell" style="max-width: 148px"><span class="penna-chip" data-penna="chip" data-detail style="max-width: 100%"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span><span class="penna-text penna-chip-detail">· studio-macbook.local</span></span><span class="docs-note">Detail truncates first</span></div>
</div>
```

1. **Container**: a pill. The size sets its height and side padding.
2. **Lead**: optional 6 px dot in its tone, or an icon at `icon_px`.
3. **Label**: the state, or the thing's name, at 500.
4. **Detail**: optional middle dot and muted fact. It is the only part that shrinks and ends in an ellipsis; `Chip::empty(id)` omits the label and takes children only.

## Sizes

```figure uses=chip
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><span class="penna-chip" data-penna="chip" data-size="medium"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><span class="penna-chip" data-penna="chip" data-size="large"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><span class="penna-chip" data-penna="chip" data-size="xlarge"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><span class="penna-chip" data-penna="chip" data-size="touch"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span>
</div>
```

Small is the default, not medium. One size per row.

## States

```figure uses=chip
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip" data-state="selected"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip" data-state="focused"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip" data-state="disabled"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span><span class="docs-note">Off</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.marker" data-interactive role="button" tabindex="0" data-state="hover"><span class="penna-text penna-chip-label" data-penna-text>1</span></span><span class="docs-note">Marker hover</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.marker" data-state="current" data-ring="success"><span class="penna-text penna-chip-label" data-penna-text>2</span></span><span class="docs-note">Current, tinted ring</span></div>
</div>
```

- Read-only by default: the pointer passes through and it is never in the focus order.
- Only `Marker` has a hover and press paint (the accent ring), and it activates on pointer input only with `.interactive(true)`.
- An interactive chip with a click handler is a tab stop: Space and Enter run it, and a click never focuses it.

## Content

The label says the state in a word or names the thing. The detail adds one fact.

```figure uses=chip
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="danger"></i><span class="penna-text penna-chip-label" data-penna-text>Needs attention</span></span></div><figcaption>Say the state in a word</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="danger"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span></div><figcaption>Don't leave the state to the dot</figcaption></figure>
</div>
```

## Placement

Chips sit `gap-related` 8 apart, at the size of the controls beside them.

```figure uses=chip
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span><button class="penna-button" data-penna="button" data-size="small" type="button"><span class="penna-label" data-penna-text>Disconnect</span></button></div><figcaption>One size in a row</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span><button class="penna-button" data-penna="button" data-size="large" type="button"><span class="penna-label" data-penna-text>Disconnect</span></button></div><figcaption>Don't mix sizes</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Chip::new(id, label)` | `.penna-chip`, `.penna-chip-label` | Label |
| `Chip::empty(id)` | `.penna-chip` with children | Icon-only, dot-only, or custom children |
| `Dot::new(tone)`, `.on(ground)` | `i.penna-dot[data-tone]` | Standalone status dot |
| `.dot(tone)` | `i.penna-dot[data-tone]` | 6 px dot before the label |
| `.icon(name)` | `<i data-icon>` | Icon before the label |
| `.detail(text)` | `.penna-chip-detail`, `data-detail` | Muted detail after the label |
| `.tone(tone)` | `data-tone` | Tone |
| `.emphasis(emphasis)` | `chip.outline`, `data-emphasis`, `data-solid`, `.penna-solid-ink` | Soft, solid, outline or ghost |
| `.look(look)` | `chip.bar` …, `badge.icon` + `data-look="icon-disc"` | Look |
| `.xsmall()` … `.touch()`, `.with_size(size)` | `data-size` | Size; small by default |
| `.selected(true)`, `.state(state)` | `data-state` | Selected, current, focused, disabled |
| `.interactive(true)` | `data-interactive` | Marker hover/press ring, button role |
| `.on_click(handler)` | `role="button"`, `tabindex="0"`, `click` | Takes focus; Space and Enter run it |
| `.ring(color)` | `data-ring` | Recolors the marker's ring |
| `.on(ground)` | `data-ground` on parent | Paint for the ground under it |
| `.multiline(true)` | | Allows the label to wrap |
| `.child(el)`, `.children(iter)` | Child elements | Extra content after the detail |

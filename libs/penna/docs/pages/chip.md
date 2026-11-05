# Chip

Names the state of a thing: a dot or an icon, a word, a detail. For a tag inside a line of text, use a badge.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Icons don't grow with the interface font size.</span></div></div></div>
```

## Kinds

```figure uses=chip
<div class="docs-show">
  <span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Soft</span></span>
  <span class="penna-chip" data-penna="chip" data-emphasis="solid" data-solid="primary"><span class="penna-solid-ink" data-penna="button" data-prominent><span class="penna-text penna-chip-label" data-penna-text>Solid</span></span></span>
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
</div>
```

- **Plain**: panes, lists, dialogs.
- **Bar**: title bars and pane headers.
- **Floating**: over a pane's content.
- **Marker**: pane markers. A ring lights it.
- **Drag**: follows a dragged pane.

## Anatomy

```figure uses=chip
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span><span class="penna-text penna-chip-detail">· ubuntu</span></span><span class="docs-note">Dot, label, detail</span></div>
  <div class="docs-cell"><span class="penna-chip" data-penna="chip"><i data-icon="laptop"></i><span class="penna-text penna-chip-label" data-penna-text>macbook</span><span class="penna-text penna-chip-detail">· this Mac</span></span><span class="docs-note">Icon, label, detail</span></div>
</div>
```

1. **Container**: a pill. The size sets its height and side padding.
2. **Lead**: a dot in its tone, or an icon.
3. **Label**: the state, or the thing's name.
4. **Detail**: optional and muted. It shrinks first and ends in an ellipsis.

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
  <div class="docs-cell"><span class="penna-chip" data-penna="chip.marker" data-state="hover"><span class="penna-text penna-chip-label" data-penna-text>1</span></span><span class="docs-note">Marker hover</span></div>
</div>
```

- Read-only unless interactive, which lights it on hover.
- An interactive chip with a click handler takes keyboard focus and runs on Space and Enter. A click never focuses it.
- Hover and press show only on the marker.

## Content

The label says the state in a word or names the thing. The detail adds one fact.

```figure uses=chip
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="danger"></i><span class="penna-text penna-chip-label" data-penna-text>Needs attention</span></span></div><figcaption>Say the state in a word</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="danger"></i><span class="penna-text penna-chip-label" data-penna-text>ubuntu</span></span></div><figcaption>Don't leave the state to the dot</figcaption></figure>
</div>
```

## Placement

Chips sit 8 apart, at the size of the controls beside them.

```figure uses=chip
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span><button class="penna-button" data-penna="button" data-size="small"><span class="penna-label" data-penna-text>Disconnect</span></button></div><figcaption>One size in a row</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><span class="penna-chip" data-penna="chip"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span><button class="penna-button" data-penna="button" data-size="large"><span class="penna-label" data-penna-text>Disconnect</span></button></div><figcaption>Don't mix sizes</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Chip::new(id, label)` | `.penna-chip`, `.penna-chip-label` | Label |
| `.dot(tone)` | `i.penna-dot[data-tone]` | Dot before the label |
| `.icon(name)` | `<i data-icon>` | Icon before the label |
| `.detail(text)` | `.penna-chip-detail`, `data-detail` | Muted detail after the label |
| `.tone(tone)` | `data-tone` | Tone |
| `.emphasis(emphasis)` | `data-penna="chip.outline"`, `data-emphasis` | Kind |
| `.look(look)` | `data-penna="chip.bar"` … | Look |
| `.xsmall()` … `.touch()` | `data-size` | Size; small by default |
| `.selected(true)` | `data-state="selected"` | Selected |
| `.state(state)` | `data-state` | Any paint state |
| `.interactive(true)` | `data-interactive` | Lights on hover |
| `.on_click(handler)` with `.interactive(true)` | `role="button"`, `tabindex="0"`, `click` | Takes focus; Space and Enter run it |
| `.ring(color)` | `data-ring` | Recolors the marker's ring |

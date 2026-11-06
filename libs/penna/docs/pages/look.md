# Look

penna draws one look. It starts from the platform's knobs, a person sets every knob in Appearance, and everything else stays the kit's.

## Kinds

Steps of fill carry the structure, so a dense screen holds few lines. Only what floats over content casts a shadow. A press replaces the hover wash, never stacks on it.

```figure uses=badge,input,list,separator
<div class="docs-tokens">
  <span class="docs-head">Kind</span><span class="docs-head">Rest</span><span class="docs-head">Picked</span><span class="docs-head">Example</span>
  <span>Button</span><span>A wash</span><span>A deeper wash</span><span class="docs-row"><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button></span>
  <span>Item</span><span>No fill</span><span>The highlight</span><span class="docs-row"><span class="penna-list" style="width: 104px"><span class="penna-list-item" data-penna="menu-item" role="option"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></span></span><span class="penna-list" style="width: 104px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span></span></span></span>
  <span>Field</span><span>A wash</span><span>An edge, typing</span><span class="docs-row"><label class="penna-input" data-penna="field" style="width: 104px"><input placeholder="Host" aria-label="Host" /></label><label class="penna-input" data-penna="field" data-state="focused" style="width: 104px"><input value="studio" aria-label="Host" /></label></span>
  <span>Card</span><span>A step of fill</span><span>A ring</span><span><span class="penna-card" data-penna="card" style="display: block; width: 160px; padding: var(--penna-inset-card)"><span class="penna-type" data-type="small" data-penna-text>3 panes, 1 window</span></span></span>
  <span>Floating</span><span>Glass and a shadow</span><span></span><span><span class="penna-tooltip" data-penna="tooltip" role="tooltip" style="position: static"><span data-penna-text>Split right</span></span></span>
  <span>Label</span><span>A wash in its tone</span><span></span><span class="docs-row"><span class="penna-badge" data-penna="badge"><span class="badge-text" data-penna-text>12</span></span><span class="penna-badge" data-penna="badge" data-tone="danger"><i data-icon="circle-x"></i><span class="badge-text" data-penna-text>3 failing</span></span></span>
  <span>Rule</span><span>A hairline</span><span></span><span><span class="penna-separator-rule" role="separator" style="display: block; width: 160px"><span data-penna="rule.bar"></span></span></span>
</div>
```

Hairlines mean something: a rule, an outline variant, a field being typed in. Increase contrast adds none.

```figure uses=input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><label class="penna-input" data-penna="field" style="width: 140px"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Connect</span></button></div><figcaption>Washes, no edges</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><label class="penna-input" data-penna="field" style="width: 140px; box-shadow: inset 0 0 0 1px var(--penna-border)"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" style="box-shadow: inset 0 0 0 1px var(--penna-border)"><span class="penna-label" data-penna-text>Connect</span></button></div><figcaption>Don't edge every control</figcaption></figure>
</div>
```

## Tint

`tint` colors what is on, picked or primary. `ink` keeps it all in the text color; a person's accent replaces it.

```figure uses=switch,check,list
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="default"><div class="docs-look"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button><button class="penna-toggle" aria-checked="true" aria-label="Sync"></button><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore</span></label><span class="penna-list" style="width: 132px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></span></span></div><span class="docs-note"><code>ink</code></span></div>
  <div class="docs-cell" data-look="blue"><div class="docs-look"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button><button class="penna-toggle" aria-checked="true" aria-label="Sync"></button><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore</span></label><span class="penna-list" style="width: 132px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></span></span></div><span class="docs-note"><code>blue</code></span></div>
  <div class="docs-cell" data-look="pink"><div class="docs-look"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Save</span></button><button class="penna-toggle" aria-checked="true" aria-label="Sync"></button><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore</span></label><span class="penna-list" style="width: 132px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span></span></span></div><span class="docs-note"><code>pink</code></span></div>
</div>
```

On a tint's solid selection everything in the row takes its ink, and a toned chip turns to a wash of it. With the ink as tint the highlight is a wash and the chip keeps its tone.

```figure uses=list,chip
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="default"><span class="penna-list" style="width: 220px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-chip" data-penna="chip" data-tone="success" data-size="xsmall"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span></span></span><span class="docs-note"><code>ink</code></span></div>
  <div class="docs-cell" data-look="blue"><span class="penna-list" style="width: 220px"><span class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span><span class="penna-chip" data-penna="chip" data-tone="success" data-size="xsmall"><i class="penna-dot" data-tone="success"></i><span class="penna-text penna-chip-label" data-penna-text>Ready</span></span></span></span></span><span class="docs-note"><code>blue</code></span></div>
</div>
```

## Neutral

`neutral` tints the window, the text and every surface above them: a hue and an amount, warm 60 at 0.15 by default. Status colors and contrast hold at every value.

## Corners

Every corner follows Corner radius. A control (a button, field, tab, chip, badge or row) rounds to a share of its own height, so a small badge and a tall sidebar entry read equally round: 31% of the height at the default, 45% at 100%. Containers (cards, panels, popovers, dialogs) keep `radius` in pixels. A corner inside another is concentric, like the segmented control's pill in its track or a menu's rows in the menu, and only `full` makes pills.

```figure uses=input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="radius-4"><div class="penna-card docs-look" data-penna="card"><label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note"><code>radius</code> 4</span></div>
  <div class="docs-cell" data-look="default"><div class="penna-card docs-look" data-penna="card"><label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note"><code>radius</code> 12</span></div>
  <div class="docs-cell" data-look="radius-20"><div class="penna-card docs-look" data-penna="card"><label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note"><code>radius</code> 20</span></div>
  <div class="docs-cell" data-look="pill"><div class="penna-card docs-look" data-penna="card"><label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note"><code>control-radius</code> full</span></div>
</div>
```

## Smoothing

`smoothing` shapes every corner, from round arcs at 2 to a squircle at 5, without changing how round it looks: a smoother corner is drawn in a larger box, so its middle sits where a round arc of the same radius would.

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="smoothing-2"><div class="penna-card docs-shape" data-penna="card"></div><span class="docs-note"><code>smoothing</code> 2</span></div>
  <div class="docs-cell" data-look="smoothing-3"><div class="penna-card docs-shape" data-penna="card"></div><span class="docs-note"><code>smoothing</code> 3</span></div>
  <div class="docs-cell" data-look="smoothing-5"><div class="penna-card docs-shape" data-penna="card"></div><span class="docs-note"><code>smoothing</code> 5</span></div>
</div>
```

Drawn at `radius` 24, where the curve shows.

## Glass

Glass is for what sits over something: what floats over the content (menus, popovers, tooltips, toasts, a button over a transcript), panes when they have gaps, and the selected row or tab in a sidebar or title bar. It is a switch: on, `liquid` blurs what is under it and bends it at the rim; off, `none` draws the popover fill.

```figure uses=menu
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="glass-none"><div class="docs-glass"><span class="docs-glass-text"><b>fn</b> split(pane: Pane) {<br />&nbsp;&nbsp;<i>let</i> width = pane.width / 2;<br />&nbsp;&nbsp;tabs.push(<em>"logs"</em>);<br />}</span><span class="docs-glass-bars"><i></i><i></i><i></i></span><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><span class="docs-note"><code>none</code></span></div>
  <div class="docs-cell" data-look="glass-liquid"><div class="docs-glass"><span class="docs-glass-text"><b>fn</b> split(pane: Pane) {<br />&nbsp;&nbsp;<i>let</i> width = pane.width / 2;<br />&nbsp;&nbsp;tabs.push(<em>"logs"</em>);<br />}</span><span class="docs-glass-bars"><i></i><i></i><i></i></span><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><span class="docs-note"><code>liquid</code></span></div>
</div>
```

Text on any glass reads at 4.5:1 over the worst content, and Reduce transparency draws it as the popover fill. Where the renderer can't bend light, `liquid` keeps its blur, tint and rim and drops the lens. Glass is neutral: a floating chip with a tone draws its tone opaque. A selected sidebar row or tab reads as strong as the highlight it replaces, and a pane with gaps, or a PiP floating over the panes, keeps a hairline edge wherever its glass draws no rim. With glass off a PiP is its pane's own color, opaque.

```figure uses=menu
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-popup" data-penna="popover"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span></span></div><div class="penna-popup-row" data-penna="menu-item" data-kind="item" role="menuitem"><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span></span></div></div></div></div><figcaption>Glass on what floats</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-card" data-penna="card" style="padding: var(--penna-inset-card); background: color-mix(in srgb, var(--penna-popover) 70%, transparent); backdrop-filter: var(--penna-glass-backdrop)"><span class="penna-type" data-type="small" data-penna-text>build · cargo test</span></div></div><figcaption>Don't make a card, or a control in content, glass</figcaption></figure>
</div>
```

## What a person sets

The same in every app, in Appearance, in three groups: Look, Size and Accessibility. There are no presets to pick: every knob is a row, starting at the platform's value.

| Knob | Does |
| --- | --- |
| Mode | System, light or dark, unless a theme locks it |
| Accent | The tint: ink, a named accent or a color |
| Tint, Tint strength | The neutral's hue and amount |
| Corner radius | The radius, 0 to 100% on a square curve: 69% is the default 12, 100% is 25 and never a pill |
| Controls | Rounded follows the radius; Pill makes controls pills |
| Glass | A switch: on draws liquid glass, off draws none |
| UI font | The face |
| Text size | The root size; everything scales with it |
| Density | Compact, regular or comfortable rows and controls |
| Increase contrast | 0 to 100%: surfaces and fills further apart, muted text closer to the ink; no edges |
| Reduce transparency | Glass draws as the popover fill |
| Motion | Off, default, or fast (every animation at half its time) |

## The kit's theme files

The look starts from `macos` on Apple platforms and `default` elsewhere. `yaru` is a third look the storybook and the tests draw. None is a choice a person makes.

| Theme file | Tint | Radius | Control radius | Smoothing | Glass |
| --- | --- | --- | --- | --- | --- |
| `default` | ink | 12 | 12 | 3 | liquid |
| `macos` | blue | 12 | full | 4 | liquid |
| `yaru` | orange | 8 | 8 | 2 | none |

## Theme file

A theme file holds only knobs. Keys it leaves out keep their defaults; keys it doesn't know are ignored. An app that takes one (`AppearanceChange::Import`) sets every knob a person has from it.

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
| `tint` | `.tint` | Ink, an accent, a hex color, or a light and dark pair |
| `neutral` | `.neutral` | Hue 0 to 360, amount 0 to 1 |
| `radius` | `.radius` | Container corners, 0 to 25; 25 is round, not a pill |
| `control-radius` | `.control_radius` | Control corners, 0 to 25 or full; the radius when left out |
| `smoothing` | `.smoothing` | Corner shape, 2 to 5 |
| `glass` | `.glass` | None, liquid or a material; liquid when left out |
| `face` | `.face` | Interface face; the platform's when left out |
| `mode` | `.mode` | Follow, light or dark; beats the person's Mode |
| | `parse_theme(json)` | Reads a theme file |
| | `.activate(mode, window, cx)` | Draws the app in it |

The HTML twin reads the same knobs: `cargo run --example theme_css` writes its tokens.

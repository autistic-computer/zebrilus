# Checkbox and radio

A checkbox turns one option on or off by itself; a radio group picks one of a few, all in view. Both take effect when the form is confirmed. For a setting that applies at once use a switch.

## Kinds

```figure uses=check
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore sessions</span></label><span class="docs-note">Checkbox</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="mixed"><span class="penna-check"></span><span class="penna-label" data-penna-text>All panes</span></label><span class="docs-note">Mixed</span></div>
  <div class="docs-cell"><div class="penna-radio-group" data-value="0"><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Last session</span></label><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Empty window</span></label></div><span class="docs-note">Radio group</span></div>
  <div class="docs-cell"><span class="penna-check" data-on><i data-icon="check"></i></span><span class="docs-note">Mark</span></div>
</div>
```

- **Checkbox**: an option on or off by itself.
- **Mixed**: over a set of checkboxes that are partly on. A press turns it on.
- **Radio group**: one of two to seven options. More go in a select.
- **Mark**: the box alone, for a row, tile or task list that takes the press itself.

```rust
Checkbox::new("restore").label("Restore sessions").checked(on).on_click(toggle)
```

## Anatomy

```figure uses=check
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div style="width: 220px"><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Reopen every pane where it was when the window closed</span></label></div><span class="docs-note">Box, mark, label</span></div>
</div>
```

1. **Box**: an edge when off, filled with the accent when on or mixed. A checkbox's corner is a quarter of the `radius` knob; a radio is round.
2. **Mark**: a check, a minus or a dot, in the fill's ink.
3. **Label**: centered on the box at its first line's capitals. A long label wraps under itself, never under the box.

## Sizes

```figure uses=check
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Box</span><span class="docs-head">Label</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>14</code></span><span><code>caption</code></span><span><label class="penna-checkbox" data-size="xsmall" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore sessions</span></label></span>
  <span>small</span><span><code>14</code></span><span><code>small</code></span><span><label class="penna-checkbox" data-size="small" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore sessions</span></label></span>
  <span>medium and up</span><span><code>16</code></span><span><code>body</code></span><span><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore sessions</span></label></span>
</div>
```

Medium is the default. It sits in a line of text, so larger sizes draw like medium. Under touch the hit area grows to 44.

## States

```figure uses=check
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="false"><span class="penna-check"></span><span class="penna-label" data-penna-text>Off</span></label><span class="docs-note">Off</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>On</span></label><span class="docs-note">On</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="false" data-state="focused"><span class="penna-check"></span><span class="penna-label" data-penna-text>Focused</span></label><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="true" data-state="focused"><span class="penna-check"></span><span class="penna-label" data-penna-text>Focused</span></label><span class="docs-note">On, focused</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="true" aria-disabled="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Disabled</span></label><span class="docs-note">Disabled</span></div>
  <div class="docs-cell"><label class="penna-checkbox" aria-checked="false" data-tone="danger"><span class="penna-check"></span><span class="penna-label" data-penna-text>Error</span></label><span class="docs-note">Error</span></div>
</div>
```

- Hover and press: the box lifts, then darkens, over the box or the label. A press flips it on release.
- Focused: a ring outside the box, one ring width off it when filled, from the keyboard only. Space or Enter flips it.
- Disabled: a fainter box, a faint label, out of the focus order.
- Error: the danger tone. Its field says what is wrong.

## Radio groups

```figure uses=check
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-radio-group" data-value="0" data-state="focused"><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Last session</span></label><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Home folder</span></label><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Empty window</span></label></div><span class="docs-note">Stacked, focused</span></div>
  <div class="docs-cell"><div class="penna-radio-group" data-value="1" data-orientation="horizontal"><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Tiled</span></label><label class="penna-radio"><span class="penna-check" data-kind="radio"></span><span class="penna-label" data-penna-text>Floating</span></label></div><span class="docs-note">In a row</span></div>
</div>
```

Stacked 8 apart, or in a row 16 apart. The group is one tab stop with the ring on the picked radio, or the first while none is picked. The arrows, Home and End pick and stop at the ends.

## Labels

Phrase the option so that on means yes.

```figure uses=check
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore sessions</span></label></div><figcaption>On turns it on</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Don't restore sessions</span></label></div><figcaption>Don't phrase it as a negative</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Checkbox::new(id)` | `label.penna-checkbox` | A box and its label |
| `.label(text)` | `.penna-label` | The label |
| `.checked(true)` | `aria-checked="true"` | On |
| `.indeterminate(true)` | `aria-checked="mixed"` | Mixed |
| `.on_click(handler)` | `change` | Gets the new value |
| `RadioGroup::new(id, labels)` | `.penna-radio-group`, `.penna-radio` | The radios |
| `.selected(index)` | `data-value` | The picked one; `None` for none |
| `.on_select(handler)` | `change` | Gets the picked index |
| `.horizontal()` | `data-orientation="horizontal"` | In a row |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Error |
| `.xsmall()`, `.small()` | `data-size` | The 14 box beside smaller text |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `Check::checkbox(on)`, `Check::radio(on)` | `.penna-check`, `data-kind="radio"` | The mark alone |
| `.on(ground)` | | Paints the mark for the surface under it |

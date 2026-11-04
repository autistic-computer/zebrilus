# Color

Color in penna does three jobs: it separates surfaces, it marks what is on or in focus, and it warns. It is never decoration.

## Surfaces

A window is a few surfaces stacked on each other: the window itself, panels beside it, and popovers that float above. Each layer steps a little away from the one under it, darker in light mode and lighter in dark mode, so layers read apart without heavy borders.

```figure story=panels/panels
```

Try the theme switcher in the corner: each look draws these surfaces its own way, but the order of the layers never changes.

## Ink

Text comes in three strengths. Full ink is for what people read and act on. Muted ink is for descriptions and labels that support it. Faint ink is only for placeholders and things that are switched off, never for text someone needs to read.

```figure uses=label,input
<div class="docs-full" style="display: flex; flex-direction: column; gap: 16px; max-width: 420px">
  <div style="display: flex; flex-direction: column; gap: 4px">
    <span class="penna-type" data-type="heading" data-penna-text>Paired computers</span>
    <span class="penna-type docs-text" data-type="body" data-muted data-penna-text>Keyboard and pointer move to whichever screen the cursor reaches.</span>
  </div>
  <label class="penna-input" data-penna="field"><input placeholder="Search computers" /></label>
</div>
```

## Highlight

One color marks what is on, picked or in focus: a switch that is on, the field you are typing in, the main button. In the default look that color is the ink itself, which keeps the interface black and white. Pick an accent in Appearance and the same places take that color instead. Either way, keep it to a few places in a view: when everything stands out, nothing does.

```figure story=selection/switch
```

```figure story=inputs/field-focused
```

## Status

Red, amber and green are kept for things going wrong, needing attention, or going right. They always come with an icon or a word, so they still read for people who can't tell the colors apart.

Colored text stays readable in every look, light and dark: the status colors and the accent are strong enough to read as text, and a label on a filled button or badge in any of them reads just as well, whatever accent you pick.

```figure story=status/notices
```

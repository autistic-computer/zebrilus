# Action bar

Holds a phone screen's one main action at the bottom, where the thumb is. For moving around, the floating bar and strip.

## Layouts

```figure uses=touch,button
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px; max-width: 100%"><div class="penna-action-bar" style="width: 100%; padding-inline: 0"><button class="penna-button" data-penna="button.quiet" data-size="touch-large"><span class="penna-label" data-penna-text>Scan QR instead</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note">Secondary above</span></div>
  <div class="docs-cell" style="width: 370px; max-width: 100%"><div class="penna-action-bar" data-row style="width: 100%; padding-inline: 0"><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Not now</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Add key</span></button></div><span class="docs-note">Secondary beside, main last</span></div>
</div>
```

- One main action at `touch-large` 56, full width; at most one secondary, above or beside it.
- 16 from the sides and 20 above the bottom edge: the bar's spot. A screen has the floating strip or the action bar, never both.
- With the keyboard up it rides on it, 8 above.
- Inside a machine, creation actions (New chat, New terminal, New browser) live in the plus disc's stack menu, not a bar.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `ActionBar::new(primary)` | `.penna-action-bar` | Bottom bar with one `touch-large` main action |
| `.secondary(button)` | second button above the main action | Optional secondary action |
| `.row()` | `data-row` | Places secondary beside main, main last |

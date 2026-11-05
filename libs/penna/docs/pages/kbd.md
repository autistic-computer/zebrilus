# Keycap

A keystroke drawn the way it is pressed. Menus and tooltips show shortcuts as plain text instead.

## Kinds

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="escape"></kbd><span class="docs-note">Key</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-shift-p"></kbd><span class="docs-note">Chord</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap"><span data-penna-text>1</span></kbd><span class="docs-note">Label</span></div>
</div>
```

- **Keystroke**: a key or a chord, spelled for the platform.
- **Label**: any short text in the same cap.

```rust
Kbd::new(Keystroke::parse("cmd-shift-p").unwrap_or_default())
```

## Anatomy

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-alt-shift-cmd-z"></kbd><span class="docs-note">Icons, then text</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="pageup"></kbd><span class="docs-note">Named key</span></div>
</div>
```

1. **Cap**: muted ink on a faint fill, at least as wide as it is tall.
2. **Glyphs**: modifiers and named keys as icons, ISO marks off Apple platforms.
3. **Text**: everything else, uppercase unless asked otherwise.

## Sizes

```figure uses=kbd
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>small</span><span><code>16</code></span><span><code>micro</code></span><span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k" data-size="small"></kbd></span>
  <span>medium</span><span><code>18</code></span><span><code>caption</code></span><span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></span>
</div>
```

Medium beside body text, small beside small text. Extra small draws like small, large and up like medium.

## States

One look. A keycap is read-only: it never hovers, presses or takes focus. On a fill it takes the ink that reads there.

## Sequences

One cap per keystroke, 4 apart.

```figure uses=kbd
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span></div><figcaption>A cap for each keystroke</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><kbd class="penna-kbd" data-penna="keycap"><span data-penna-text>⌃B then →</span></kbd></div><figcaption>Don't put a sequence in one cap</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Kbd::new(keystroke)` | `.penna-kbd`, `data-kbd` | Draws a keystroke |
| `Keycap::new(label)` | a `span` in the cap | Draws a fixed label |
| `.lowercase()` | `data-kbd-lowercase` | Keeps the key lowercase |
| `.small()` | `data-size="small"` | Size; medium by default |
| `.on(ground)` | `data-ground` on the surface | Paint for the ground under it |
| `.id(id)` | `aria-label` | Names it by its keystroke |
| `Kbd::text()` | `data-kbd-text` | The keystroke as text |
| `Kbd::binding_for_action(..)` | none | The keystroke bound to an action |

# Keycap

A keystroke drawn the way it is pressed: in settings, palettes, shortcuts and tooltips at small size. Menus show shortcuts as plain text instead.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">The touch size (22 px) is drawn in HTML and CSS, not built natively yet.</span>
    </div>
  </div>
</div>
```

## Kinds

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="escape"></kbd><span class="docs-note">Key</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-shift-p"></kbd><span class="docs-note">Chord</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap"><span data-penna-text>1</span></kbd><span class="docs-note">Label</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-glyph><i data-icon="check"></i></kbd><span class="docs-note">Glyph</span></div>
</div>
```

- **Keystroke**: `Kbd::new(keystroke)` draws a key or chord spelled for the platform.
- **Label**: `Keycap::new(label)` puts any short text in the same cap.
- **Glyph**: `Keycap::icon(icon)` puts one icon in a square cap, for a mark no key makes.

```rust
Kbd::new(Keystroke::parse("cmd-shift-p").unwrap_or_default())
Keycap::icon(IconName::Check).tone(Tone::Accent).emphasis(Emphasis::Solid)
```

## Anatomy

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-alt-shift-cmd-z"></kbd><span class="docs-note">Icons, then text</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="pageup"></kbd><span class="docs-note">Named key</span></div>
</div>
```

1. **Cap**: muted ink on a faint fill, at least as wide as it is tall, `space-4` inset at the sides.
2. **Glyphs**: modifiers and named keys as icons at 1.2× the text size, ISO marks off Apple platforms.
3. **Text**: everything else, uppercase unless `.lowercase()` is set.

## Sizes

```figure uses=kbd
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>small</span><span><code>16</code></span><span><code>micro</code></span><span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k" data-size="small"></kbd></span>
  <span>medium</span><span><code>18</code></span><span><code>caption</code></span><span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></span>
  <span>touch</span><span><code>22</code></span><span><code>touch-small</code></span><span><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k" data-size="touch"></kbd></span>
</div>
```

Medium beside body text, small beside small text and in tooltips, touch beside touch text. Extra small draws like small, large and xlarge like medium.

## Emphasis

Soft is the default cap. Solid lights it in a tone: a picked answer's key is solid accent.

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-size="small"><span data-penna-text>1</span></kbd><span class="docs-note">Soft</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-tone="accent" data-emphasis="solid" data-solid="accent"><span data-penna-text>2</span></kbd><span class="docs-note">Solid, lit</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-tone="accent" data-emphasis="solid" data-solid="accent" data-glyph><i data-icon="check"></i></kbd><span class="docs-note">Glyph, lit</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-emphasis="outline"><span data-penna-text>3</span></kbd><span class="docs-note">Outline</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-emphasis="ghost"><span data-penna-text>4</span></kbd><span class="docs-note">Ghost</span></div>
</div>
```

## States

```figure uses=kbd
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k" data-state="disabled"></kbd><span class="docs-note">Disabled</span></div>
  <div class="docs-cell"><kbd class="penna-kbd" data-penna="keycap" data-tone="accent" data-emphasis="solid" data-solid="accent" data-state="disabled"><span data-penna-text>2</span></kbd><span class="docs-note">Lit, disabled</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-ground type="button"><span class="penna-label" data-penna-text>Send</span><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-enter"></kbd></button><span class="docs-note">On primary fill</span></div>
</div>
```

A keycap is read-only: it never hovers, presses or takes focus. In a disabled row it dims, and a lit cap stays lit at half strength. On a fill `.on(ground)` gives it the paint and ink that read there.

## Sequences

One cap per keystroke, `gap-tight` 4 apart.

```figure uses=kbd
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span></div><figcaption>A cap for each keystroke</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><kbd class="penna-kbd" data-penna="keycap"><span data-penna-text>⌃B then →</span></kbd></div><figcaption>Don't put a sequence in one cap</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Kbd::new(keystroke)` | `.penna-kbd`, `data-kbd` | Draws a keystroke |
| `Kbd::binding_for_action(..)` | none | Keystroke bound to an action (`_in` for focus scope) |
| `kbd.text()`, `Kbd::format(&key)` | `data-kbd-text` | Spelled-out keystroke as plain text |
| `Keycap::new(label)` | `span[data-penna-text]` in cap | Draws a fixed label |
| `Keycap::icon(icon)` | `data-glyph`, `<i data-icon>` | Draws one glyph in a square cap |
| `Keycap::emphasis(..)`, `.tone(..)` | `data-emphasis`, `data-tone`, `data-solid` | Soft by default; solid lights it |
| `Keycap::disabled(true)` | `data-state="disabled"` | Dims it with its row |
| `.lowercase()` | `data-kbd-lowercase` | Keeps letters lowercase |
| `.xsmall()`, `.small()`, `.with_size(size)` | `data-size` (`xsmall`, `small`, `touch`) | Size; medium by default |
| `.on(ground)` | `data-ground` on the surface | Paint for the ground under it |
| `.id(id)` | `aria-label` | Announces the spelled-out keystroke |

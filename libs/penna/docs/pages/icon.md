# Icon

A glyph from the kit's set beside a label, or alone on an icon button in a bar where a tooltip names it. For loading use a [spinner](spinner.html); for a key, a [keycap](kbd.html); for an app logo, `IconName::Asset`.

## Set

111 glyphs in 8 groups on a 24 grid with a 1.5 stroke and no fill. Corners follow half the theme radius and redraw when the theme changes. Search by name, variant, or note, preview sizes live, and click any glyph to copy its Rust or HTML snippet.

```figure uses=chip,icon,input,segmented
<div class="docs-ic" data-docs-icons>
  <div class="docs-ic-bar">
    <label class="penna-input docs-ic-search" data-penna="field" data-size="small"><i data-icon="search"></i><input type="search" placeholder="Search 109 icons…" aria-label="Search icons" data-ic-search /></label>
    <div class="docs-ic-controls">
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="1" role="radiogroup" aria-label="Icon size" data-ic-size>
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>14</span></span>
          <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>16</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>20</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>24</span></span>
        </div>
      </div>
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Copy format" data-ic-copy-fmt>
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Rust</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>HTML</span></span>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-ic-groups" role="group" aria-label="Icon groups" data-ic-groups></div>
  <div class="docs-ic-detail" data-penna="card" data-ground data-ic-detail>
    <span class="docs-ic-detail-icon"><i data-icon="bell" data-size="large" data-ic-preview></i></span>
    <div class="docs-ic-detail-copy">
      <span class="docs-ic-detail-code" data-ic-code>IconName::Bell</span>
      <span class="docs-ic-detail-note" data-ic-note>Click any tile below to copy to clipboard.</span>
    </div>
    <span class="docs-lk-mono" data-ic-count>109 of 109</span>
  </div>
  <div class="docs-ic-grid" data-ic-grid></div>
</div>
```

- **Glyph**: `Icon::new(IconName::Bell)` draws from the kit's set for the current theme.
- **Asset**: `IconName::Asset("icons/logo.svg")` draws the app's own SVG unchanged.

```rust
Icon::new(IconName::GitBranch).small()
Icon::new(IconName::Asset("icons/logo.svg"))
```

## Tones and rotation

Icons inherit the surrounding text color by default, so inside controls they follow hover, selected and disabled with the label. Rotation turns in paint only, keeping layout still.

```figure uses=icon
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><i data-icon="bell" data-size="large" data-tone="foreground"></i><span class="docs-note">Foreground</span></div>
  <div class="docs-cell"><i data-icon="bell" data-size="large" data-tone="muted"></i><span class="docs-note">Muted</span></div>
  <div class="docs-cell"><i data-icon="sparkle" data-size="large" data-tone="accent"></i><span class="docs-note">Accent</span></div>
  <div class="docs-cell"><i data-icon="circle-check" data-size="large" data-tone="success"></i><span class="docs-note">Success</span></div>
  <div class="docs-cell"><i data-icon="triangle-alert" data-size="large" data-tone="warning"></i><span class="docs-note">Warning</span></div>
  <div class="docs-cell"><i data-icon="circle-x" data-size="large" data-tone="danger"></i><span class="docs-note">Danger</span></div>
  <div class="docs-cell"><span class="penna-inline" data-gap="related"><i data-icon="chevron-right" data-size="large"></i><i data-icon="chevron-right" data-size="large" data-turn="1"></i><i data-icon="chevron-right" data-size="large" data-turn="2"></i><i data-icon="chevron-right" data-size="large" data-turn="3"></i></span><span class="docs-note">Turns 0–3</span></div>
</div>
```

## Sizes

```figure uses=icon,label
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">px</span><span class="docs-head">Beside</span><span class="docs-head">Example</span>
  <span>xsmall, small</span><span><code>12</code></span><span><span><code>caption</code>, <code>small</code></span></span><span style="gap: var(--penna-space-4)"><i data-icon="eye" data-size="small"></i><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span></span>
  <span>medium</span><span><code>14</code></span><span><code>body</code></span><span style="gap: var(--penna-space-6)"><i data-icon="panel-right" data-size="medium"></i><span class="penna-type" data-type="body" data-penna-text>Split right</span></span>
  <span>large, xlarge, touch</span><span><code>16</code></span><span><span><code>lead</code>, <code>heading</code></span></span><span style="gap: var(--penna-space-8)"><i data-icon="monitor-plus" data-size="large"></i><span class="penna-type" data-type="lead" data-penna-text>Add host</span></span>
  <span>touch-large</span><span><code>20</code></span><span><code>touch-body</code></span><span style="gap: var(--penna-space-8)"><i data-icon="sliders" data-size="touch-large"></i><span class="penna-type" data-type="touch-body" data-penna-text>Chat options</span></span>
  <span>Size(px)</span><span><code>custom</code></span><span><code>title bar</code></span><span style="gap: var(--penna-space-6)"><i data-icon="app-window" data-size="6"></i><span class="penna-type" data-type="small" data-muted data-penna-text>24 px specimen</span></span>
</div>
```

Medium 14 is the default. Place an icon `icon_gap` before its label and drop it by `cap_center_drop(font_size, icon_size, cx)` so it centers on the label's capitals. List and tree rows keep 14 beside their text.

## Labels

Pair an icon with a word wherever there is room. Alone, only on a square icon button in a bar or row, with a tooltip that names the action.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" type="button"><i data-icon="monitor-plus"></i><span class="penna-label" data-penna-text>Add host</span></button><button class="penna-button" data-penna="button" type="button"><i data-icon="window-plus"></i><span class="penna-label" data-penna-text>New window</span></button></div><figcaption>Say what it does</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-shape="icon" type="button" aria-label="Add host"><i data-icon="monitor-plus"></i></button><button class="penna-button" data-penna="button" data-shape="icon" type="button" aria-label="New window"><i data-icon="window-plus"></i></button><button class="penna-button" data-penna="button" data-shape="icon" type="button" aria-label="Layers"><i data-icon="layers-plus"></i></button></div><figcaption>Don't make people guess</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Icon::new(IconName::Bell)` | `<i data-icon="bell">` | A glyph from the 107-icon set (`IconName::ALL`) |
| `IconName::Asset(path)` | none | The app's own SVG, drawn unchanged |
| `.xsmall()`, `.small()`, `.medium()` … `.touch_large()` | `data-size` (`xsmall` … `touch-large`, `6` for 24) | Size; medium 14 when unset |
| `.with_size(Size::Size(px))` | `--penna-icon-size` | Custom pixel size |
| `.text_color(color)` | `data-tone` | Color; inherits text color by default |
| `.transform(Transformation::rotate(..))` | `data-turn` (`1`–`3`) | Quarter turns in paint only |
| `cap_center_drop(font, icon, cx)` | `.penna-glyph` | Optical cap-height alignment / stroke parity |

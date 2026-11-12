# Icon

A glyph beside a label, or alone on an icon button in a bar. One set, drawn for the theme.

## Set

24 grid, 1.5 stroke, no fill. Corners follow the theme radius.

```figure uses=icon
<div class="docs-show" data-gap="wide">
  <i data-icon="square-terminal" data-size="6"></i><i data-icon="git-branch" data-size="6"></i><i data-icon="folder" data-size="6"></i><i data-icon="server" data-size="6"></i><i data-icon="settings" data-size="6"></i><i data-icon="bell" data-size="6"></i><i data-icon="search" data-size="6"></i><i data-icon="panel-left" data-size="6"></i><i data-icon="keyboard" data-size="6"></i><i data-icon="laptop" data-size="6"></i>
</div>
```

```rust
Icon::new(IconName::GitBranch).small()
```

## Toggles

A chrome toggle draws its outline glyph off and its fill glyph on, in the same color.

```figure uses=icon
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><i data-icon="panel-left" data-size="6"></i><span class="docs-note">Off</span></div>
  <div class="docs-cell"><i data-icon="panel-left-fill" data-size="6"></i><span class="docs-note">On</span></div>
  <div class="docs-cell"><i data-icon="chat" data-size="6"></i><span class="docs-note">Off</span></div>
  <div class="docs-cell"><i data-icon="chat-fill" data-size="6"></i><span class="docs-note">On</span></div>
</div>
```

## Sizes

```figure uses=icon,label
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">px</span><span class="docs-head">Beside</span><span class="docs-head">Example</span>
  <span>xsmall, small</span><span><code>12</code></span><span><span><code>caption</code>, <code>small</code></span></span><span style="gap: var(--penna-space-4)"><i data-icon="eye" data-size="small"></i><span class="penna-type" data-type="small" data-penna-text>Show hidden files</span></span>
  <span>medium</span><span><code>14</code></span><span><code>body</code></span><span style="gap: var(--penna-space-6)"><i data-icon="panel-right" data-size="medium"></i><span class="penna-type" data-type="body" data-penna-text>Split right</span></span>
  <span>large, xlarge, touch</span><span><code>16</code></span><span><span><code>lead</code>, <code>heading</code></span></span><span style="gap: var(--penna-space-8)"><i data-icon="server-plus" data-size="large"></i><span class="penna-type" data-type="lead" data-penna-text>Add host</span></span>
</div>
```

A control sizes its icon from its own size. List and tree rows keep 14 beside their text.

## Labels

Pair an icon with a word. Alone, only on an icon button in a bar or row, with a tooltip.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" type="button"><i data-icon="server-plus"></i><span class="penna-label" data-penna-text>Add host</span></button><button class="penna-button" data-penna="button" type="button"><i data-icon="window-plus"></i><span class="penna-label" data-penna-text>New window</span></button></div><figcaption>Say what it does</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" type="button" aria-label="Add host"><i data-icon="server-plus"></i></button><button class="penna-button" data-penna="button" type="button" aria-label="New window"><i data-icon="window-plus"></i></button><button class="penna-button" data-penna="button" type="button" aria-label="Layers"><i data-icon="layers-plus"></i></button></div><figcaption>Don't make people guess</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Icon::new(IconName::Bell)` | `<i data-icon="bell">` | A glyph from the set |
| `IconName::Asset(path)` | none | The app's own file, as it is |
| `.xsmall()`, `.small()`, `.medium()`, `.large()` | `data-size` | Size; medium (14) when unset |
| `.with_size(Size::Size(px))` | `--penna-icon-size` | Any size |
| `.text_color(color)` | `data-tone` | Color; the text color by default |
| `.transform(Transformation::rotate(..))` | `data-turn` | Turns it, paint only |

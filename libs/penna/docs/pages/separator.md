# Separator

A one pixel line between things that takes no room. Try a gap first; for a line on an element's edge use `.rule()`.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>A custom color takes any color, not only the theme's.</span></div></div></div>
```

## Anatomy

```figure uses=separator,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-stack" data-gap="related" style="width: 200px"><span class="penna-type" data-type="lead" data-penna-text>Sessions</span><div class="penna-separator-rule" role="separator"><div data-penna="rule"></div></div><span class="penna-type" data-type="lead" data-muted data-penna-text>work, scratch, logs</span></div><span class="docs-note">Horizontal</span></div>
  <div class="docs-cell"><div class="penna-inline" style="height: 20px; gap: var(--penna-space-12)"><span class="penna-type" data-type="lead" data-penna-text>macbook</span><div class="penna-separator-rule" data-axis="vertical" role="separator" aria-orientation="vertical"><div data-penna="rule"></div></div><span class="penna-type" data-type="lead" data-penna-text>work</span></div><span class="docs-note">Vertical</span></div>
</div>
```

1. **Line**: 1px in the divider ink, centered on its box.
2. **Box**: no room along its axis, so the gaps around it are the parent's.
3. **Vertical**: needs a parent with a set height.

```rust
Separator::horizontal().look(RuleLook::List)
```

## Looks

- **Plain**: between panes.
- **Bar**: under bars, over footers.
- **List**: between menu groups, setting rows and stacked cards.
- **Tree**: the guides that join a tree's children.

All four draw a hairline in the same ink.

## Use

Group with space first. A line between every row is noise.

```figure uses=separator,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="group" style="width: 160px"><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-penna-text>work</span><span class="penna-type" data-type="body" data-penna-text>scratch</span></div><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-penna-text>logs</span><span class="penna-type" data-type="body" data-penna-text>builds</span></div></div></div><figcaption>A gap between groups</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-stack" data-gap="related" style="width: 160px"><span class="penna-type" data-type="body" data-penna-text>work</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>scratch</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>logs</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>builds</span></div></div><figcaption>Don't rule every row</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Separator::horizontal()` | `.penna-separator-rule` | A horizontal line |
| `Separator::vertical()` | `data-axis="vertical"` | A vertical line |
| `.look(RuleLook::List)` | inner `data-penna="rule.list"` | Look: `rule`, `rule.bar`, `rule.list`, `rule.tree` |
| `.color(color)` | `data-color="danger"` | Line color |
| `.id(id)` | `role="separator"`, `aria-orientation` | An accessibility node |
| `.rule(look, edge, cx)` | `data-rule` | A line on an element's edge |

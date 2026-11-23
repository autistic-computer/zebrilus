# Separator

A one pixel line between things that takes no room. Try a gap first; for a line on an element's edge use `.rule()`.

## Anatomy

```figure uses=separator,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-stack" data-gap="related" style="width: 200px"><span class="penna-type" data-type="lead" data-penna-text>Sessions</span><div class="penna-separator-rule" role="separator"><div data-penna="rule"></div></div><span class="penna-type" data-type="lead" data-muted data-penna-text>work, scratch, logs</span></div><span class="docs-note">Horizontal</span></div>
  <div class="docs-cell"><div class="penna-inline" style="height: 20px; gap: var(--penna-space-12)"><span class="penna-type" data-type="lead" data-penna-text>macbook</span><div class="penna-separator-rule" data-axis="vertical" role="separator" aria-orientation="vertical"><div data-penna="rule"></div></div><span class="penna-type" data-type="lead" data-penna-text>work</span></div><span class="docs-note">Vertical</span></div>
</div>
```

1. **Line**: 1px in the divider ink, centered on its box.
2. **Box**: no room along its axis, so the gaps around it stay the parent's. Unlike `Separator`, `.rule(look, edge, cx)` takes 1px of layout room along the edged side.
3. **Vertical**: needs a parent with a set height.
4. **Accessibility**: `.id(id)` announces a non-draggable splitter (`role="separator"`).

```rust
Separator::horizontal().look(RuleLook::List)
```

## Looks and edge rules

```figure uses=panel,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-panel" data-penna="panel" data-ground style="width: 200px; overflow: hidden; box-shadow: inset 0 0 0 1px var(--penna-divider)"><div data-rule="rule.bar" style="padding: var(--penna-space-8) var(--penna-inset-card)"><span class="penna-type" data-type="body" data-strong data-penna-text>Bar rule</span></div><div style="padding: var(--penna-space-8) var(--penna-inset-card)"><span class="penna-type" data-type="small" data-muted data-penna-text>1 px along the edged side</span></div></div><span class="docs-note">Edge rule (bottom)</span></div>
  <div class="docs-cell"><div class="penna-panel" data-penna="settings.card" data-ground style="width: 200px; overflow: hidden"><div style="padding: var(--penna-space-8) var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>First row</span></div><div style="margin-left: var(--penna-inset-card); border-top: 1px solid var(--penna-divider)"></div><div style="padding: var(--penna-space-8) var(--penna-inset-card)"><span class="penna-type" data-type="body" data-penna-text>Second row</span></div></div><span class="docs-note">Inset list rule (12 in)</span></div>
</div>
```

- **Plain**: between panes (default).
- **Bar**: under bars, over footers.
- **List**: between menu groups, setting rows and stacked cards; in settings cards and ruled touch groups, list rules start `inset-card` 12 in where text begins and run to the far edge.
- **Tree**: the guides that join a tree's children.

All four draw a hairline in the same divider ink.

## Tone

`.tone(tone)` draws the line in a tone's ink (`Tone::Neutral` by default). A status tone comes with a word.

```figure uses=separator,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="related" style="width: 160px"><span class="penna-type" data-type="body" data-penna-text>Danger zone</span><div class="penna-separator-rule" data-tone="danger" role="separator"><div data-penna="rule"></div></div><span class="penna-type" data-type="body" data-muted data-penna-text>Kill every session</span></div></div><figcaption>A word with the tone</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-stack" data-gap="related" style="width: 160px"><span class="penna-type" data-type="body" data-penna-text>work</span><div class="penna-separator-rule" data-tone="success" role="separator"><div data-penna="rule"></div></div><span class="penna-type" data-type="body" data-penna-text>scratch</span></div></div><figcaption>Don't color a line to decorate</figcaption></figure>
</div>
```

## Use

Group with space first. A line between every row is noise.

```figure uses=separator,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-stack" data-gap="group" style="width: 160px"><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-penna-text>work</span><span class="penna-type" data-type="body" data-penna-text>scratch</span></div><div class="penna-stack" data-gap="tight"><span class="penna-type" data-type="body" data-penna-text>logs</span><span class="penna-type" data-type="body" data-penna-text>builds</span></div></div></div><figcaption>A gap between groups</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-stack" data-gap="related" style="width: 160px"><span class="penna-type" data-type="body" data-penna-text>work</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>scratch</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>logs</span><div class="penna-separator-rule" role="separator"><div data-penna="rule.list"></div></div><span class="penna-type" data-type="body" data-penna-text>builds</span></div></div><figcaption>Don't rule every row</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Separator::horizontal()` | `.penna-separator-rule` | A horizontal line |
| `Separator::vertical()` | `data-axis="vertical"` | A vertical line |
| `.look(RuleLook::List)` | inner `data-penna="rule.list"` | Look (`RuleLook::Plain` default): `rule`, `rule.bar`, `rule.list`, `rule.tree` |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Ink (`Tone::Neutral` default): `primary`, `danger`, `warning`, `success`, `accent` |
| `.id(id)` | `role="separator"`, `aria-orientation` | Announces a non-draggable splitter |
| `.rule(look, edge, cx)` | `data-rule="rule.list"`, `data-edge="top left"` | 1 px edge rule; space-separated edges, bottom when absent |

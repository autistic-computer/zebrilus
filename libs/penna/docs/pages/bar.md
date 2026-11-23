# Bar

A strip across a sheet, a pane or the window that holds chrome. For a box use a [card](card.html), for a line a [separator](separator.html).

## Looks

```figure uses=panel,bar,button,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Pane</span>
      <span class="docs-lk-mono">bar.pane</span>
    </div>
    <div class="penna-panel" data-penna="picker" role="group" style="width: 100%; overflow: hidden">
      <div class="penna-bar" data-penna="bar.pane" role="toolbar" data-rule="rule.bar" style="justify-content: space-between">
        <span class="penna-type" data-type="body" data-penna-text>zsh · ~/dev</span>
        <span class="penna-inline" data-gap="related"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Split right" data-tooltip="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Close" data-tooltip="Close"><i data-icon="xmark"></i></button></span>
      </div>
      <div style="padding: var(--penna-inset-card)"><span class="penna-type" data-type="mono" data-muted data-penna-text>$ cargo test --all</span></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Plain on panel</span>
      <span class="docs-lk-mono">bar · on(ground)</span>
    </div>
    <div class="penna-panel" data-penna="panel" data-ground role="group" style="width: 100%; overflow: hidden; box-shadow: inset 0 0 0 1px var(--penna-divider)">
      <div class="penna-bar" data-penna="bar" role="toolbar" data-rule="rule.bar" style="justify-content: space-between">
        <span class="penna-type" data-type="body" data-strong data-penna-text>Connections</span>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Close" data-tooltip="Close"><i data-icon="xmark"></i></button>
      </div>
      <div style="padding: var(--penna-inset-card)"><span class="penna-type" data-type="small" data-muted data-penna-text>Takes the surface beneath it</span></div>
    </div>
  </div>
</div>
```

- **Plain**: sheet headers, a strip over a panel.
- **Pane**: pane headers, browser tabs and toolbars.
- **Window**: the strip across the top of the window.

None paints a fill. The edge rule sets it apart.

```rust
Bar::new("header", BarLook::Pane)
    .flex()
    .items_center()
    .rule(RuleLook::Bar, Edge::Bottom, cx)
    .child(title)
    .child(actions)
```

## Anatomy

1. **Height**: 40 with flush panes, 34 with pane margins. The bar sets it.
2. **Inset**: `inset-chrome` 6 at the sides. The bar sets it; a caller's height or padding replaces both.
3. **Controls**: ghost, one size, `gap-related` 8 apart; `control-chrome` 22 in pane headers.
4. **Edge**: a bar rule where it meets content.

## Controls

Every control in a bar has the same size.

```figure uses=bar,button
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-bar penna-inline" data-penna="bar" role="toolbar" data-rule="rule.bar" data-gap="related" style="width: 100%; justify-content: flex-end"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="More"><i data-icon="ellipsis"></i></button></div></div><figcaption>One size</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-bar penna-inline" data-penna="bar" role="toolbar" data-rule="rule.bar" data-gap="related" style="width: 100%; justify-content: flex-end"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="large" type="button" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button.quiet" data-size="xsmall" type="button"><span class="penna-label" data-penna-text>More</span></button></div></div><figcaption>Don't mix sizes</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Bar::new(id, BarLook::Plain)` | `.penna-bar`, `data-penna="bar"`, `role="toolbar"` | Plain look |
| `BarLook::Pane`, `Window` | `bar.pane`; `titlebar` with `role="banner"` | Other looks |
| `.h(px)`, `.px(space)` | `height`, `padding` | Replace the height or the inset |
| `.rule(RuleLook::Bar, Edge::Bottom, cx)` | `data-rule="rule.bar"`, `data-edge` | Edge rule; bottom when `data-edge` is absent |
| `.on(ground)` | `data-ground` on parent | Puts it on another ground |

# Bar

A strip across a sheet, a pane or the window that holds chrome. For a box use a card, for a line a separator.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>A bar sets no height or inset, so callers pick their own. In HTML no kit stylesheet draws its edge, and it isn't announced as a toolbar.</span></div></div></div>
```

## Looks

```figure uses=panel,bar,separator,label
<div class="penna-panel" data-penna="picker" style="width: 420px">
  <div class="penna-bar" data-penna="bar.pane" role="toolbar" style="justify-content: space-between; height: 40px; padding: 0 var(--penna-inset-chrome) 0 var(--penna-space-12)">
    <span class="penna-type" data-type="body" data-penna-text>zsh · ~/dev</span>
    <span class="penna-inline" data-gap="related"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Close"><i data-icon="xmark"></i></button></span>
  </div>
  <div class="penna-separator-rule" role="separator"><div data-penna="rule.bar"></div></div>
  <div style="height: 64px"></div>
</div>
```

- **Plain**: sheet headers, a strip over a panel.
- **Pane**: pane headers.
- **Window**: the title bar.

None paints a fill on the shipped themes. The edge rule sets it apart.

```rust
Bar::new("header", BarLook::Pane)
    .h(cx.theme().pane_header_height())
    .px(Space::INSET_CHROME)
    .rule(RuleLook::Bar, Edge::Bottom, cx)
```

## Anatomy

1. **Height**: 40 with flush panes, 34 with pane margins.
2. **Inset**: `inset-chrome` 6 around its controls.
3. **Controls**: ghost, one size, `gap-related` 8 apart.
4. **Edge**: a bar rule where it meets content.

## Controls

Every control in a bar has the same size.

```figure uses=bar
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-bar penna-inline" data-penna="bar" data-gap="related"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="More"><i data-icon="ellipsis"></i></button></div></div><figcaption>One size</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-bar penna-inline" data-penna="bar" data-gap="related"><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" aria-label="Split right"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="large" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button.quiet" data-size="xsmall"><span class="penna-label" data-penna-text>More</span></button></div></div><figcaption>Don't mix sizes</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Bar::new(id, BarLook::Plain)` | `.penna-bar`, `data-penna="bar"` | Plain look |
| `BarLook::Pane`, `Window` | `bar.pane`, `titlebar` | Other looks |
| `.h(cx.theme().pane_header_height())` | | Height; the caller's |
| `.px(Space::INSET_CHROME)` | | Inset; the caller's |
| `.rule(RuleLook::Bar, Edge::Bottom, cx)` | `data-rule="rule.bar"` | Edge rule, where the theme draws one |
| `.on(ground)` | | Puts it on another ground |

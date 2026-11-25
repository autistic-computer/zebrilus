# Hover card

A delayed floating preview surface for inspecting a host, commit, or symbol on pointer hover or keyboard focus without opening a modal popover.

## Delayed preview

Hover or focus the trigger below to inspect the preview card (`240 ms` open delay, `140 ms` close delay). Moving the pointer onto the card keeps it open.

```figure uses=hover_card,badge,icon,label
<div style="padding-bottom: 132px">
  <div class="penna-hover-card">
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button"><i data-icon="git-branch"></i><span class="penna-label" data-penna-text>e5c2c7e · main</span></button>
    <div class="penna-hover-card-panel" data-penna="popover" data-ground role="tooltip">
      <div class="docs-row">
        <span class="penna-type" data-type="small" data-strong>Showcase &amp; parity badges</span>
        <span class="docs-spacer"></span>
        <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text">95.1%</span></span>
      </div>
      <span class="penna-type" data-type="caption" data-muted>16 files changed · 34 storybook baselines verified across 6 theme × mode pairs.</span>
    </div>
  </div>
</div>
```

```rust
HoverCard::new("commit-preview")
    .trigger(Button::new("sha").ghost().small().label("e5c2c7e"))
    .content(commit_summary_card)
```

## API (`penna::HoverCard`)

| Rust | HTML (`uses=hover_card`) | Role |
| --- | --- | --- |
| `HoverCard::new(id)` | `.penna-hover-card` | Hover/focus trigger + floating card host |
| `.trigger(el)` | First child trigger | Element that opens the preview after 240 ms |
| `.content(el)` | `.penna-hover-card-panel[data-penna="popover"]` | Liquid-glass floating card keeping hover alive |
| `.anchor(Anchor)` | CSS positioning | Anchor corner snapped to window bounds |

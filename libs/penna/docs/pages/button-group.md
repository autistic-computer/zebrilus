# Button group

An attached horizontal or vertical cluster of related actions sharing one field container and quiet hairline dividers. For exclusive mode switches, use [Segmented](segmented.html).

## Horizontal and vertical clusters

```figure uses=button_group,button,icon
<div class="docs-show" data-gap="wide">
  <div class="penna-button-group" data-penna="field" data-ground role="group" aria-label="Pane layout">
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button"><i data-icon="panel-left"></i><span class="penna-label" data-penna-text>Left</span></button>
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button"><i data-icon="panel-right"></i><span class="penna-label" data-penna-text>Right</span></button>
    <button class="penna-button" data-penna="button.quiet" data-size="small" type="button"><i data-icon="panel-bottom"></i><span class="penna-label" data-penna-text>Bottom</span></button>
  </div>
  <div class="penna-button-group" data-penna="field" data-ground data-orientation="vertical" role="group" aria-label="Zoom controls">
    <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Zoom in"><i data-icon="plus"></i></button>
    <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Zoom out"><i data-icon="minus"></i></button>
  </div>
</div>
```

```rust
ButtonGroup::new()
    .child(Button::new("left").ghost().small().icon(IconName::PanelLeft).label("Left"))
    .child(Button::new("right").ghost().small().icon(IconName::PanelRight).label("Right"))
    .child(Button::new("bottom").ghost().small().icon(IconName::PanelBottom).label("Bottom"))
```

## API (`penna::ButtonGroup`)

| Rust | HTML (`uses=button_group`) | Role |
| --- | --- | --- |
| `ButtonGroup::new()` | `.penna-button-group[data-penna="field"]` | Shared outer container for attached actions |
| `.vertical()` / `.orientation(...)` | `data-orientation="vertical"` | Stack buttons vertically with horizontal dividers |
| `.dividers(bool)` | inner hairlines | Show or hide `RuleLook::List` rules between items |
| `.on(ground)` | `data-ground` | Ground propagation for nested panels |

# Tooltip

Names a control on hover, keyboard focus or a long press. For more than a phrase, use a [popover](popover.html).

## Anatomy

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" data-ground role="tooltip" style="margin: 0"><span data-penna-text>Notifications</span></div><span class="docs-note">Label</span></div>
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" data-ground role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-d"></kbd></span></div><span class="docs-note">Label and key</span></div>
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" data-ground role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="ctrl-a"></kbd><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="|"></kbd></span></div><span class="docs-note">Prefix, then a key</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" aria-label="Split right" data-tooltip="Split right" data-tooltip-key="ctrl-a |"><i data-icon="panel-right"></i></button><span class="docs-note">Hover or tab to it</span></div>
</div>
```

1. **Box**: floating `tooltip` surface (`space-4` × `space-10` inset), `small` text, one line.
2. **Label**: names the action or the thing.
3. **Key**: optional `small` 16 px keycaps on the tooltip's ground, `space-12` after the label and `gap-tight` 4 apart.

```rust
Button::new("split")
    .icon(IconName::PanelRight)
    .tooltip("Split right")
    .tooltip_keys([prefix, pipe_key])

Tooltip::new("Split right")
    .action(&SplitRight, Some("Pane"))
    .build(window, cx)
```

## Behavior

- **Hover**: shows 500ms after the pointer rests, 13 px clear of the cursor (`1px + space-12`), and flips at the window edge. Only one tooltip shows at a time; disabled buttons still show theirs (use `aria-disabled="true"` in HTML).
- **Keyboard focus**: `Button::tooltip` shows at once below the button's middle on keyboard focus (`Switch::tooltip` and element `.tooltip(...)` show on hover). Hides on Escape (without consuming the key), pointer move, or blur.
- **Touch**: shows after a 500ms press at the finger until lifted; cancels if the finger moves past 8 px.

## Wording

A few words in sentence case, no period. Required on icon-only buttons (doubles as the accessible name); don't repeat a visible label unless adding its shortcut. Give the key through `.action()` or `.keys()`, never inside the text.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-tooltip" data-penna="tooltip" data-ground role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-d"></kbd></span></div></div><figcaption>Name the action</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-tooltip" data-penna="tooltip" data-ground role="tooltip" style="margin: 0"><span data-penna-text>Click here to split the pane (⌘D).</span></div></div><figcaption>Don't write instructions</figcaption></figure>
</div>
```

## API (`penna::components::tooltip::Tooltip`)

| Rust | HTML | Does |
| --- | --- | --- |
| `Tooltip::new(text)` | `data-tooltip` | Label |
| `.action(&action, Option<&str>)` | `data-tooltip-key` | Keys bound to an action (overridden by `.keys()`) |
| `.keys(strokes)`, `.tooltip_keys(strokes)` | `data-tooltip-key="ctrl-a \|"` | Explicit keystroke sequence |
| `.tooltip(text)` on `Button` or `Switch` | `data-tooltip` | Tooltip on a control; required when icon-only |
| `.build(window, cx)` | `.penna-tooltip[data-penna="tooltip"][data-ground][data-floating][role="tooltip"]` | Floating box (`data-open`, `data-fixed`, `data-tooltip-host`, `data-tooltip-pinned`) |

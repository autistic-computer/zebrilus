# Tooltip

Names a control on hover, keyboard focus or a long press. For more than a phrase, use a popover.

## Anatomy

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" role="tooltip" style="margin: 0"><span data-penna-text>Notifications</span></div><span class="docs-note">Label</span></div>
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-d"></kbd></span></div><span class="docs-note">Label and key</span></div>
  <div class="docs-cell"><div class="penna-tooltip" data-penna="tooltip" role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="ctrl-a"></kbd><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="|"></kbd></span></div><span class="docs-note">Prefix, then a key</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" aria-label="Split right" data-tooltip="Split right" data-tooltip-key="cmd-d"><i data-icon="panel-right"></i></button><span class="docs-note">Hover or tab to it</span></div>
</div>
```

1. **Box**: a floating surface, small text, one line.
2. **Label**: names the action or the thing.
3. **Key**: optional, a small keycap per keystroke after the label.

```rust
Button::new("split")
    .icon(IconName::PanelRight)
    .tooltip("Split right")
    .tooltip_keys([prefix, Keystroke::parse("|")?])
```

## Behavior

- Shows 500ms after the pointer rests, just below and right of it.
- Shows at once when the keyboard focuses the control, below its middle.
- On touch, shows after a 500ms press, at the finger, until it lifts.
- Flips at the window edge.
- Hides when the pointer leaves or presses. After keyboard focus it hides on Escape, a pointer move or when focus leaves.

## Wording

A few words in sentence case, no period. Give the key with the action or the keys, never in the text.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-tooltip" data-penna="tooltip" role="tooltip" style="margin: 0"><span data-penna-text>Split right</span><span class="penna-tooltip-key"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-d"></kbd></span></div></div><figcaption>Name the action</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-tooltip" data-penna="tooltip" role="tooltip" style="margin: 0"><span data-penna-text>Click here to split the pane (⌘D).</span></div></div><figcaption>Don't write instructions</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Tooltip::new(text)` | `data-tooltip` | Label |
| `.action(&action, context)` | `data-tooltip-key` | Keys bound to the action |
| `.keys(strokes)`, `.tooltip_keys(strokes)` on a button | `data-tooltip-key="ctrl-a \|"` | A sequence the app binds itself |
| `.tooltip(text)` on a button or switch | `data-tooltip` | Tooltip on a control; required with only an icon |
| `.build(window, cx)` | `.penna-tooltip[data-floating]` | The floating box, for any element's `.tooltip` |

# Popover

A panel that opens from a button for one short task. For a list of actions use a menu, for a hint a tooltip.

## Anatomy

```figure uses=popover,input
<div style="height: 148px">
  <div class="penna-popover" data-open>
    <button class="penna-button penna-popover-trigger" data-penna="button" data-state="current" type="button" aria-haspopup="dialog"><i data-icon="pencil"></i><span class="penna-label" data-penna-text>Rename</span></button>
    <div class="penna-popover-panel" data-penna="popover" tabindex="-1" role="dialog" aria-label="Rename session">
      <div class="penna-stack" data-gap="related" style="width: 260px">
        <span class="penna-type" data-type="small" data-muted data-penna-text>Session name</span>
        <label class="penna-input" data-penna="field"><input value="scratch" aria-label="Session name" /></label>
        <div class="penna-inline" data-gap="related" style="justify-content: flex-end">
          <button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

1. **Trigger**: a button. Selected while open.
2. **Panel**: the popover look, `inset-card` 12, `body` text.
3. **Offset**: `space-4` below the trigger, leading edges aligned.

`.anchor()` picks the corner. Bottom anchors open above, center anchors center on the trigger.

```figure uses=popover,label
<div style="height: 96px; width: 320px; display: flex; align-items: flex-start; justify-content: center">
  <div class="penna-popover" data-anchor="center" data-open>
    <button class="penna-button penna-popover-trigger" data-penna="button" data-state="current" type="button" aria-haspopup="dialog"><span class="penna-label" data-penna-text>Center</span></button>
    <div class="penna-popover-panel" data-penna="popover" tabindex="-1" role="dialog" aria-label="Centered popover"><span class="penna-type" data-type="body" data-penna-text>Opens centered on its button</span></div>
  </div>
</div>
```

```rust
Popover::new("rename")
    .trigger(Button::new("open").label("Rename"))
    .content(|_, _, _| rename_form())
```

## Behavior

- A press on the trigger, or Enter, Space or Down while it has focus, opens it; the same again closes it.
- Focus moves in, and back to the trigger on close.
- Escape or a press outside closes it.
- `.open()` lets the app open and close it from code; presses and Escape then ask it through `.on_toggle()`.
- It stays 8 inside the window and moves up for the keyboard.

## Content

One short task with its actions. A list of actions belongs in a menu.

```figure uses=panel,input,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="popover" style="width: 220px"><div class="penna-stack" data-gap="related" style="padding: var(--penna-inset-card)"><label class="penna-input" data-penna="field"><input value="scratch" aria-label="Session name" /></label><div class="penna-inline" data-gap="related" style="justify-content: flex-end"><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button></div></div></div></div><figcaption>A small form</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="popover" style="width: 220px"><div class="penna-stack" data-gap="tight" style="padding: var(--penna-inset-card)"><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Copy path</span></button><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Rename</span></button><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Close</span></button></div></div></div><figcaption>Don't build a menu from buttons</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Popover::new(id)` | `.penna-popover` | Holds the trigger and the panel |
| `.trigger(button)` | `.penna-popover-trigger` | Opens it; selected while open |
| `.content(builder)` | `.penna-popover-panel`, `data-penna="popover"` | What the panel holds |
| `.anchor(anchor)` | `data-anchor`: `right`, `center`, `bottom`, `bottom right`, `bottom center` | Corner it hangs from |
| `.overlay_closable(false)` | `data-closable="false"` | Stays open on a press outside |
| `.on_dismiss(handler)` | `penna-dismiss` | Runs when it closes |
| `.open(open)`, `.on_toggle(handler)` | `pennaPopover.setOpen()` | The app holds the open state; presses and Escape ask through `on_toggle` |
| `.appearance(false)` | | Drops the look, inset and text style |
| | `data-open`, `pennaPopover.setOpen()` | Open state |

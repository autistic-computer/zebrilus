# Popover

A panel that opens from a button for one short task. For a list of actions use a [menu](menu.html), for a hint a [tooltip](tooltip.html).

## Anatomy

```figure uses=popover,input,label
<div style="min-height: 168px">
  <div class="penna-popover" data-open data-closable="false">
    <button class="penna-button penna-popover-trigger" data-penna="button" data-state="current" type="button" aria-haspopup="dialog" aria-expanded="true"><i data-icon="pencil"></i><span class="penna-label" data-penna-text>Rename</span></button>
    <div class="penna-popover-panel" data-penna="popover" data-ground tabindex="-1" role="dialog" aria-label="Rename session">
      <div class="penna-stack" data-gap="related" style="width: min(260px, calc(100vw - 64px))">
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

1. **Trigger**: a button, selected while the popover is open.
2. **Panel**: the `popover` look with `inset-card` 12 and `body` text; `.appearance(false)` drops the look, inset and text style for custom surfaces.
3. **Content**: your form or controls, with corners concentric inside the panel.
4. **Offset**: `space-4` below the trigger, leading edges aligned.

`.anchor()` picks the corner. Bottom anchors open above, center anchors center on the trigger.

```figure uses=popover,label
<div style="min-height: 104px; width: min(100%, 320px); display: flex; align-items: flex-start; justify-content: center">
  <div class="penna-popover" data-anchor="center" data-open data-closable="false">
    <button class="penna-button penna-popover-trigger" data-penna="button" data-state="current" type="button" aria-haspopup="dialog" aria-expanded="true"><span class="penna-label" data-penna-text>Center</span></button>
    <div class="penna-popover-panel" data-penna="popover" data-ground tabindex="-1" role="dialog" aria-label="Centered popover"><span class="penna-type" data-type="body" data-penna-text>Opens centered on its button</span></div>
  </div>
</div>
```

```rust
Popover::new("rename")
    .trigger(Button::new("open").label("Rename"))
    .content(|state, window, cx| {
        rename_form(move |window, cx| state.dismiss(window, cx))
    })
```

## States and behavior

- **Closed and open**: the trigger draws selected while open; the panel itself has no hover or pressed look.
- **Toggle**: a press on the trigger, or Enter, Space or Down while it has focus, opens it; the same again closes it.
- **Focus**: moves into the panel on open, and returns to whatever held focus before opening on close as long as focus is still inside the panel.
- **Dismiss**: Escape or a press outside closes it (unless `.overlay_closable(false)`). Calling `state.dismiss(window, cx)` closes from inside `.content(..)`.
- **Controlled**: `.open()` lets the app hold open state; presses and Escape then ask through `.on_toggle()`.
- **Clamping and motion**: stays 8 inside the window and moves up for the keyboard. Fades in moving 6 into place over 160ms, and out moving 6 back over 100ms, ignoring input while leaving.

## Content

One short task with its actions. A list of actions belongs in a [menu](menu.html).

```figure uses=panel,input,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-panel" data-penna="popover" data-ground style="width: min(100%, 220px)"><div class="penna-stack" data-gap="related" style="padding: var(--penna-inset-card)"><label class="penna-input" data-penna="field"><input value="scratch" aria-label="Session name" /></label><div class="penna-inline" data-gap="related" style="justify-content: flex-end"><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button></div></div></div></div><figcaption>A small form</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-panel" data-penna="popover" data-ground style="width: min(100%, 220px)"><div class="penna-stack" data-gap="tight" style="padding: var(--penna-inset-card)"><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Copy path</span></button><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Rename</span></button><button class="penna-button" data-penna="button.quiet" type="button"><span class="penna-label" data-penna-text>Close</span></button></div></div></div><figcaption>Don't build a menu from buttons</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Popover::new(id)` | `.penna-popover` | Holds the trigger and the panel |
| `.trigger(button)` | `.penna-popover-trigger`, `aria-haspopup="dialog"`, `data-state="current"` | Opens it; selected while open |
| `.content(\|state, window, cx\| ..)` | `.penna-popover-panel[data-penna="popover"][data-ground]` | Dialog body (`role="dialog"`, `tabindex="-1"`) |
| `state.dismiss(window, cx)`, `state.is_open()` | `pennaPopover.setOpen(el, false)` | Closes or queries the popover from content |
| `.anchor(anchor)` | `data-anchor`: `right`, `center`, `bottom`, `bottom right`, `bottom center` | Corner it hangs from |
| `.overlay_closable(false)` | `data-closable="false"` | Stays open on a press outside |
| `.on_dismiss(handler)` | `penna-open`, `penna-dismiss` | Fires when open state changes |
| `.open(open)`, `.on_toggle(handler)` | `data-open`, `pennaPopover.setOpen(el, open)` | Controlled open state |
| `.appearance(false)` | | Drops the look, inset and text style |

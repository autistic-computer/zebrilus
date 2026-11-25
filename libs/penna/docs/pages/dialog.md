# Dialog

A modal window over the current view for a focused decision or short form. Use `AlertDialog` when the choice is destructive or must be answered before clicking away.

## Standard and alert dialogs

`Dialog` opens through `WindowExt::open_dialog` on a floating glass surface (`dialog`), with an `inset-pane` (16 px) gutter, a trailing close button, and actions aligned to the end with the primary button last. `AlertDialog` locks out scrim clicks and removes the close button so the user answers explicitly.

```figure uses=dialog,input
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">Dialog</span><span class="docs-pr-col-meta">Form · closable</span></div>
    <div class="docs-dodont-panel" data-penna="panel" data-ground style="padding: 20px 16px">
      <div class="penna-dialog" data-penna="dialog" data-ground role="dialog" aria-label="Rename session" style="width: min(100%, 300px); max-height: none">
        <div class="penna-dialog-main">
          <div class="penna-dialog-gutter"><div class="penna-dialog-title"><span data-penna-text>Rename session</span></div></div>
          <div class="penna-dialog-body">
            <div class="penna-dialog-scroll" style="gap: 8px">
              <div class="penna-dialog-description">Names appear in the sidebar and window title.</div>
              <label class="penna-input" data-penna="field"><input value="scratch" aria-label="Session name" /></label>
            </div>
          </div>
        </div>
        <div class="penna-dialog-gutter">
          <div class="penna-dialog-footer">
            <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
            <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Rename</span></button>
          </div>
        </div>
        <button class="penna-button penna-dialog-close" data-penna="icon-button" data-shape="icon" type="button" aria-label="Close"><i data-icon="xmark"></i></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head"><span class="docs-pr-col-title">AlertDialog</span><span class="docs-pr-col-meta">Destructive · explicit</span></div>
    <div class="docs-dodont-panel" data-penna="panel" data-ground style="padding: 20px 16px">
      <div class="penna-dialog" data-penna="dialog" data-ground role="alertdialog" aria-label="Remove host" style="width: min(100%, 300px); max-height: none">
        <div class="penna-dialog-main">
          <div class="penna-dialog-gutter">
            <div class="penna-dialog-head">
              <i data-icon="triangle-alert" data-tone="danger"></i>
              <div class="penna-dialog-lines">
                <div class="penna-dialog-title"><span data-penna-text>Remove host?</span></div>
                <div class="penna-dialog-description">Saved trust keys for ubuntu will be cleared.</div>
              </div>
            </div>
          </div>
        </div>
        <div class="penna-dialog-gutter">
          <div class="penna-dialog-footer">
            <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
            <button class="penna-button" data-penna="button" data-tone="danger" data-prominent type="button"><span class="penna-label" data-penna-text>Remove</span></button>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

```rust
window.open_dialog(cx, |dialog, _, cx| {
    dialog
        .title("Rename session")
        .button_props(
            DialogButtonProps::default()
                .ok_text("Rename")
                .show_cancel(true),
        )
        .on_ok(|_, _, _| true)
        .child(Input::new(&name_state))
});

window.open_alert_dialog(cx, |alert, _, _| {
    alert
        .icon(Icon::new(IconName::TriangleAlert).danger())
        .title("Remove host?")
        .description("Saved trust keys for ubuntu will be cleared.")
        .button_props(
            DialogButtonProps::default()
                .ok_text("Remove")
                .ok_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Solid))
                .show_cancel(true),
        )
});
```

## Live stage and keyboard trapping

Inside a `.penna-dialog-stage`, `dialog.js` manages the backdrop scrim, stacking offsets (`--dialog-offset`), focus trapping on `Tab`, `Enter` to confirm, and `Escape` to dismiss. Click a trigger below to open and dismiss a live modal inside the frame.

```figure uses=dialog
<div class="penna-dialog-stage" style="position: relative; width: 100%; height: 248px; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); overflow: hidden; background: var(--penna-panel); display: flex; align-items: center; justify-content: center; gap: 8px">
  <button class="penna-button" data-penna="button" type="button" data-dialog-open="docs-live-dialog"><span class="penna-label" data-penna-text>Open dialog</span></button>
  <div class="penna-dialog-layer" id="docs-live-dialog">
    <div class="penna-dialog-place">
      <div class="penna-dialog-room">
        <div class="penna-dialog" data-penna="dialog" data-ground role="dialog" aria-label="Disconnect session" style="width: min(100%, 300px)">
          <div class="penna-dialog-main">
            <div class="penna-dialog-gutter"><div class="penna-dialog-title"><span data-penna-text>Disconnect session?</span></div></div>
            <div class="penna-dialog-body">
              <div class="penna-dialog-scroll">
                <div class="penna-dialog-description">Running panes keep alive on the remote host until you attach again.</div>
              </div>
            </div>
          </div>
          <div class="penna-dialog-gutter">
            <div class="penna-dialog-footer">
              <button class="penna-button" data-penna="button" type="button" data-dialog-cancel><span class="penna-label" data-penna-text>Stay</span></button>
              <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button" data-dialog-ok><span class="penna-label" data-penna-text>Disconnect</span></button>
            </div>
          </div>
          <button class="penna-button penna-dialog-close" data-penna="icon-button" data-shape="icon" type="button" aria-label="Close" data-dialog-close><i data-icon="xmark"></i></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

- **Callbacks decide close:** `.on_ok` and `.on_cancel` return `bool`; returning `false` keeps the dialog open when validation fails.
- **Label verbs clearly:** pass explicit verb text via `.ok_text(...)` rather than falling back to the default `"OK"`.
- **Static previews:** add `data-still` on `.penna-dialog-layer` in HTML when showing a non-dismissible diagram.

## API

| Rust (`penna::blocks::dialog`) | HTML (`uses=dialog`) | Role |
| --- | --- | --- |
| `WindowExt::open_dialog(cx, f)` | `.penna-dialog-stage > .penna-dialog-layer` | Modal stack host and scrim |
| `Dialog::new(cx)` | `.penna-dialog[data-penna="dialog"][data-ground]` | Floating glass dialog surface |
| `.title(...)` / `.footer(...)` | `.penna-dialog-title`, `.penna-dialog-footer` | Header title and trailing actions |
| `DialogButtonProps` | `[data-dialog-ok]`, `[data-dialog-cancel]` | Confirm and cancel action buttons |
| `.close_button(bool)` | `.penna-dialog-close[data-dialog-close]` | Top-right dismiss icon button |
| `.overlay_closable(false)` | `.penna-dialog-layer[data-static]` | Ignore backdrop clicks |
| `AlertDialog::new(cx)` | `role="alertdialog"` + `.penna-dialog-head` | Non-scrim-closable confirmation |
| `window.close_dialog(cx)` | `window.pennaDialog.close(layer)` | Close top modal programmatically |

# Empty state

On a phone, an empty screen says what is missing or under way; its main action sits in the [action bar](action-bar.html) at the bottom. For desktop chat empty and welcome views, use `chat::welcome_state` and `chat::empty_state` on [Chat](chat.html).

## Anatomy

```figure uses=touch,button
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Empty screen</span>
      <span class="docs-lk-mono">32 px icon · title</span>
    </div>
    <div class="docs-phone" style="display: flex; flex-direction: column; height: 420px">
      <div style="height: 88px"></div>
      <div class="penna-empty"><i data-icon="laptop"></i><b>No machines yet</b></div>
      <div style="flex: 1"></div>
      <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Add machine</span></button></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Connecting</span>
      <span class="docs-lk-mono">32 px spinner · state line</span>
    </div>
    <div class="docs-phone" style="display: flex; flex-direction: column; height: 420px">
      <div style="height: 88px"></div>
      <div class="penna-empty"><i class="penna-touch-spinner" data-icon="loader"></i><b>Connecting</b><span>Waiting for ssh on fabrico@xps</span></div>
      <div style="flex: 1"></div>
      <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Cancel</span></button></div>
    </div>
  </div>
</div>
```

1. **Icon or spinner**: 32 px muted icon (`EMPTY_GLYPH`) when empty, or a 32 px spinner when connecting.
2. **Title**: `title`, naming what is missing or in progress.
3. **Line**: optional `touch-small` muted line for live state ("Waiting for ssh on fabrico@xps"), never instructions on how to use the screen.
4. **Action**: the screen's main action sits in the [action bar](action-bar.html) at the bottom, never as a button directly under the text.

```rust
use penna::blocks::touch::{action_bar, PhoneEmpty, ACTION_BAR_BOTTOM};

v_flex()
    .size_full()
    .child(div().h(px(96.0)))
    .child(PhoneEmpty::new(IconName::Laptop, "No machines yet").on(ground))
    .child(div().flex_1())
    .child(
        action_bar(px(ACTION_BAR_BOTTOM)).child(
            Button::new("add")
                .primary()
                .with_size(Size::TouchLarge)
                .w_full()
                .label("Add machine"),
        ),
    )

PhoneEmpty::spinner("Connecting")
    .line("Waiting for ssh on fabrico@xps")
    .on(ground)
```

## Copy and placement

Keep the empty block focused on status, and keep the action down where the thumb reaches.

```figure uses=touch,button,panel
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground style="flex-direction: column; justify-content: space-between; min-height: 220px; padding: 48px 16px 16px"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-empty" style="padding: 0 8px"><i data-icon="laptop"></i><b>No machines yet</b></div><div class="penna-action-bar" style="width: 100%; padding: 0"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Add machine</span></button></div></div><figcaption>State title above, action at the bottom</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground style="flex-direction: column; justify-content: center; gap: 16px; min-height: 220px; padding: 48px 16px 24px"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-empty" style="padding: 0 8px"><i data-icon="laptop"></i><b>No machines yet</b><span>Tap the button below to pair a computer.</span></div><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch" type="button"><span class="penna-label" data-penna-text>Add machine</span></button></div><figcaption>Don't explain the UI or place the button under the text</figcaption></figure>
</div>
```

## API (`penna::blocks::touch`)

| Rust | HTML | Does |
| --- | --- | --- |
| `PhoneEmpty::new(icon, title)` | `.penna-empty` with `<i data-icon>` and `<b>` | Centered phone empty state with a 32 px icon |
| `PhoneEmpty::spinner(title)` | `.penna-empty` with `<i class="penna-touch-spinner" data-icon="loader">` | Centered phone connecting state with a 32 px spinner |
| `.line(text)` | `<span>` under `<b>` | Optional muted `touch-small` state line |
| `.on(ground)` | `data-ground` on parent | Paint inks for the ground under it |
| `EMPTY_GLYPH` | `--penna-icon-size: 32px` | Leading glyph and spinner size (32 px) |

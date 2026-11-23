# Action bar

Holds a phone screen's one main action at the bottom, where the thumb is. For moving around between screens, use the floating bar and strip.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail"><code>action_bar(bottom)</code> stacks actions in one column natively. The side-by-side row (<code>data-row</code>) and floating surface (<code>[data-penna="floating"]</code>) are drawn in CSS, not built natively yet.</span>
    </div>
  </div>
</div>
```

## Layouts

```figure uses=touch,button,panel
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Single main action</span>
      <span class="docs-lk-mono">56 px · 20 px bottom</span>
    </div>
    <div data-penna="card" data-ground style="overflow: hidden; padding-top: var(--penna-space-12)">
      <div class="penna-action-bar">
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Secondary above</span>
      <span class="docs-lk-mono">Quiet over primary</span>
    </div>
    <div data-penna="card" data-ground style="overflow: hidden; padding-top: var(--penna-space-12)">
      <div class="penna-action-bar">
        <button class="penna-button" data-penna="button.quiet" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Scan QR instead</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Side by side</span>
      <span class="docs-lk-mono">CSS only · main last</span>
    </div>
    <div data-penna="card" data-ground style="overflow: hidden; padding-top: var(--penna-space-12)">
      <div class="penna-action-bar" data-row>
        <button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Not now</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Add key</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Floating surface</span>
      <span class="docs-lk-mono">CSS only · over scroll</span>
    </div>
    <div data-penna="card" data-ground style="overflow: hidden; padding-top: var(--penna-space-16)">
      <div class="penna-action-bar" data-penna="floating">
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Continue</span></button>
      </div>
    </div>
  </div>
</div>
```

- **Main action**: `touch-large` 56, full width. Add at most one secondary action, stacked above or beside it with the main action last.
- **Insets**: 16 from the sides and `ACTION_BAR_BOTTOM` (20) above the bottom edge at rest. When the keyboard is up, pass `ACTION_BAR_KEYBOARD` (8, `data-keys` in HTML).
- **One bar per screen**: a screen has either the floating navigation strip or an action bar, never both.
- **Creation actions**: inside a machine, creation actions (New chat, New terminal, New browser) live in the plus disc's stack menu, not an action bar.

```rust
use penna::blocks::touch::{action_bar, ACTION_BAR_BOTTOM, ACTION_BAR_KEYBOARD};

let bottom = if keyboard_open {
    px(ACTION_BAR_KEYBOARD)
} else {
    px(ACTION_BAR_BOTTOM)
};

action_bar(bottom)
    .child(
        Button::new("scan")
            .quiet()
            .with_size(Size::TouchLarge)
            .w_full()
            .label("Scan QR instead"),
    )
    .child(
        Button::new("connect")
            .primary()
            .with_size(Size::TouchLarge)
            .w_full()
            .label("Connect"),
    )
```

## Primary emphasis

One primary button per bar so the thumb has one obvious target.

```figure uses=touch,button,panel
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground style="padding: 40px 16px 24px"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-action-bar" data-row style="width: 100%; padding: 0"><button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Not now</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Add key</span></button></div></div><figcaption>One primary action, trailing</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground style="padding: 40px 16px 24px"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-action-bar" data-row style="width: 100%; padding: 0"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Not now</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Add key</span></button></div></div><figcaption>Don't make both actions primary</figcaption></figure>
</div>
```

## API (`penna::blocks::touch`)

| Rust | HTML | Does |
| --- | --- | --- |
| `action_bar(bottom)` | `.penna-action-bar` | Bottom `v_flex` container (`px(16)`, `gap(8)`, `pb(bottom)`) |
| `ACTION_BAR_BOTTOM` | default (`20px`) | Bottom inset at rest (20 px) |
| `ACTION_BAR_KEYBOARD` | `data-keys` (`8px`) | Bottom inset above the keyboard (8 px) |
| `.child(Button::new(..).with_size(Size::TouchLarge).w_full())` | `> .penna-button[data-size="touch-large"]` | Full-width 56 px action inside the bar |
| none | `data-row` | Equal-width side-by-side row (CSS only) |
| none | `[data-penna="floating"]` | Floating container surface over scrolled content (CSS only) |

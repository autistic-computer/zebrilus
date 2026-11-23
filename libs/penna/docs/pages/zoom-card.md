# Zoom card

What a [tile](tile.html) becomes when it grows to fill the screen: the machine you went into, or the form behind the add tile. For a picker or short ask, use a [sheet](sheet.html).

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail"><code>ZoomCard</code> layout and ground are built natively. Drag-down and pinch-out dismiss, the in-card QR scanner accessory, and automatic zoom-out on disconnect are not built natively yet (the system camera opens <code>zz://</code> links instead).</span>
    </div>
  </div>
</div>
```

## Anatomy

```figure uses=compact,touch,button,input,field,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: min(100%, 370px); height: 656px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div>
      <div class="penna-touch-header"><span data-penna-text>Found on your tailnet</span><span class="penna-touch-count" data-penna-text>3</span></div>
      <div class="penna-touch-group" data-penna="settings.card" data-ground>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" tabindex="0"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span data-penna-text>macbook</span><span data-penna-text>macOS · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" tabindex="0"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>alienware</span><span data-penna-text>Linux · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span data-penna-text>homelab</span><span data-penna-text>No zz on it yet</span></span></div>
      </div>
    </div>
    <div class="penna-field">
      <label class="penna-field-label" for="zoom-card-address" data-penna-text>Or an address</label>
      <div class="penna-input" data-penna="field" data-size="touch-large" style="padding-right: 4px">
        <input id="zoom-card-address" placeholder="user@host" />
        <span class="penna-input-suffix"><button class="penna-button" data-penna="phone.button" data-size="touch" data-shape="icon" type="button" aria-label="Scan QR code"><i data-icon="qr-code"></i></button></span>
      </div>
    </div>
  </div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button" disabled aria-disabled="true"><span class="penna-label" data-penna-text>Connect</span></button></div>
</div>
```

1. **Top row**: 44 tall, holding the trailing close control.
2. **Large title**: 52 tall, `touch-title`.
3. **Body**: `inset-pane` 16 at the sides, groups `gap-group` 16 apart.
4. **Action bar**: optional bottom actions passed via `.action(...)`, rendered in [`action_bar`](action-bar.html).

While smaller than the screen, the card uses the phone container corner (`--penna-phone-radius`, twice `radius`, matching the tile); once full-screen, `.rounded(px(0.0))` takes the screen's own corners.

```rust
use penna::blocks::compact::{zoom_card_ground, ZoomCard};

let ground = zoom_card_ground(cx);

ZoomCard::new("card-add", "Add machine")
    .top_inset(safe_top)
    .rounded(card_radius)
    .on_close(on_close)
    .child(tailnet_group)
    .child(address_field)
    .action(
        Button::new("connect")
            .primary()
            .with_size(Size::TouchLarge)
            .w_full()
            .disabled(address.is_empty())
            .label("Connect"),
    )
```

## Connecting and scan states

```figure uses=compact,touch,button,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Connecting card</span>
      <span class="docs-lk-mono">Built natively</span>
    </div>
    <div class="penna-zoom-card" data-penna="phone.card" style="width: min(100%, 370px); height: 500px; padding-top: var(--penna-inset-pane)">
      <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Close"><i data-icon="xmark"></i></button></div>
      <div class="penna-zoom-title">xps</div>
      <div class="penna-zoom-body" style="padding-top: 80px"><div class="penna-empty"><i class="penna-touch-spinner" data-icon="loader"></i><b>Connecting</b><span>Waiting for ssh on fabrico@xps</span></div></div>
      <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Cancel</span></button></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">QR scan view</span>
      <span class="docs-lk-mono">Spec only</span>
    </div>
    <div class="penna-zoom-card" data-penna="phone.card" style="width: min(100%, 370px); height: 500px; padding-top: var(--penna-inset-pane)">
      <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Close"><i data-icon="xmark"></i></button></div>
      <div class="penna-zoom-title">Add machine</div>
      <div class="penna-zoom-body">
        <div style="aspect-ratio: 1; max-height: 280px; display: grid; place-items: center; border-radius: calc(var(--penna-phone-radius) * var(--penna-corner-grow)); corner-shape: superellipse(var(--penna-window-k)); background: var(--penna-bg); box-shadow: inset 0 0 0 1px var(--penna-divider); color: var(--penna-fg-faint); --penna-icon-size: 32px"><i data-icon="qr-code"></i></div>
      </div>
      <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Type an address instead</span></button></div>
    </div>
  </div>
</div>
```

- **Add card**: lists discovered tailnet machines first (a tap adds and connects; machines without zz stay disabled with the reason), followed by the address field (`Or an address`) and a `Connect` button disabled until text is entered. Without tailnet machines, the address field comes first and focuses immediately.
- **Connecting card**: opens behind a connecting tile using [`PhoneEmpty::spinner`](empty-state.html) positioned in the upper middle. `Cancel` is drawn as a secondary action and zooms back to the offline tile; once connected, the card becomes the machine view.
- **Card of tiles**: a zoomed machine can itself show window tiles, replacing the action bar with the floating navigation strip and plus disc. Each window card zooms one level further.
- **Motion (spec)**: the tile grows to fill the screen while the grid behind scales up, cross-fading between 30% and 70% on the touch spring.

## API (`penna::blocks::compact`)

| Rust | HTML | Does |
| --- | --- | --- |
| `ZoomCard::new(id, title)` | `.penna-zoom-card[data-penna="phone.card"]` | Full-screen zoom card with 44 px top row, 52 px title, and body |
| `.on_close(handler)` | `.penna-zoom-top` close `.penna-touch-icon` | Trailing 44 px close button handler |
| `.action(el)` | `.penna-action-bar` | Repeatable bottom action inside the card's action bar |
| `.top_inset(px)`, `.bottom_inset(px)` | `padding-top`, `.penna-action-bar` padding | Top safe-area inset and bottom inset (default `ACTION_BAR_BOTTOM` 20 px) |
| `.rounded(px)` | `--penna-r` | Corner radius while mid-zoom (default 0 px when full-screen) |
| `.child(el)`, `.children(els)` | `.penna-zoom-body` | Body sections (`ParentElement`, 16 px padding and group gap) |
| `zoom_card_ground(cx)` | none | Ground for rows and controls inside a zoom card |

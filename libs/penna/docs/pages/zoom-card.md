# Zoom card

What a tile becomes when it grows to fill the screen: the machine you went into, or the form behind the add tile. For a short question, a sheet.

## Anatomy

```figure uses=compact,touch,button,input,list
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; max-width: 100%; height: 656px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div>
      <div class="penna-touch-header"><span>Found on your tailnet</span><span class="penna-touch-count">3</span></div>
      <div class="penna-touch-group" data-penna="settings.card" data-ground>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>macOS · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>Linux · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
      </div>
    </div>
    <label class="penna-input" data-penna="field" data-size="touch-large"><input placeholder="user@host" aria-label="Address" /><button class="penna-touch-icon" data-penna="phone.button" style="margin-right: -6px" aria-label="Scan QR code"><i data-icon="qr-code"></i></button></label>
  </div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" disabled aria-disabled="true"><span class="penna-label" data-penna-text>Connect</span></button></div>
</div>
```

1. **Top row**: 44, the close control at its end.
2. **Large title**: 52, `touch-title`.
3. **Body**: `inset-pane` 16, groups `gap-group` 16 apart.
4. **Action bar**: the one main action.

While it is smaller than the screen the card has the phone container corner, twice `radius`, the tile's; once it fills the screen it takes the screen's.

## Add card

Behind the add tile. Found machines first, as touch rows with the add mark: a tap adds the machine and connects it, and a machine without zz is a disabled row that says so. Then the address field with its scan accessory; Connect stays off until an address is typed. With no tailnet the field comes first, focused, with the keyboard up.

## Scan

```figure uses=compact,touch,button,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 660px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div style="aspect-ratio: 1; border-radius: calc(var(--penna-phone-radius) * var(--penna-corner-grow)); corner-shape: superellipse(var(--penna-window-k)); background: color-mix(in srgb, var(--penna-fg) 80%, var(--penna-bg))"></div>
  </div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Type an address instead</span></button></div>
</div>
```

The scan accessory turns the same card into the scanner: the camera's picture, square, and a way back to typing. The picture is the app's.

## Connecting card

```figure uses=compact,touch,button,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 520px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">xps</div>
  <div class="penna-zoom-body" style="justify-content: center"><div class="penna-empty"><i class="penna-touch-spinner" data-icon="loader"></i><b>Connecting</b><span>Waiting for ssh on fabrico@xps</span></div></div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Cancel</span></button></div>
</div>
```

Behind a connecting tile. Cancel is its one action, drawn as a secondary since stopping is not what the card is for; it zooms back to the tile, now not connected. When the connection lands, the card becomes the machine.

## A card of tiles

What you went into can be a screen of window cards in turn, a machine holding its sessions: then the floating strip and plus disc sit at the bottom in place of the action bar, and each card zooms one level further. Its windows and sessions are made from the plus disc's menu, not from a card. When what the card shows goes away, as when a machine disconnects, it zooms back out to its tile, which says why.

## Motion

It opens with the zoom: the tile's layer grows to the screen while the grid behind scales up around it, and the two cross-fade between 30% and 70% of the way, on the touch spring. It closes the same way back, one level at a time, from the close control, a drag down or a pinch out, or by dragging the bar's handle up.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `ZoomCard::new(id, title, on_close)` | `.penna-zoom-card[data-penna="phone.card"]` | Full-screen zoom card with top row, title and body |
| `.action_bar(bar)`, `.top(el)` | `.penna-action-bar`, `.penna-zoom-top` | Bottom action bar and optional leading top control |
| `zoom_card_ground(cx)` | none | Ground for rows and cards inside a zoom card |

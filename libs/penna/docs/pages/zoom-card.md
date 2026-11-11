# Zoom card

What a tile becomes when it grows to fill the screen: the machine you went into, or the form behind the add tile. For a short question, a sheet.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Zoom built, card not yet</span><span class="penna-text penna-notice-detail" data-penna-text>The zoom between tile and screen is native; the card's close control, drag down and pinch out, the add card's found rows and scanner, and the connecting card are drawn here with the HTML kit.</span></div></div></div>
```

## Anatomy

```figure uses=compact,touch,button,input,list
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 656px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div>
      <div class="penna-touch-header"><span>Found on your tailnet</span><span class="penna-touch-count">3</span></div>
      <div class="penna-touch-group" data-penna="settings.card">
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>macOS · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>Linux · runs zz</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" data-state="disabled" aria-disabled="true"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span><span>No zz on it yet</span></span></div>
      </div>
    </div>
    <label class="penna-input" data-penna="field" data-size="touch-large"><input placeholder="user@host" /></label>
  </div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" disabled aria-disabled="true"><span class="penna-label" data-penna-text>Connect</span></button></div>
</div>
```

1. **Top row**: 44, the close control at its end.
2. **Large title**: 52, `touch-title`.
3. **Body**: `inset-pane` 16, groups `gap-group` 16 apart.
4. **Action bar**: the one main action.

While it is smaller than the screen the card has the phone container corner, twice `radius`, the tile's; once it fills the screen it takes the screen's. Natively the zoom starts it at 10.

## Add card

Behind the add tile. Found machines first, as touch rows with the add mark: a tap adds the machine and connects it, and a machine without zz is a disabled row that says so. Then the address field with its scan accessory; Connect stays off until an address is typed. With no tailnet the field comes first, focused, with the keyboard up.

## Scan

```figure uses=compact,touch,button,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 660px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div style="aspect-ratio: 1; border-radius: calc(var(--penna-phone-radius) * var(--penna-corner-grow)); corner-shape: superellipse(var(--penna-window-k)); background: color-mix(in srgb, var(--penna-fg) 80%, var(--penna-bg))"></div>
    <span class="penna-type" data-type="touch-small" data-muted data-penna-text>On your computer, the code is in zz Settings, Hosts, under Add this machine to a phone.</span>
  </div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Type an address instead</span></button></div>
</div>
```

The scan accessory turns the same card into the scanner: the camera's picture, square, one line saying where the code is, and a way back to typing. The picture is the app's.

## New card

```figure uses=compact,touch,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 420px; padding-top: var(--penna-inset-pane)">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">New</div>
  <div class="penna-zoom-body">
    <div>
      <div class="penna-touch-header"><span>In session zz</span></div>
      <div class="penna-touch-group" data-penna="settings.card">
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="chat"></i><span class="penna-touch-row-lines"><span>New chat</span></span><i data-icon="chevron-right"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="square-terminal"></i><span class="penna-touch-row-lines"><span>New terminal</span></span></div>
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="globe"></i><span class="penna-touch-row-lines"><span>New browser</span></span></div>
      </div>
    </div>
  </div>
</div>
```

Behind the add tile at the end of a group of things you make rather than find. A header says where the new thing goes, then one row per kind; the whole row is the target and picking it is the action, so there is no action bar. A row that leads to another choice ends in a chevron; one that makes the thing at once has no mark, and the card zooms on into what it made. The kinds are the app's.

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

What you went into can be a screen of tiles in turn, a machine holding its windows: then the pill sits at the bottom in place of the action bar, and its tiles zoom one level further. When what the card shows goes away, as when a machine disconnects, it zooms back out to its tile, which says why.

## Motion

It opens with the zoom: the tile's layer grows to the screen while the grid behind scales up around it, and the two cross-fade between 30% and 70% of the way, on the touch spring. It closes the same way back, one level at a time, from the close control, a drag down or a pinch out, or on a card of tiles by dragging the pill up.

# Zoom card

What a tile becomes when it grows to fill the screen: the machine you went into, or the form behind the add tile. For a short question, a sheet.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Zoom built, card not yet</span><span class="penna-text penna-notice-detail" data-penna-text>The zoom between tile and screen is native; the card's close control, drag down and pinch out are drawn here with the HTML kit.</span></div></div></div>
```

## Anatomy

```figure uses=compact,touch,button,input,list
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 656px; padding-top: var(--penna-inset-pane); border-radius: 28px">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Add machine</div>
  <div class="penna-zoom-body">
    <div>
      <div class="penna-touch-header"><span>Found on your tailnet</span><span class="penna-touch-count">3</span></div>
      <div class="penna-touch-group" data-penna="settings.card">
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span><span>macOS · runs zz</span></span><i data-icon="plus" data-add></i></div>
        <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span><span>Linux · runs zz</span></span><i data-icon="plus" data-add></i></div>
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

## Motion

It opens with the zoom: the tile's layer grows to the screen while the grid behind scales up around it, and the two cross-fade between 30% and 70% of the way, on the touch spring. It closes the same way back, from the close control, a drag down or a pinch out.

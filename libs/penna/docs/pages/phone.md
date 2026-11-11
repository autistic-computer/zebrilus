# Phone

A phone screen is built from phone components, sized for a thumb from the start. It is not the desktop made bigger: desktop components under a coarse pointer keep their layout and only grow their targets.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Not built natively yet</span><span class="penna-text penna-notice-detail" data-penna-text>The phone components are drawn here with the HTML kit. Natively there are the 44 touch rung, the touch text styles, sheets, the pill and the bento overview; the 56 rung and the components below are next.</span></div></div></div>
```

## Rules

1. **The whole thing is the target.** A tile, row or card is tapped as one piece; no button sits inside one. More actions live one zoom in, or behind press-and-hold.
2. **Nothing tappable is under 44**, and what is drawn is as big as what is tappable: no small drawing with an invisible hit area.
3. **One main action per screen**, full width, at the bottom where the thumb is. It rides the keyboard. The top of the screen is for reading.
4. **Going into a thing is a zoom**; a short question that returns you where you were is a sheet.
5. **State is said in words and a tone dot**, never by look alone.
6. **Text is touch-body 17** by default, touch-small 15 for details, never under 13.

```figure uses=compact,touch,button,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-machine-grid" style="width: 181px; grid-template-columns: 1fr"><div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-state">Not connected</span><span class="penna-machine-foot"><span>Last seen 2 days ago</span></span><span class="penna-press"></span></div></div></div><figcaption>The tile is the target</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-machine-grid" style="width: 181px; grid-template-columns: 1fr"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-state">Not connected</span><span class="penna-machine-foot"><button class="penna-button" data-penna="button" data-size="small" style="align-self: flex-start"><span class="penna-label" data-penna-text>Connect</span></button></span></div></div></div><figcaption>Don't put a small button in it</figcaption></figure>
</div>
```

## Two touch rungs

```figure
<div class="docs-tokens">
  <span class="docs-head">Rung</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">For</span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span>the smallest target, icon buttons, accessories</span>
  <span>touch-large</span><span><code>56</code></span><span><code>touch-body</code></span><span>main actions and fields</span>
</div>
```

```figure uses=button,compact
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button><span class="docs-note">touch, icon button</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-size="touch"><span class="penna-label" data-penna-text>Not now</span></button><span class="docs-note">touch</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" style="width: 220px"><span class="penna-label" data-penna-text>Connect</span></button><span class="docs-note">touch-large</span></div>
</div>
```

`touch-large` is as tall as the pill, so a main action lines up with the pill it takes the place of, and a 44 accessory fits inside a 56 field with the 4 inside gap. Under a coarse pointer on a tablet or a touch screen, desktop components take `touch` and the touch scale instead; that is desktop layout made reachable, not a phone screen.

## Screen

```figure uses=compact,touch,button,label
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 520px; padding-top: var(--penna-inset-pane); border-radius: 28px">
  <div class="penna-zoom-top"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Close"><i data-icon="xmark"></i></button></div>
  <div class="penna-zoom-title">Title</div>
  <div class="penna-zoom-body"><span class="penna-type" data-type="touch-small" data-muted data-penna-text>Body: inset-pane 16, groups gap-group 16 apart.</span></div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Main action</span></button></div>
</div>
```

1. **Top row**: 44, the close or back control at its end.
2. **Large title**: 52, `touch-title`, folding into the top row as the page scrolls.
3. **Body**: `inset-pane` 16 at the sides, groups `gap-group` 16 apart.
4. **Bottom**: the pill on the overview and on a pane, or the action bar on a zoom card or sheet. Same spot, so never both.

## Surfaces

Flat by default; glass only for what sits over something. Tiles, rows, fields and the main action are flat fills. The pill, its menu, floating buttons, sheets, the key row's pill and the chat composer float: glass when glass is on, and with glass off the popover fill, a hairline all round and a close shadow.

```figure uses=compact,touch,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="glass-liquid"><div style="position: relative; width: 280px; height: 320px; overflow: hidden; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)"><div style="display: flex; flex-direction: column; gap: 8px; padding: 16px"><div class="penna-machine-grid"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>macbook</span></span><span class="penna-machine-foot"><span>3 chats</span></span></div><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-foot"><span>Offline</span></span></div></div><div class="penna-touch-group" data-penna="settings.card"><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus" data-add></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus" data-add></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>thinkpad</span></span><i data-icon="plus" data-add></i></div></div></div><span class="docs-glass-bars" style="position: absolute; left: 40px; bottom: 32px; margin: 0"><i></i><i></i><i></i></span><div class="penna-compact-bar" style="position: absolute; right: 0; bottom: 0; left: 0; height: auto; padding: 0 16px 16px"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="server"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></button></div></div><span class="docs-note"><code>liquid</code></span></div>
  <div class="docs-cell" data-look="glass-none"><div style="position: relative; width: 280px; height: 320px; overflow: hidden; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)"><div style="display: flex; flex-direction: column; gap: 8px; padding: 16px"><div class="penna-machine-grid"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>macbook</span></span><span class="penna-machine-foot"><span>3 chats</span></span></div><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-foot"><span>Offline</span></span></div></div><div class="penna-touch-group" data-penna="settings.card"><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus" data-add></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus" data-add></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>thinkpad</span></span><i data-icon="plus" data-add></i></div></div></div><span class="docs-glass-bars" style="position: absolute; left: 40px; bottom: 32px; margin: 0"><i></i><i></i><i></i></span><div class="penna-compact-bar" style="position: absolute; right: 0; bottom: 0; left: 0; height: auto; padding: 0 16px 16px"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="server"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></button></div></div><span class="docs-note"><code>none</code></span></div>
</div>
```

- **Flat**: the screen, tiles, rows and row groups, fields, the empty state, the zoom card, the key row's keys.
- **Solid**: the main action, never glass.
- **Floating**: the pill, its menu, the gear disc, sheets, the key row's pill, the chat composer, and the action bar when content scrolls under it.

Not built natively yet: the pill menu's chips and the key row's pill draw a flat tint today, glass on or off.

## Status marks

```figure uses=touch,compact,badge,label
<div class="docs-show" data-gap="wide">
  <span class="docs-show"><span class="penna-tone-dot"></span><span class="penna-type" data-type="touch-small" data-penna-text>Connected</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="warning"></span><span class="penna-type" data-type="touch-small" data-penna-text>1 waiting</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="off"></span><span class="penna-type" data-type="touch-small" data-penna-text>Not connected</span></span>
  <span class="docs-show"><i class="penna-touch-spinner" data-icon="loader"></i><span class="penna-type" data-type="touch-small" data-penna-text>Connecting</span></span>
  <span class="docs-show"><span class="penna-touch-badge" data-penna="badge">3</span><span class="penna-type" data-type="touch-small" data-penna-text>chats</span></span>
  <span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button></span>
</div>
```

- **Tone dot**: 8, always beside its word.
- **Spinner**: 16, one turn every 900ms.
- **Count badge**: 20, the `touch-small` line.
- **Icon button**: 44 with a 20 icon; over the content, in a 56 disc.

## The pieces

[Button](button.html), [Field](field.html), [List](list.html) for touch rows, [Tile](tile.html) for the machine tile, and the [action bar](action-bar.html), [zoom card](zoom-card.html), [sheet](sheet.html) and [empty state](empty-state.html) blocks.

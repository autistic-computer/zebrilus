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

**Press and hold** a tile, or the header over a group of tiles, for a sheet of what you can do to it: touch rows, the one that closes or removes last in danger ink. One rule for every tile, so nothing that acts on a tile sits inside it. A header that can be held is 44 tall. Not built natively yet.

```figure uses=sheet,touch,label
<div style="position: relative; width: 370px; height: 330px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
  <div class="penna-sheet" style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-title" data-penna-text>Port the bento grid</span><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button></div>
        <div class="penna-sheet-list">
          <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename</span></span></div>
          <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Close window</span></span></div>
        </div>
      </div>
    </div>
  </div>
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
<div class="penna-zoom-card" data-penna="phone.card" style="width: 370px; height: 520px; padding-top: var(--penna-inset-pane)">
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

A screen of things is the same at every level: the large title, tiles two to a row under a section header per group, the add tile last in each grid. Going into a tile is a zoom, and what opens may be another screen of tiles; the close control, dragging the pill up or a pinch goes back one level.

## Surfaces

Flat by default; glass only for what sits over something. Tiles, rows, fields and the main action are flat fills. The pill, its menu, floating buttons, sheets, the key row's pill and the chat composer float: glass when glass is on, and with glass off the popover fill, a hairline all round and a close shadow.

```figure uses=compact,touch,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" data-look="glass-liquid"><div style="position: relative; width: 280px; height: 320px; overflow: hidden; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)"><div style="display: flex; flex-direction: column; gap: 8px; padding: 16px"><div class="penna-machine-grid"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>macbook</span></span><span class="penna-machine-foot"><span>3 chats</span></span></div><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-foot"><span>Offline</span></span></div></div><div class="penna-touch-group" data-penna="settings.card"><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus"></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>thinkpad</span></span><i data-icon="plus"></i></div></div></div><span class="docs-glass-bars" style="position: absolute; left: 40px; bottom: 32px; margin: 0"><i></i><i></i><i></i></span><div class="penna-compact-bar" style="position: absolute; right: 0; bottom: 0; left: 0; height: auto; padding: 0 16px 16px"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="server"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></button></div></div><span class="docs-note"><code>liquid</code></span></div>
  <div class="docs-cell" data-look="glass-none"><div style="position: relative; width: 280px; height: 320px; overflow: hidden; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)"><div style="display: flex; flex-direction: column; gap: 8px; padding: 16px"><div class="penna-machine-grid"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>macbook</span></span><span class="penna-machine-foot"><span>3 chats</span></span></div><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-foot"><span>Offline</span></span></div></div><div class="penna-touch-group" data-penna="settings.card"><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus"></i></div><div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>thinkpad</span></span><i data-icon="plus"></i></div></div></div><span class="docs-glass-bars" style="position: absolute; left: 40px; bottom: 32px; margin: 0"><i></i><i></i><i></i></span><div class="penna-compact-bar" style="position: absolute; right: 0; bottom: 0; left: 0; height: auto; padding: 0 16px 16px"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="server"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></button></div></div><span class="docs-note"><code>none</code></span></div>
</div>
```

- **Flat**: the screen, tiles, rows and row groups, fields, the empty state, the zoom card, the key row's keys.
- **Solid**: the main action, never glass.
- **Floating**: the pill, its menu, the gear disc, sheets, the key row's pill, the chat composer, and the action bar when content scrolls under it.

Not built natively yet: the pill menu's chips and the key row's pill draw a flat tint today, glass on or off.

## Corners

The desktop's four corner rules, unchanged. Controls round from their height, the floating disc included. Phone containers share one corner, twice `radius`, since the phone's 56 rung is twice the desktop's 28. Rows inside a group are concentric with it.

```figure uses=compact,touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button><span class="docs-note">Icon button, 44</span></div>
  <div class="docs-cell"><span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button></span><span class="docs-note">Disc, 56</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" style="width: 160px"><span class="penna-label" data-penna-text>Connect</span></button><span class="docs-note">Main action, 56</span></div>
  <div class="docs-cell" style="width: 300px"><div class="penna-touch-group" data-penna="settings.card" style="width: 100%"><div class="penna-touch-row" data-penna="phone.row" role="button" data-state="hover"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span></span><i data-icon="plus"></i></div><div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div></div><span class="docs-note">Group, rows inside it</span></div>
  <div class="docs-cell"><div class="penna-machine-grid" style="width: 160px; grid-template-columns: 1fr"><div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>xps</span></span><span class="penna-machine-state">Connected</span><span class="penna-press"></span></div></div><span class="docs-note">Machine tile</span></div>
</div>
```

| Piece | Kind | Default | macos | Yaru |
| --- | --- | --- | --- | --- |
| Main action, field, pill, disc (56) | control | 17.5 | a capsule | 14.3 |
| Icon button (44) | control | 13.7 | a circle | 11.2 |
| Icon button in the disc | the disc's less 6 | 11.5 | a circle | 8.3 |
| Touch row, 52 and 64 | control, never a capsule | 16.2, 20 | 16.2, 20 | 13.2, 16.3 |
| Touch row in a group | at most the group's less 4 | 16.2, 20 | 16.2, 20 | 12, 12 |
| Tile, row group, sheet, zoom card | phone container, twice `radius` | 24 | 24 | 16 |

Not built natively yet: tiles take the control radius (12, 25, 8), the sheet keeps `radius`, the zoom starts at 10, and the disc follows the pill's corner, a circle in every look.

## Status marks

```figure uses=touch,compact,badge,label
<div class="docs-show" data-gap="wide">
  <span class="docs-show"><span class="penna-tone-dot"></span><span class="penna-type" data-type="touch-small" data-penna-text>Connected</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="warning"></span><span class="penna-type" data-type="touch-small" data-penna-text style="color: var(--penna-warning-ink)">1 waiting</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="off"></span><span class="penna-type" data-type="touch-small" data-penna-text>Not connected</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="danger"></span><span class="penna-type" data-type="touch-small" data-penna-text style="color: var(--penna-danger-ink)">Failed</span></span>
  <span class="docs-show"><span class="penna-tone-dot" data-tone="strong"></span><span class="penna-type" data-type="touch-small" data-penna-text>Done</span></span>
  <span class="docs-show"><i class="penna-touch-spinner" data-icon="loader"></i><span class="penna-type" data-type="touch-small" data-penna-text>Connecting</span></span>
  <span class="docs-show"><span class="penna-touch-badge" data-penna="badge">3</span><span class="penna-type" data-type="touch-small" data-penna-text>chats</span></span>
  <span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" aria-label="Settings"><i data-icon="settings"></i></button></span>
</div>
```

- **Tone dot**: 8, always beside its word, which takes its tone's ink: green connected, amber waiting for you, red failed, the ink for done and not yet seen, faint off. Idle has no mark. No state takes the accent: it is a person's pick, and Yaru's orange would read as waiting.
- **Spinner**: 16, one turn every 900ms, for working and connecting. Working is never a green dot: green only means connected.
- **Count badge**: 20, the `touch-small` line.
- **Icon button**: 44 with a 20 icon; over the content, in a 56 disc.

## The pieces

[Button](button.html), [Field](field.html), [List](list.html) for touch rows, [Tile](tile.html) for the phone tile, machine and window, and the [action bar](action-bar.html), [zoom card](zoom-card.html), [sheet](sheet.html) and [empty state](empty-state.html) blocks.

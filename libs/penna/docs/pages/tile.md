# Tile

A thumbnail you pick or drag, drawn to scale. For a box of content use a card, for a list a row.

## Variants

```figure uses=well,tile,label
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div></div><span class="docs-note">Screen, on a well</span></div>
  <div class="docs-cell"><div class="penna-tile" data-penna="tile.card" data-variant="card"><span class="penna-type" data-type="body" data-strong data-truncate data-penna-text>xps</span><span class="penna-type" data-type="caption" data-truncate data-penna-text>Linux · 1 screen</span></div><span class="docs-note">Card, in a dock</span></div>
</div>
```

- **Screen**: a screen drawn to scale on a well.
- **Card**: something you can place, waiting in a dock.
- **Drop**: where a dragged thing will land.
- **Veil**: the tint over panes while something is dragged.
- **Mini**: a pane inside a window's tile. Takes the caller's fill.

```rust
Tile::new("dp-1")
    .on(Well::ground(cx))
    .variant(TileVariant::Screen)
    .state(PaintState::Selected)
```

## States

```figure uses=well,tile,label
<div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)">
  <div class="penna-well-row">
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen"><span class="penna-type" data-type="caption" data-penna-text>Rest</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="hover"><span class="penna-type" data-type="caption" data-penna-text>Dragged</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="selected"><span class="penna-type" data-type="caption" data-penna-text>Selected</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="focused"><span class="penna-type" data-type="caption" data-penna-text>Focused</span></div>
    <div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-state="disabled"><span class="penna-type" data-type="caption" data-penna-text>Away</span></div>
  </div>
</div>
```

- Selected: a 2px ring outside the edge.
- Focused: the focus ring. A tile with a click handler takes keyboard focus and runs on Space and Enter; a click never focuses it.
- Off: half the fill, faint label, no shadow.
- Dragged: the app sets hover, and the tile rises a step of fill and lifts on a close shadow, so it reads in dark too.

## Tone

A tone tints the label. Say what it means in words.

```figure uses=well,tile,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-tone="danger"><span class="penna-type" data-type="caption" data-penna-text>Can't go there</span></div></div></div></div><figcaption>Danger with a reason</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-well" data-penna="well" style="padding: var(--penna-space-16)"><div class="penna-well-row"><div class="penna-tile" data-penna="tile.screen" data-variant="screen" data-tone="danger"><span class="penna-type" data-type="caption" data-penna-text>DP-1</span></div></div></div></div><figcaption>Don't rely on the color</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Tile::new(id)` | `.penna-tile`, `data-penna="tile.card"`, `data-variant="card"` | Card variant |
| `.variant(TileVariant::Screen)` | `tile.screen`, `screen` | Also `Drop`, `Veil`, `Mini` |
| `.state(PaintState::Selected)` | `data-state="selected"` | Also `Hover`, `Focused`, `Disabled` |
| `.on_click(handler)` | `role="button"`, `tabindex="0"`, `click` | Takes focus; Space and Enter run it |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Tints the label |
| `.on(ground)` | inside `.penna-well` | Paints it on a ground |
| `.edge(width)` | `data-edge` | Drop zone edge width |

Size, inset, picking and dragging are yours, natively and in HTML.

## Phone tile

The cell of every screen of things on a phone, two to a row: machines on the overview, windows inside a machine. One anatomy, two kinds of content. The whole tile is the target, and holding it opens a sheet of what you can do to it.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>States and window content not built natively yet</span><span class="penna-text penna-notice-detail" data-penna-text>The grid, the cell and the add tile are native, but with the control radius as their corner; a window is a bento tile of mini panes. These states, window content, press and hold and the phone corner are drawn with the HTML kit.</span></div></div></div>
```

### Machine content

```figure uses=compact,touch
<div class="penna-machine-grid" style="width: 370px">
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>alienware</span></span><span class="penna-machine-state">Connected</span><span class="penna-machine-foot"><span>1 chat</span><span>cachyos-upgrade</span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="warning"></span><span>macbook</span></span><span class="penna-machine-state" data-tone="warning">1 waiting for you</span><span class="penna-machine-foot"><span>3 chats</span><span>2 working</span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><i class="penna-touch-spinner" data-icon="loader"></i><span>homelab</span></span><span class="penna-machine-state">Connecting</span><span class="penna-machine-foot"><span>Over Tailscale</span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-state">Not connected</span><span class="penna-machine-foot"><span>Last seen 2 days ago</span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button" data-state="active"><span class="penna-machine-name"><span class="penna-tone-dot"></span><span>alienware</span></span><span class="penna-machine-state">Connected</span><span class="penna-machine-foot"><span>1 chat</span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.add" data-add role="button"><i data-icon="plus"></i>Add machine<span class="penna-press"></span></div>
</div>
```

| State | Says | A tap |
| --- | --- | --- |
| Connected | a green dot, "Connected" | zooms into the machine |
| Waiting | a warning dot, "1 waiting for you" | zooms into the machine |
| Connecting | a spinner, "Connecting", how it is reached | zooms into the [connecting card](zoom-card.html), where Cancel is the one action |
| Offline | a faint dot, "Not connected", when it was last seen | reconnects |
| Pressed | the press over the tile | |
| Add | "Add machine" | zooms into the [add card](zoom-card.html) |

### Window content

```figure uses=compact,touch
<div class="penna-machine-grid" style="width: 370px">
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><span class="penna-tone-dot" data-tone="warning"></span><span>Make the pill menu offer New chat</span></span><span class="penna-machine-state" data-tone="warning"><span>Needs approval</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="chat"></i><i data-icon="square-terminal"></i></span><span class="penna-machine-where"><span>zebrilus · main</span><span class="penna-machine-age">now</span></span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><span class="penna-tone-dot" data-tone="warning"></span><span>Pick a tailnet library</span></span><span class="penna-machine-state" data-tone="warning"><span>Has a question</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="chat"></i></span><span class="penna-machine-where"><span>zebrilus · tailnet</span><span class="penna-machine-age">4m</span></span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><i class="penna-touch-spinner" data-icon="loader"></i><span>Fix pane stack hit testing on iPad</span></span><span class="penna-machine-state"><span>Running cargo test -p zz-mux</span><span>62%</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="chat"></i></span><span class="penna-machine-where"><span>zebrilus · pane-stack</span><span class="penna-machine-age">now</span></span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><span class="penna-tone-dot" data-tone="strong"></span><span>Port the bento grid</span></span><span class="penna-machine-state" data-tone="strong"><span>Done</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="chat"></i><i data-icon="globe"></i></span><span class="penna-machine-where"><span>penna · main</span><span class="penna-machine-age">12m</span></span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><span class="penna-tone-dot" data-tone="danger"></span><span>cargo build</span></span><span class="penna-machine-state" data-tone="danger"><span>Exit 101</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="square-terminal"></i></span><span class="penna-machine-where"><span>nidus · main</span><span class="penna-machine-age">2h</span></span></span><span class="penna-press"></span></div>
  <div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name" data-lines="2"><span>zsh</span></span><span class="penna-machine-foot"><span class="penna-machine-kinds"><i data-icon="square-terminal"></i></span><span class="penna-machine-where"><span>.files · main</span><span class="penna-machine-age">3d</span></span></span><span class="penna-press"></span></div>
</div>
```

| State | Mark, word | A tap |
| --- | --- | --- |
| Waiting for you | a warning dot, "Needs approval" or "Has a question" | zooms into the pane that asks |
| Working | a spinner, the work's own message and its percent | zooms into the active pane |
| Done, not seen | a dot in the ink, "Done" | zooms into the pane that finished |
| Failed | a danger dot, "Failed" or "Exit 101" | zooms into the pane that failed |
| Idle | nothing | zooms into the active pane |

Top: the mark, then the title, up to two lines for a window. Under it the state in one line; a figure stays whole and the words before it shorten. At the bottom a 16 glyph per kind of pane, then where it works and how long ago it last changed. Working is a spinner, never green: green means a machine is connected.

Name strong `touch-body`, state and lines `touch-small`, inset 12. The corner is the phone container corner, twice `radius`: 24 by default, 16 in Yaru. No button inside, no live preview, no ghost tiles.

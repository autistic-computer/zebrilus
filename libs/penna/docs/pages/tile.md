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

## Phone tiles and cards

The cells of every screen of things on a phone: `MachineTile` on the overview and `WindowCard` inside a machine. The whole surface is the target, and holding it opens a modal of what you can do to it.

### Machine tiles

```figure uses=compact,touch
<div class="penna-machine-grid" style="width: 370px; max-width: 100%">
  <div class="penna-machine-card" data-penna="phone.card" role="button"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="hard-drive"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">alienware</span><div class="penna-machine-line"><span class="penna-face-stack"><span class="penna-face-chip"><i data-icon="bot"></i></span><span class="penna-face-chip"><i data-icon="square-terminal"></i></span></span></div></div><span class="penna-press"></span></div>
  <div class="penna-machine-card" data-penna="phone.card" data-waiting role="button"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span><span class="penna-machine-waiting">1 waiting</span></div><div class="penna-machine-bottom"><span class="penna-machine-label">macbook</span><div class="penna-machine-line"><span class="penna-face-stack"><span class="penna-face-chip"><i data-icon="bot"></i></span><span class="penna-face-chip"><i data-icon="square-terminal"></i></span><span class="penna-face-chip"><i data-icon="globe"></i></span></span></div></div><span class="penna-press"></span></div>
  <div class="penna-machine-card" data-penna="phone.card" role="button"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="server"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">homelab</span><div class="penna-machine-line"><span class="penna-machine-state">Connecting</span></div></div><span class="penna-press"></span></div>
  <div class="penna-machine-card" data-penna="phone.card" data-dimmed role="button"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">xps</span><div class="penna-machine-line"><span class="penna-machine-state">Not connected</span></div></div><span class="penna-press"></span></div>
  <div class="penna-machine-card" data-penna="phone.card" role="button" data-state="active"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="hard-drive"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">alienware</span><div class="penna-machine-line"><span class="penna-machine-state">Connected</span></div></div><span class="penna-press"></span></div>
  <div class="penna-machine-card" data-penna="phone.add" style="align-items: center; justify-content: center; gap: 6px; color: var(--penna-muted)" role="button"><i data-icon="plus"></i>Add machine<span class="penna-press"></span></div>
</div>
```

`MachineTile` is 112 tall, two to a row: the host's face at the top left, a waiting word in warning ink at the top right (washing the card in 14% warning), the host name below, and either stacked 24 program faces or its connection state. Offline hosts dim their face and name to 55%.

| State | Says | A tap |
| --- | --- | --- |
| Connected | stacked faces of running programs | zooms into the machine |
| Waiting | 14% warning wash and "1 waiting" | zooms into the machine |
| Connecting | "Connecting" | zooms into the [connecting card](zoom-card.html), where Cancel is the one action |
| Offline | dimmed face and name, "Not connected" | reconnects |
| Add | "Add machine" | zooms into the [add card](zoom-card.html) |

### Window cards

```figure uses=compact,touch
<div style="display: flex; flex-direction: column; gap: 8px; width: 370px; max-width: 100%">
  <div class="penna-window-card" data-penna="phone.card" data-waiting role="button">
    <div class="penna-window-row">
      <span class="penna-face-chip"><i data-icon="bot"></i></span>
      <div class="penna-window-row-copy">
        <span class="penna-window-row-title">Fix glass rim parity</span>
        <span class="penna-window-row-detail"><span style="color: var(--penna-warning-ink)">Needs approval</span><span>cargo test -p penna</span></span>
      </div>
      <i data-icon="chevron-right"></i>
    </div>
  </div>
  <div class="penna-window-card" data-penna="phone.card" role="button">
    <div class="penna-window-row">
      <span class="penna-face-chip"><i data-icon="globe"></i></span>
      <div class="penna-window-row-copy">
        <span class="penna-window-row-title">Look · penna</span>
        <span class="penna-window-row-detail"><span>127.0.0.1:8099</span></span>
      </div>
      <i data-icon="chevron-right"></i>
    </div>
  </div>
  <div class="penna-window-card" data-penna="phone.card" role="button">
    <div class="penna-window-tail-card">
      <div class="penna-window-tail-head">
        <span class="penna-face-chip"><i data-icon="square-terminal"></i></span>
        <span class="penna-window-row-title">cargo watch</span>
        <i class="penna-touch-spinner" data-icon="loader"></i>
      </div>
      <div class="penna-window-tails">
        <pre class="penna-window-tail">   Compiling penna v0.1.0
running 7 tests
test theme_css::rim ... ok
test result: ok. 7 passed</pre>
        <div class="penna-window-tail-rule"></div>
        <pre class="penna-window-tail">serving target/docs
GET /look.html 200
GET /phone.html 200
GET /chat.html 200</pre>
      </div>
    </div>
  </div>
</div>
```

`WindowCard` spans the full width in one of two shapes: a 64 row card (face slot, title, status or summary line, trailing chevron) for chats and browsers, or a tail card (24 header over up to four lines of live mono output per split pane, parted by a vertical rule) for terminals. Holding a card opens its modal with Rename and Close window; new windows and sessions are made from the plus disc's menu.

### Phone API

| Rust | HTML | Does |
| --- | --- | --- |
| `MachineTile::new(id, name)` | `.penna-machine-card[data-penna="phone.card"]` | 112 tall host tile |
| `.face()`, `.faces()`, `.waiting()`, `.state()`, `.dimmed()`, `.on_hold()` | `data-waiting`, `data-dimmed`, `.penna-face-stack` | Host face, program faces, waiting wash and hold modal |
| `WindowCard::new(id, title)` | `.penna-window-card[data-penna="phone.card"]` | Full-width window row or tail card |
| `.faces()`, `.line()`, `.state()`, `.working()`, `.waiting()`, `.tail(WindowTail)` | `.penna-window-row`, `.penna-window-tail-card` | Row summary or 4-line mono terminal tail |
| `phone_card_ground(waiting, cx)` | none | Flat card fill with optional 14% warning wash |

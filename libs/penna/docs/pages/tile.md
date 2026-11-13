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

The cell of every screen of things on a phone: machines on the overview, windows inside a machine. The whole tile is the target, and holding it opens a sheet of what you can do to it.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Partly built natively</span><span class="penna-text penna-notice-detail" data-penna-text>The phone tile is native with both contents, its marks, previews and corner. Press and hold is drawn with the HTML kit.</span></div></div></div>
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
<div class="penna-bento" style="--rows: 6; --penna-bento-width: 370px; width: 370px"><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 0; --row: 0; --columns: 4; --span: 2"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="warning"></span><span>Make the pill menu offer New chat</span></span><span class="penna-machine-state" data-tone="warning">Needs approval</span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 0.5; --h: 1"><span class="penna-window-chat"><span class="penna-window-bubble"><i style="--w: 0.92"></i><i style="--w: 0.5"></i></span><span class="penna-window-lines"><i style="--w: 1"></i><i style="--w: 0.9"></i><i style="--w: 0.96"></i><i style="--w: 0.62"></i></span></span></span><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0.5; --y: 0; --w: 0.5; --h: 0.5"><pre class="penna-window-term">~/dev/zebrilus main&#10;<b>❯ cargo test -p zz-client pill</b>&#10;   Compiling zz-client v0.42.0&#10;running 14 tests&#10;test pill::menu_order ... ok&#10;<span data-ink="success">test result: ok. 14 passed</span></pre></span><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0.5; --y: 0.5; --w: 0.5; --h: 0.5"><span class="penna-window-host">127.0.0.1:8090</span><span class="penna-window-page"><i data-bar style="--w: 0.55"></i><i style="--w: 1"></i><i style="--w: 0.9"></i><i style="--w: 0.72"></i><i data-block></i></span></span></span><span class="penna-press"></span></div><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 0; --row: 2; --columns: 2; --span: 2"><span class="penna-machine-name"><i class="penna-touch-spinner" data-icon="loader"></i><span>Fix pane stack hit testing on iPad</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><span class="penna-window-chat"><span class="penna-window-bubble"><i style="--w: 0.8"></i><i style="--w: 0.42"></i></span><span class="penna-window-lines"><i style="--w: 1"></i><i style="--w: 0.94"></i><i style="--w: 0.7"></i></span></span></span></span><span class="penna-press"></span></div><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 2; --row: 2; --columns: 2; --span: 2"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="strong"></span><span>Port the bento grid to penna</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 0.5; --h: 1"><span class="penna-window-chat"><span class="penna-window-bubble"><i style="--w: 0.7"></i></span><span class="penna-window-lines"><i style="--w: 1"></i><i style="--w: 0.88"></i><i style="--w: 0.95"></i><i style="--w: 0.5"></i></span></span></span><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0.5; --y: 0; --w: 0.5; --h: 1"><span class="penna-window-host">docs.rs</span><span class="penna-window-page"><i data-bar style="--w: 0.4"></i><i style="--w: 1"></i><i style="--w: 0.84"></i><i style="--w: 0.92"></i><i data-block></i></span></span></span><span class="penna-press"></span></div><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 0; --row: 4; --columns: 2; --span: 2"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="danger"></span><span>cargo build</span></span><span class="penna-machine-state" data-tone="danger">Exit 101</span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><pre class="penna-window-term">~/dev/nidus warp&#10;<b>❯ cargo build --release</b>&#10;   Compiling nidus v0.9.1&#10;<span data-ink="danger">error[E0425]: cannot find value `edge`</span>&#10;  --&gt; src/warp.rs:88:21&#10;<span data-ink="danger">error: could not compile `nidus`</span></pre></span></span><span class="penna-press"></span></div><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" style="--column: 2; --row: 4; --columns: 2; --span: 2"><span class="penna-machine-name"><span>Phone machines</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><span class="penna-window-host">127.0.0.1:8090</span><span class="penna-window-page"><i data-bar style="--w: 0.55"></i><i style="--w: 1"></i><i style="--w: 0.9"></i><i style="--w: 0.72"></i><i data-block></i></span></span></span><span class="penna-press"></span></div></div>
```

The preview is the tile, and text is one line: the mark, then the title. A window takes half the width with one or two panes, the full width with three, and a taller full width with four or more. The preview is the window's own split, each pane on the ground's fill with a concentric corner and no label: a terminal's top lines as tiny live text, a browser's host over a quiet page, a chat's last message as a bubble over the reply's lines.

| State | Mark, line | A tap |
| --- | --- | --- |
| Waiting for you | a warning dot, "Needs approval" or "Has a question" | zooms into the pane that asks |
| Failed | a danger dot, "Failed" or "Exit 101" | zooms into the pane that failed |
| Working | a spinner | zooms into the pane at work |
| Done, not seen | a dot in the ink | zooms into the pane that finished |
| Idle | nothing | zooms into the active pane |

Words come only when the window asks for you or broke, so those tiles stand out; calm tiles are the mark alone. Folder, branch, age and the work's message are under the title of the hold sheet. Working is a spinner, never green: green means a machine is connected.

```figure uses=compact,touch
<div class="penna-bento" style="--rows: 2; --penna-bento-width: 370px; width: 181px"><div class="penna-bento-tile penna-window-tile" data-penna="phone.card" role="button" data-state="active" style="--column: 0; --row: 0; --columns: 2; --span: 2"><span class="penna-machine-name"><span>cargo watch</span></span><span class="penna-window-preview"><span class="penna-window-pane" data-penna="tile.mini" style="--x: 0; --y: 0; --w: 1; --h: 1"><pre class="penna-window-term">~/dev/zebrilus main&#10;<b>❯ cargo watch -x &#x27;check -p zz-mux&#x27;</b>&#10;[Running &#x27;cargo check -p zz-mux&#x27;]&#10;    Checking zz-mux v0.42.0&#10;<span data-ink="success">    Finished `dev` profile in 2.14s</span></pre></span></span><span class="penna-press"></span></div></div>
```

Pressed, the press covers the whole tile. Mini panes are not targets: the zoom grows the pane the tap opens out of its preview. Title strong `touch-body`, line `touch-small`, inset 12, panes 4 apart. The corner is the phone container corner, twice `radius`, 24 by default and 16 in Yaru; a mini pane's is 12 less, so 12 and 4. No button inside, no ghost tiles. A session's grid has no add tile and its header no button: windows are made from the pill's menu. A machine with no sessions shows one add tile alone, which opens that menu.

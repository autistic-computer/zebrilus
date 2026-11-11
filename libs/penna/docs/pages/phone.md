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
7. **Text says a state, a value or a name.** No eyebrows, subheads or helper lines that explain what the screen already shows.

```figure uses=compact,touch,button,label
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-machine-grid" style="width: 181px; grid-template-columns: 1fr"><div class="penna-machine-tile" data-penna="phone.card" role="button"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-state">Not connected</span><span class="penna-machine-foot"><span>Last seen 2 days ago</span></span><span class="penna-press"></span></div></div></div><figcaption>The tile is the target</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-machine-grid" style="width: 181px; grid-template-columns: 1fr"><div class="penna-machine-tile" data-penna="phone.card"><span class="penna-machine-name"><span class="penna-tone-dot" data-tone="off"></span><span>xps</span></span><span class="penna-machine-state">Not connected</span><span class="penna-machine-foot"><button class="penna-button" data-penna="button" data-size="small" style="align-self: flex-start"><span class="penna-label" data-penna-text>Connect</span></button></span></div></div></div><figcaption>Don't put a small button in it</figcaption></figure>
</div>
```

**Press and hold** a tile, or the header over a group of tiles, for a sheet of what you can do to it: touch rows, the one that closes or removes last in danger ink. One rule for every tile, so nothing that acts on a tile sits inside it. Under the sheet's title, one muted line holds what the tile leaves out: a window's work, folder, branch and age. A header that can be held is 44 tall. Not built natively yet.

```figure uses=sheet,touch,label
<div style="position: relative; width: 370px; height: 330px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
  <div class="penna-sheet" style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-copy"><span class="penna-sheet-title" data-penna-text>Fix pane stack hit testing</span><span class="penna-sheet-detail" data-penna-text>Testing, 62% · zebrilus · pane-stack · now</span></span><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button></div>
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

A screen of things is the same at every level: the large title, then tiles under a section header per group. Machines are square tiles two to a row with the add tile last. A machine's windows are a bento grid per session, sized by what each holds, under the session's name and nothing else; a bento grid has no add tile, since it would leave holes. An empty screen is the title and one add tile; inside a machine it opens the pill's menu. Going into a tile is a zoom, and what opens may be another screen of tiles; the close control, dragging the pill up or a pinch goes back one level.

## Pill

The pill is the one place a machine's windows and sessions are made. Its second line says where they will land: inside a machine, the session in view, the one whose section is under the top row, following the scroll. Tap or hold it for its menu.

```figure uses=compact,touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell"><div class="penna-compact-bar" style="width: 370px; height: auto; padding: 0"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="laptop"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">macbook</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">zz</span><span class="penna-dots"><span class="penna-dot" data-active></span><span class="penna-dot"></span><span class="penna-dot"></span></span></span></span></button></div><span class="docs-note">macbook over the session in view</span></div>
  <div class="docs-cell"><div style="position: relative; width: 390px; height: 600px; overflow: hidden; border-radius: 28px; background: var(--penna-bg); box-shadow: 0 0 0 1px var(--penna-divider)"><div class="penna-stack" data-open><div class="penna-stack-scrim"></div><div class="penna-stack-column" style="right: 0; bottom: 84px; left: 0"><div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating" style="color: var(--penna-muted)"><i data-icon="settings"></i><span>Settings</span></button></div><div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating"><i data-icon="server"></i><span>All machines</span></button></div><div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating"><i data-icon="layers-plus"></i><span>New session</span></button></div><div class="penna-stack-group"><span class="penna-stack-label" data-penna="floating">New in zz</span><button class="penna-stack-chip" data-penna="floating"><i data-icon="globe"></i><span>New browser</span></button><button class="penna-stack-chip" data-penna="floating" data-state="selected"><i data-icon="square-terminal"></i><span>New terminal</span></button><button class="penna-stack-chip" data-penna="floating"><i data-icon="chat-plus"></i><span>New chat</span></button></div></div></div><div class="penna-compact-bar" style="position: absolute; right: 0; bottom: 0; left: 0; height: auto; padding: 0 16px 20px"><button class="penna-compact-pill" data-penna="floating"><span class="penna-pill-tile" data-penna="phone.pill.icon"><i data-icon="laptop"></i></span><span class="penna-pill-copy"><span class="penna-pill-title">macbook</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">zz</span><span class="penna-dots"><span class="penna-dot" data-active></span><span class="penna-dot"></span><span class="penna-dot"></span></span></span></span></button></div></div><span class="docs-note">Held, sliding over New terminal</span></div>
</div>
```

Its menu is a stack menu (below): chips in groups, the likeliest nearest the thumb, Settings apart at the top. A group whose chips make things in one place is headed by a label chip saying where ("New in zz"). What it makes lands in the session the pill names. With no sessions it reads "In a new session" and New session is left out. Not built natively yet: the pill's second line shows a session count, and the menu has no label and no New chat.

## Stack menu

The phone's one menu: chips that rise out of what you pressed, the pill or a key in the key row. Tap the anchor and the stack stays for a tap on a chip; or hold it, slide to a chip and let go, in one stroke.

| Gesture on the pill | Does | Told apart by |
| --- | --- | --- |
| Tap | opens the stack, staying | let go within 280 ms, moved under 10 |
| Hold | opens it under the finger | still within 10 at 280 ms |
| Hold, then slide | picks the chip it lets go on | after the hold the stack owns the touch |
| Sideways swipe | pages | moved 10 before 280 ms, across |
| Drag up | the lift, back one level | moved 10 before 280 ms, upward |

Chips are 52, the floating surface, a 20 glyph and the label in strong `touch-body`, 8 apart in a group and 16 between groups. The chip under the finger inverts and grows 4%, with a tick. Letting go on the anchor keeps the stack open; anywhere else closes it. Close and other danger chips ask first in a sheet. More than six chips in a group go in two columns. Chips never shrink: what does not fit folds behind More. Not built natively yet: the pill's menu opens on a tap only, at 44, and nothing ticks.

```figure uses=touch,compact,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell"><button class="penna-stack-chip" data-penna="floating"><i data-icon="square-terminal"></i><span>New terminal</span></button><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><button class="penna-stack-chip" data-penna="floating" data-state="selected"><i data-icon="square-terminal"></i><span>New terminal</span></button><span class="docs-note">Under the finger</span></div>
  <div class="docs-cell"><button class="penna-stack-chip" data-penna="floating" data-tone="danger"><i data-icon="xmark"></i><span>Close</span></button><span class="docs-note">Danger</span></div>
  <div class="docs-cell"><span class="penna-stack-label" data-penna="floating">New in zz</span><span class="docs-note">Group label, not a target</span></div>
  <div class="docs-cell"><div class="penna-stack-group" data-chords><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^C</span><span>Interrupt</span></button></div><span class="docs-note">Chord</span></div>
</div>
```

## Inside a pane

No header: the content runs to the edges and the pill is the one handle. Its second line takes a window tile's words ("Needs approval" in warning ink, "Failed" in danger ink, the agent's working message), and otherwise where the pane works. Its menu makes a browser, terminal or chat in the session, renames or closes the pane (Close asks first), and goes back to the overview.

**When the keyboard is up, the pill gives its place to what you type with**: the composer in a chat, the key row in a terminal, the address field in a browser, 8 above the keyboard. Keyboard down, the pill is back.

```figure uses=chat,touch,compact,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell" style="width: 370px"><div class="penna-chat-composer"><div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch"><button class="penna-button" data-penna="button.send" data-shape="icon" aria-label="Chat options"><i data-icon="plus"></i></button><span class="penna-chat-input" role="textbox" data-placeholder="Message Claude Code…"></span><button class="penna-button" data-penna="button.send" data-shape="icon" disabled aria-label="Send"><i data-icon="arrow-up"></i></button></div></div><span class="docs-note">Chat: the composer row</span></div>
  <div class="docs-cell" style="width: 370px"><div class="penna-key-row" data-size="touch"><div class="penna-key-row-pill" data-penna="floating"></div><span class="penna-key-row-pin"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Prefix"><span style="display: flex; align-items: center; gap: 1px; --penna-icon-size: 14px"><i data-icon="key-control"></i><span class="penna-text" data-penna-text>b</span></span></button></span></span><div class="penna-key-row-scroll"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Control"><i data-icon="key-control"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Alt"><i data-icon="key-option"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Tab"><i data-icon="key-tab-iso"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Escape"><i data-icon="key-escape"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key"><span class="penna-text" data-penna-text>/</span></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key"><span class="penna-text" data-penna-text>@</span></button></span></div><span class="penna-key-row-pin"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Arrows"><i data-icon="arrows-move"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Hide keyboard"><i data-icon="chevron-down"></i></button></span></span><button class="penna-key-row-more" data-penna="phone.pill" data-side="back" aria-label="Scroll back"><i data-icon="chevron-left"></i></button><button class="penna-key-row-more" data-penna="phone.pill" data-side="ahead" aria-label="Scroll ahead"><i data-icon="chevron-right"></i></button></div><span class="docs-note">Terminal: the key row, 44 keys, both ends pinned</span></div>
</div>
```

The **mini pill** and a **badge on a thumbnail** are the two things drawn smaller than their target. Scrolling a pane shrinks the pill to 32, a handle like a sheet's grabber, and its target stays 44. A badge, like a pasted image's remove control, is an accessory of the picture: a 24 floating disc in its corner, its 44 target centered on it. Not built natively yet: the keyboard rule in a chat and a browser, 44 keys with the pinned end, the 44 target, and the pane menu.

## Terminal

The key row takes the pill's place while the keyboard is up: 44 keys in one 56 pill, the prefix pinned at the start, the arrow pad and hide pinned at the end, and between them esc, tab, ctrl, alt, / and @, most used first, scrolling under a fade. A tap on ctrl or alt latches it for the next key; holding it opens its chords as a stack menu, the most used nearest the key. The prefix opens the pill's own stack. The arrow pad's bubble is the stack's sibling: the cell under the finger inverts and grows like a chip.

```figure uses=touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell" style="width: 361px"><div class="penna-key-row" data-size="touch"><div class="penna-key-row-pill" data-penna="floating"></div><span class="penna-key-row-pin"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Prefix"><span style="display: flex; align-items: center; gap: 1px; --penna-icon-size: 14px"><i data-icon="key-control"></i><span class="penna-text" data-penna-text>b</span></span></button></span></span><div class="penna-key-row-scroll"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Escape"><i data-icon="key-escape"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Tab"><i data-icon="key-tab"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Control"><i data-icon="key-control"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Alt"><i data-icon="key-option"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Slash"><span class="penna-text" data-penna-text>/</span></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="At"><span class="penna-text" data-penna-text>@</span></button></span></div><span class="penna-key-row-pin"><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Arrows"><i data-icon="arrows-move"></i></button></span><span class="penna-key-row-slot"><button class="penna-key" data-penna="phone.key" aria-label="Hide keyboard"><i data-icon="chevron-down"></i></button></span></span><button class="penna-key-row-more" data-penna="phone.pill" data-side="back" aria-label="Scroll back"><i data-icon="chevron-left"></i></button><button class="penna-key-row-more" data-penna="phone.pill" data-side="ahead" aria-label="Scroll ahead"><i data-icon="chevron-right"></i></button></div><span class="docs-note">The key row on a 393 phone</span></div>
  <div class="docs-cell"><div style="display: flex; padding: 6px; font-size: var(--penna-text-touch-small-size); line-height: var(--penna-text-touch-small-line); --penna-icon-size: 20px"><button class="penna-key" data-penna="phone.key" style="width: 44px; height: 44px"><i data-icon="key-control"></i></button></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div style="display: flex; padding: 6px; font-size: var(--penna-text-touch-small-size); line-height: var(--penna-text-touch-small-line); --penna-icon-size: 20px"><button class="penna-key" data-penna="phone.key" style="width: 44px; height: 44px" data-pressed><i data-icon="key-control"></i></button></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div style="display: flex; padding: 6px; font-size: var(--penna-text-touch-small-size); line-height: var(--penna-text-touch-small-line); --penna-icon-size: 20px"><button class="penna-key" data-penna="phone.key" style="width: 44px; height: 44px" data-latched><i data-icon="key-control"></i></button></div><span class="docs-note">Latched</span></div>
  <div class="docs-cell"><div style="display: flex; padding: 6px; font-size: var(--penna-text-touch-small-size); line-height: var(--penna-text-touch-small-line); --penna-icon-size: 20px"><button class="penna-key" data-penna="phone.key" style="width: 44px; height: 44px" data-armed><span style="display: flex; align-items: center; gap: 1px; --penna-icon-size: 14px"><i data-icon="key-control"></i><span class="penna-text" data-penna-text>b</span></span></button></div><span class="docs-note">Prefix armed</span></div>
</div>
```

```figure uses=touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-end">
  <div class="docs-cell"><div class="penna-stack-group" data-chords data-columns="2"><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^A</span><span>Line start</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^E</span><span>Line end</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^W</span><span>Kill word</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^K</span><span>Kill to end</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^R</span><span>History</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^U</span><span>Kill line</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^Z</span><span>Suspend</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^L</span><span>Clear</span></button><button class="penna-stack-chip" data-penna="floating" data-state="selected"><span class="penna-stack-cap">^C</span><span>Interrupt</span></button><button class="penna-stack-chip" data-penna="floating"><span class="penna-stack-cap">^D</span><span>End input</span></button></div><span class="docs-note">Holding ctrl, over ^C</span></div>
  <div class="docs-cell"><span class="penna-arrow-bubble" data-penna="floating" data-size="touch" style="position: static; transform: none"><span></span><span class="penna-arrow-cell" data-penna="phone.arrow-pad" data-state="selected"><i data-icon="arrow-up"></i></span><span></span><span class="penna-arrow-cell" data-penna="phone.arrow-pad"><i data-icon="arrow-left"></i></span><span class="penna-arrow-hub"></span><span class="penna-arrow-cell" data-penna="phone.arrow-pad"><i data-icon="arrow-right"></i></span><span></span><span class="penna-arrow-cell" data-penna="phone.arrow-pad"><i data-icon="arrow-down"></i></span><span></span></span><span class="docs-note">The arrow pad, held</span></div>
  <div class="docs-cell"><div class="penna-find" data-penna="field.composer" style="width: 361px"><button class="penna-button" data-penna="button.quiet" data-shape="icon" aria-label="Close find"><i data-icon="xmark"></i></button><span class="penna-find-query">pill-find</span><span class="penna-find-count">3 of 7</span><button class="penna-button" data-penna="button.quiet" data-shape="icon" aria-label="Previous match"><i data-icon="chevron-up"></i></button><button class="penna-button" data-penna="button.quiet" data-shape="icon" aria-label="Next match"><i data-icon="chevron-down"></i></button></div><span class="docs-note">Find, riding the keyboard</span></div>
</div>
```

Find takes the pill's place by the same rule: a 56 field with a 44 close, the query, the count as its value, and previous and next as 44 buttons. A tap on a link opens a sheet titled with the link, with Open in a browser pane, Open on this phone and Copy. Scrolled back, the pill shrinks to the mini pill and a 44 jump to bottom floats over it. Not built natively yet: 44 keys in one pill with the pinned end, the inverted latch, the chord stacks, the arrow pad's new bubble, find, the link sheet and jump to bottom.

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
- **Floating**: the pill, the stack menu's chips, the arrow pad's bubble, the gear disc, sheets, the key row's pill, the chat composer, and the action bar when content scrolls under it.

Not built natively yet: the pill menu's chips and the key row's pill draw a flat tint today, glass on or off, and the arrow pad's bubble is a popover.

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
| Stack chip (52) | control | 16.2 | a capsule | 13.2 |
| Arrow pad bubble | its 44 cells' corner plus 6 | 19.7 | 28 | 17.2 |
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

- **Tone dot**: 8, beside its word, which takes its tone's ink: green connected, amber waiting for you, red failed, the ink for done and not yet seen, faint off. Idle has no mark. A window tile drops the word for its calm states (working, done and not yet seen, idle) and keeps it for waiting and failed: on a screen of many windows the ones with words are the ones to look at. No state takes the accent: it is a person's pick, and Yaru's orange would read as waiting.
- **Spinner**: 16, one turn every 900ms, for working and connecting. Working is never a green dot: green only means connected.
- **Count badge**: 20, the `touch-small` line.
- **Icon button**: 44 with a 20 icon; over the content, in a 56 disc.

## The pieces

[Button](button.html), [Field](field.html), [List](list.html) for touch rows, [Tile](tile.html) for the phone tile, machine and window, and the [action bar](action-bar.html), [zoom card](zoom-card.html), [sheet](sheet.html) and [empty state](empty-state.html) blocks.

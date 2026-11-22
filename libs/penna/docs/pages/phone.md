# Phone

A phone screen is built from phone components, sized for a thumb from the start. It is not the desktop made bigger: desktop components under a coarse pointer keep their layout and only grow their targets.

## Rules

1. **The whole thing is the target.** A tile, row or card is tapped as one piece; no button sits inside one. More actions live one zoom in, or behind press-and-hold.
2. **Nothing tappable is under 44**, and what is drawn is as big as what is tappable: no small drawing with an invisible hit area.
3. **One main action per screen**, full width, at the bottom where the thumb is. It rides the keyboard. The top of the screen is for reading.
4. **Going into a thing is a zoom**; a short question that returns you where you were is a modal, and a list you pick from is a sheet.
5. **State is said in words and a tone dot**, never by look alone.
6. **Text is touch-body 17** by default, touch-small 15 for details, never under 13.
7. **Text says a state, a value or a name.** No eyebrows, subheads or helper lines that explain what the screen already shows.

```figure uses=compact,touch,button,label
<div class="docs-dodont" data-docs-phone>
  <figure data-verdict="do">
    <div class="docs-dodont-panel docs-ph-dodont">
      <i class="docs-mark" data-icon="circle-check"></i>
      <div class="penna-machine-grid docs-ph-tile-solo">
        <div class="penna-machine-card" data-penna="phone.card" role="button" tabindex="0">
          <div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span></div>
          <div class="penna-machine-bottom">
            <span class="penna-machine-label">xps</span>
            <div class="penna-machine-line"><span class="penna-machine-state">Not connected</span></div>
          </div>
          <span class="penna-press"></span>
        </div>
      </div>
    </div>
    <figcaption>The tile is the target</figcaption>
  </figure>
  <figure data-verdict="dont">
    <div class="docs-dodont-panel docs-ph-dodont">
      <i class="docs-mark" data-icon="circle-x"></i>
      <div class="penna-machine-grid docs-ph-tile-solo">
        <div class="penna-machine-card" data-penna="phone.card">
          <div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span></div>
          <div class="penna-machine-bottom">
            <span class="penna-machine-label">xps</span>
            <div class="penna-machine-line"><button class="penna-button" data-penna="button" data-size="small" type="button"><span class="penna-label" data-penna-text>Connect</span></button></div>
          </div>
        </div>
      </div>
    </div>
    <figcaption>Don't put a small button in it</figcaption>
  </figure>
</div>
```

**Press and hold** a tile, window card or section header for a menu of what you can do to it, in a card in the middle of the screen: its name, then touch rows in one group, the one that closes or removes last in danger ink. One rule for every tile, so nothing that acts on a tile sits inside it. A header that can be held is 44 tall. Closing asks first, in a card with the question, what goes, Cancel and Close; renaming opens a card with the name in a 48 field, Cancel and Rename.

```figure uses=sheet,touch,compact,button,input,label,segmented
<div class="docs-ph-split" data-phone-hold>
  <div class="docs-ph-frame docs-ph-hold-frame">
    <div class="docs-ph-hold-behind">
      <div class="penna-touch-header" data-hold><span>zz</span><span class="penna-touch-count">2</span></div>
      <div class="penna-window-card" data-penna="phone.card" role="button" tabindex="0" data-phone-hold-card>
        <div class="penna-window-row">
          <i data-icon="square-terminal"></i>
          <div class="penna-window-row-copy">
            <span class="penna-window-row-title">Fix hit testing</span>
            <span class="penna-window-row-detail"><span>~/dev/penna</span></span>
          </div>
        </div>
      </div>
      <span class="docs-ph-hold-hint">Tap the card to reopen the menu</span>
    </div>
    <div class="penna-modal">
      <div class="penna-modal-scrim"></div>
      <div class="penna-modal-area">
        <div class="penna-modal-card" data-penna="drawer" data-phone-hold-view="menu">
          <div class="penna-modal-head"><span data-penna-text>Fix hit testing</span><button class="penna-sheet-disc" type="button" data-sheet-close data-phone-hold-dismiss="Dismissed — tap card to reopen" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
          <div class="penna-sheet-groups" data-menu>
            <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
              <div class="penna-touch-row" data-penna="phone.row" role="button" tabindex="0" data-phone-hold-open="rename"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename</span></span></div>
              <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button" tabindex="0" data-phone-hold-open="close"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Close window</span></span></div>
            </div>
          </div>
        </div>
        <div class="penna-modal-card" data-penna="drawer" data-phone-hold-view="rename" hidden>
          <div class="penna-modal-head"><span data-penna-text>Rename window</span></div>
          <label class="penna-input" data-penna="field"><input value="Fix hit testing" aria-label="Window name" /></label>
          <div class="penna-modal-bar">
            <button class="penna-button" data-penna="button" type="button" data-phone-hold-open="menu"><span class="penna-label" data-penna-text>Cancel</span></button>
            <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button" data-phone-hold-dismiss="Renamed — tap card to reopen"><span class="penna-label" data-penna-text>Rename</span></button>
          </div>
        </div>
        <div class="penna-modal-card" data-penna="drawer" data-phone-hold-view="close" hidden>
          <div class="penna-modal-head"><span data-penna-text>Close window?</span></div>
          <p class="penna-modal-detail">1 terminal pane will stop.</p>
          <div class="penna-modal-bar">
            <button class="penna-button" data-penna="button" type="button" data-phone-hold-open="menu"><span class="penna-label" data-penna-text>Cancel</span></button>
            <button class="penna-button" data-penna="button" data-tone="danger" data-prominent type="button" data-phone-hold-dismiss="Closed — tap card to reopen"><span class="penna-label" data-penna-text>Close</span></button>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-ph-aside" data-penna="panel" data-ground>
    <div class="docs-ph-aside-head">
      <span class="docs-ph-aside-title">Modal flow</span>
      <span class="docs-ph-mono" data-phone-hold-status>Hold menu · ruled group, danger last</span>
    </div>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" data-phone-hold-mode role="radiogroup" aria-label="Hold modal step">
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Menu</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Rename</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Close</span></span>
      </div>
    </div>
    <div class="docs-ph-facts">
      <div class="docs-ph-fact"><span class="docs-ph-fact-key">Hold menu</span><span class="docs-ph-fact-val">Ruled touch rows, danger last</span></div>
      <div class="docs-ph-fact"><span class="docs-ph-fact-key">Rename</span><span class="docs-ph-fact-val">48 px field, Cancel and Rename</span></div>
      <div class="docs-ph-fact"><span class="docs-ph-fact-key">Confirm</span><span class="docs-ph-fact-val">Question, what goes, Cancel and Close</span></div>
    </div>
  </div>
</div>
```

## Two touch rungs

```figure
<div class="docs-tokens">
  <span class="docs-head">Rung</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">For</span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-control-touch">control-touch</button></span><span><code>44</code></span><span><code>touch-body</code></span><span>The smallest target, icon buttons, accessories</span>
  <span><button class="docs-clr-token" type="button" data-copy-token="--penna-control-touch-lg">control-touch-lg</button></span><span><code>56</code></span><span><code>touch-body</code></span><span>Main actions and fields</span>
</div>
```

```figure uses=button,input,compact,label
<div class="docs-ph-rungs" data-penna="panel" data-ground>
  <div class="docs-ph-rung-row">
    <div class="docs-ph-rung-controls">
      <button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Settings"><i data-icon="settings"></i></button>
      <button class="penna-button" data-penna="button" data-size="touch" type="button"><span class="penna-label" data-penna-text>Not now</span></button>
    </div>
    <span class="docs-ph-mono">touch · 44 px</span>
  </div>
  <div class="docs-ph-rung-row">
    <div class="docs-ph-rung-controls">
      <label class="penna-input" data-penna="field" data-size="touch-large"><input value="studio.local" aria-label="Touch large host" /></label>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button>
    </div>
    <span class="docs-ph-mono">touch-large · 56 px</span>
  </div>
</div>
```

`touch-large` is as tall as the bar, so a main action lines up with the strip it takes the place of, and a 44 accessory fits inside a 56 field with the 6 inside gap. Under a coarse pointer on a tablet or a touch screen, desktop components take `touch` and the touch scale instead; that is desktop layout made reachable, not a phone screen.

A phone has no Text size or Density knob. It follows the device's text size from 100% to 135% and never goes smaller, since the touch layout already sits on its minimum targets, and it is always comfortable.

## Screen

```figure uses=compact,touch,button,input,label
<div class="docs-ph-split" data-phone-screen>
  <div class="penna-zoom-card docs-ph-zoom" data-penna="phone.card">
    <div class="penna-zoom-top docs-ph-part" data-phone-part="top"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Close"><i data-icon="xmark"></i></button></div>
    <div class="penna-zoom-title docs-ph-part" data-phone-part="title">Add machine</div>
    <div class="penna-zoom-body docs-ph-part" data-phone-part="body">
      <label class="penna-input" data-penna="field" data-size="touch-large"><input placeholder="studio.local" aria-label="Machine host" /></label>
      <div class="penna-touch-group" data-penna="settings.card" data-ground>
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div>
        <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus"></i></div>
      </div>
    </div>
    <div class="penna-action-bar docs-ph-part" data-phone-part="bottom"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button></div>
  </div>
  <div class="docs-ph-zones">
    <button class="docs-ph-zone" data-penna="panel" data-ground type="button" data-phone-zone="top">
      <span class="docs-ph-zone-top"><span class="docs-ph-zone-name">1. Top row</span><span class="docs-ph-mono">44 px</span></span>
      <span class="docs-ph-zone-copy">Close control at the end, or back control at the start.</span>
    </button>
    <button class="docs-ph-zone" data-penna="panel" data-ground type="button" data-phone-zone="title">
      <span class="docs-ph-zone-top"><span class="docs-ph-zone-name">2. Large title</span><span class="docs-ph-mono">52 px · touch-title</span></span>
      <span class="docs-ph-zone-copy">Folds into the top row as the page scrolls.</span>
    </button>
    <button class="docs-ph-zone" data-penna="panel" data-ground type="button" data-phone-zone="body">
      <span class="docs-ph-zone-top"><span class="docs-ph-zone-name">3. Body</span><span class="docs-ph-mono">inset-pane 16 · gap-group 16</span></span>
      <span class="docs-ph-zone-copy">16 px at the sides, groups 16 px apart.</span>
    </button>
    <button class="docs-ph-zone" data-penna="panel" data-ground type="button" data-phone-zone="bottom">
      <span class="docs-ph-zone-top"><span class="docs-ph-zone-name">4. Bottom</span><span class="docs-ph-mono">56 px</span></span>
      <span class="docs-ph-zone-copy">Floating bar on overviews and panes, or action bar on zoom cards and sheets. Never both.</span>
    </button>
  </div>
</div>
```

A screen of things is the same at every level: the large title, then tiles or cards under a section header per group. Machines are 112 tall tiles two to a row with stacked program faces and a 14% warning wash when waiting. A machine's windows are full-width cards per session: a 64 row card for chats and pages, or a 4-line mono tail card for terminals. Going into a card is a zoom; the close control, dragging the bar's handle up or a pinch goes back one level.

## Bar and strip

The bottom bar splits navigation from creation: a 56 floating strip pill — `pill_strip` — beside a 56 action disc, topped by a 28 by 4 grab handle — `pill_handle` — for lifting back one level. Inside the strip, every sibling sits as a 48 mark slot and the open item expands into a capsule showing its title and status line; swiping across the strip pages smoothly between siblings.

```figure uses=compact,touch,button,label
<div class="docs-ph-bar-section">
  <div class="docs-ph-strip-card" data-penna="panel" data-ground data-phone-strip>
    <div class="docs-ph-card-head">
      <span class="docs-ph-aside-title">Handle, strip pill and plus disc</span>
      <span class="docs-ph-mono" data-phone-strip-val>cargo test · ~/dev/penna</span>
    </div>
    <div class="docs-ph-strip-well">
      <div class="penna-bar-stack docs-ph-strip-bar">
        <div class="penna-bar-handle" aria-hidden="true"></div>
        <div class="penna-strip-row">
          <div class="penna-strip-pill" data-penna="floating">
            <button class="penna-strip-slot" type="button" data-active data-penna="phone.pill.icon" data-phone-slot data-phone-slot-label="cargo test · ~/dev/penna" aria-current="true" aria-label="cargo test terminal"><i data-icon="square-terminal"></i><span class="penna-pill-copy"><span class="penna-pill-title">cargo test</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">~/dev/penna</span></span></span></button>
            <button class="penna-strip-slot" type="button" data-phone-slot data-phone-slot-label="Agent · 1 waiting" aria-current="false" aria-label="Agent chat"><i data-icon="bot"></i><span class="penna-pill-copy"><span class="penna-pill-title">Agent</span><span class="penna-pill-meta"><span class="penna-pill-meta-text" data-tone="warning">1 waiting</span></span></span></button>
            <button class="penna-strip-slot" type="button" data-phone-slot data-phone-slot-label="Phone · penna.dev" aria-current="false" aria-label="Browser"><i data-icon="globe"></i><span class="penna-pill-copy"><span class="penna-pill-title">Phone · penna</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">penna.dev</span></span></span></button>
          </div>
          <button class="penna-bar-disc" data-penna="floating" type="button" aria-label="New"><i data-icon="plus"></i></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-ph-split" data-phone-stack-demo>
    <div class="docs-ph-frame docs-ph-stack-frame">
      <div class="docs-ph-frame-bg">
        <div class="penna-machine-grid">
          <div class="penna-machine-card" data-penna="phone.card" data-waiting><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span><span class="penna-machine-waiting">1 waiting</span></div><div class="penna-machine-bottom"><span class="penna-machine-label">macbook</span><div class="penna-machine-line"><span class="penna-face-stack"><span class="penna-face-chip"><i data-icon="bot"></i></span><span class="penna-face-chip"><i data-icon="square-terminal"></i></span></span></div></div></div>
          <div class="penna-machine-card" data-penna="phone.card" data-dimmed><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="hard-drive"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">xps</span><div class="penna-machine-line"><span class="penna-machine-state">Offline</span></div></div></div>
        </div>
      </div>
      <div class="penna-stack" data-open>
        <div class="penna-stack-scrim"></div>
        <div class="penna-stack-column docs-ph-stack-col">
          <div class="penna-stack-group"><button class="penna-stack-chip docs-ph-quiet" data-penna="floating" type="button"><i data-icon="settings"></i><span>Settings</span></button></div>
          <div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="server"></i><span>All machines</span></button></div>
          <div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="layers-plus"></i><span>New session</span></button></div>
          <div class="penna-stack-group"><span class="penna-stack-label" data-penna="floating">New in zz</span><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="globe"></i><span>New browser</span></button><button class="penna-stack-chip" data-penna="floating" data-state="selected" type="button"><i data-icon="square-terminal"></i><span>New terminal</span></button><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="chat-plus"></i><span>New chat</span></button></div>
        </div>
      </div>
      <div class="penna-bar-stack docs-ph-bar-dock">
        <div class="penna-bar-handle" aria-hidden="true"></div>
        <div class="penna-strip-row">
          <div class="penna-strip-pill" data-penna="floating">
            <span class="penna-strip-slot" data-active data-penna="phone.pill.icon"><i data-icon="laptop"></i><span class="penna-pill-copy"><span class="penna-pill-title">macbook</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">zz</span></span></span></span>
            <span class="penna-strip-slot"><i data-icon="hard-drive"></i></span>
          </div>
          <button class="penna-bar-disc" data-penna="floating" type="button" data-phone-stack-toggle aria-label="New"><i data-icon="plus"></i></button>
        </div>
      </div>
    </div>
    <div class="docs-ph-aside" data-penna="panel" data-ground>
      <div class="docs-ph-aside-head">
        <span class="docs-ph-aside-title">Plus disc menu</span>
        <span class="docs-ph-mono" data-phone-stack-val>Selected · New terminal</span>
      </div>
      <p class="docs-ph-aside-copy">The plus disc opens a stack menu of what can be created in the current session, headed by a label chip, with Settings and All machines above.</p>
      <div class="docs-ph-facts">
        <div class="docs-ph-fact"><span class="docs-ph-fact-key">Tap strip</span><span class="docs-ph-fact-val">Opens the active pane's actions</span></div>
        <div class="docs-ph-fact"><span class="docs-ph-fact-key">Swipe strip</span><span class="docs-ph-fact-val">Pages smoothly between siblings</span></div>
        <div class="docs-ph-fact"><span class="docs-ph-fact-key">Drag handle up</span><span class="docs-ph-fact-val">Lifts back one level</span></div>
      </div>
      <div class="docs-ph-aside-foot">
        <button class="penna-button" data-penna="button" type="button" data-phone-stack-btn><span class="penna-label" data-penna-text>Close stack</span></button>
      </div>
    </div>
  </div>
</div>
```

## Stack menu

The phone's one menu: chips that rise out of what you pressed, the plus disc, the active strip capsule or a key in the key row. Tap the anchor and the stack stays for a tap on a chip; or hold it, slide to a chip and let go, in one stroke.

| Gesture | Does | Told apart by |
| --- | --- | --- |
| Tap | opens the stack, staying | let go within 280 ms, moved under 10 |
| Hold | opens it under the finger | still within 10 at 280 ms |
| Hold, then slide | picks the chip it lets go on | after the hold the stack owns the touch |
| Sideways swipe on strip | pages between siblings | moved 10 before 280 ms, across |
| Drag up on handle or strip | lifts back one level | moved 10 before 280 ms, upward |

Chips are 52, the floating surface, a 20 glyph and the label in strong `touch-body`, 8 apart in a group and 16 between groups. The chip under the finger inverts to solid primary and grows 4%, with a tick. Letting go on the anchor keeps the stack open; anywhere else closes it. Close and other danger chips ask first in a modal. More than six chips in a group go in two columns. Chips never shrink: what does not fit folds behind More.

```figure uses=touch,compact,button,label
<div class="docs-ph-chips" data-penna="panel" data-ground>
  <div class="docs-ph-chip-item"><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="square-terminal"></i><span>New terminal</span></button><span class="docs-ph-mono">Rest</span></div>
  <div class="docs-ph-chip-item"><button class="penna-stack-chip" data-penna="floating" data-state="selected" type="button"><i data-icon="square-terminal"></i><span>New terminal</span></button><span class="docs-ph-mono">Under the finger</span></div>
  <div class="docs-ph-chip-item"><button class="penna-stack-chip" data-penna="floating" data-tone="danger" type="button"><i data-icon="xmark"></i><span>Close</span></button><span class="docs-ph-mono">Danger</span></div>
  <div class="docs-ph-chip-item"><span class="penna-stack-label" data-penna="floating">New in zz</span><span class="docs-ph-mono">Group label</span></div>
  <div class="docs-ph-chip-item"><div class="penna-stack-group" data-chords><button class="penna-stack-chip" data-penna="floating" type="button"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>C</span><span>Interrupt</span></button></div><span class="docs-ph-mono">Chord</span></div>
</div>
```

## Inside a pane

No header: the content runs to the edges and the bottom strip is the one handle. Its second line takes a window's status ("Needs approval" in warning ink, "Failed" in danger ink, the agent's working message), and otherwise where the pane works.

**When the keyboard is up, the bar gives its place to what you type with**: the composer in a chat, the key row in a terminal, the address field in a browser, 8 above the keyboard. Keyboard down, the bar is back. A **badge on a thumbnail** is the one control drawn smaller than its target: a 24 floating disc in the corner of an 88 image or file tile, with its 44 target centered on it.

```figure uses=chat,touch,compact,button,label,switch
<div class="docs-ph-pane-split">
  <div class="docs-pr-col" data-phone-kbd-rule>
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Keyboard rule</span>
      <span class="docs-ph-switch-row">
        <span class="docs-pr-col-meta">Keyboard</span>
        <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Toggle keyboard" data-phone-kbd-switch type="button"></button>
      </span>
    </div>
    <div class="docs-ph-pane-card" data-penna="panel" data-ground>
      <div class="docs-ph-pane-stage">
        <div class="docs-ph-pane-transcript">
          <span class="docs-ph-pane-msg">Updated corner scaling for floating panels.</span>
        </div>
        <div class="docs-ph-dock-slot" data-phone-dock="composer">
          <div class="penna-chat-composer">
            <div class="penna-chat-field input-chrome" data-penna="field.composer" data-size="touch">
              <button class="penna-button" data-penna="button.send" data-shape="icon" type="button" aria-label="Chat options"><i data-icon="sliders"></i></button>
              <input class="penna-chat-input docs-ph-chat-input" type="text" placeholder="Message agent…" aria-label="Message agent" data-phone-compose-input />
              <button class="penna-button" data-penna="button.send" data-shape="icon" disabled type="button" aria-label="Send" data-phone-compose-send><i data-icon="arrow-up"></i></button>
            </div>
          </div>
          <div class="docs-ph-kbd-tray">Keyboard · 8 px below</div>
        </div>
        <div class="docs-ph-dock-slot" data-phone-dock="bar" hidden>
          <div class="penna-bar-stack docs-ph-strip-bar">
            <div class="penna-bar-handle" aria-hidden="true"></div>
            <div class="penna-strip-row">
              <div class="penna-strip-pill" data-penna="floating">
                <span class="penna-strip-slot" data-active data-penna="phone.pill.icon"><i data-icon="bot"></i><span class="penna-pill-copy"><span class="penna-pill-title">Agent</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">~/dev/penna</span></span></span></span>
                <span class="penna-strip-slot"><i data-icon="square-terminal"></i></span>
              </div>
              <button class="penna-bar-disc" data-penna="floating" type="button" aria-label="New"><i data-icon="plus"></i></button>
            </div>
          </div>
        </div>
      </div>
      <span class="docs-ph-mono" data-phone-kbd-val>Composer · 8 px above keyboard</span>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Thumbnail badge</span>
      <span class="docs-pr-col-meta">24 in 44</span>
    </div>
    <div class="docs-ph-thumb-card" data-penna="panel" data-ground>
      <div class="docs-ph-thumb-stage">
        <span class="penna-chat-file">
          <i data-icon="file"></i>
          <span class="penna-chat-file-copy">
            <span class="penna-chat-file-name">kit.rs</span>
            <span class="penna-chat-file-meta">28 KB</span>
          </span>
          <button class="penna-chat-thumb-remove docs-ph-hit-ring" type="button" aria-label="Remove file"><span class="penna-chat-thumb-disc" data-penna="floating"><i data-icon="xmark"></i></span></button>
        </span>
      </div>
      <span class="docs-ph-mono">24 px disc · 44 px target</span>
    </div>
  </div>
</div>
```

## Terminal

The key row takes the bar's place while the keyboard is up: one 56 floating pill of four evenly shared 48 keys — `esc`, `tab`, `ctrl`, `alt` — beside two 56 floating discs 8 apart: the thumbstick arrow pad and done to drop the keyboard. Keys rest bare on the pill as `phone.key` and take the `phone.pill.icon` wash while pressed or latched. Holding `ctrl` or `alt` opens its chords as a stack menu; dragging the arrow disc sends arrows in any direction without raising a second overlay.

```figure uses=touch,button,label
<div class="docs-ph-term-stack">
  <div class="docs-ph-term-card" data-penna="panel" data-ground data-phone-term>
    <div class="docs-ph-card-head">
      <span class="docs-ph-aside-title">Split key row</span>
      <span class="docs-ph-mono" data-phone-term-val>Ctrl latched</span>
    </div>
    <div class="docs-ph-strip-well">
      <div class="penna-key-bar docs-ph-key-bar">
        <div class="penna-key-pill" data-penna="floating">
          <button class="penna-key-cap" data-penna="phone.key" type="button" data-phone-tap="Escape" aria-label="Escape"><i data-icon="key-escape"></i></button>
          <button class="penna-key-cap" data-penna="phone.key" type="button" data-phone-tap="Tab" aria-label="Tab"><i data-icon="key-tab"></i></button>
          <button class="penna-key-cap" data-penna="phone.pill.icon" type="button" data-phone-mod="Ctrl" aria-pressed="true" aria-label="Control"><i data-icon="key-control"></i></button>
          <button class="penna-key-cap" data-penna="phone.key" type="button" data-phone-mod="Alt" aria-pressed="false" aria-label="Alt"><i data-icon="key-option"></i></button>
        </div>
        <button class="penna-key-disc" data-penna="floating" type="button" data-phone-arrows aria-pressed="false" aria-label="Arrows"><i data-icon="arrows-move"></i></button>
        <button class="penna-key-disc" data-penna="floating" type="button" data-phone-tap="Keyboard dismissed" aria-label="Done"><i data-icon="check"></i></button>
      </div>
    </div>
    <div class="docs-ph-key-states">
      <div class="docs-ph-key-state">
        <div class="penna-key-pill docs-ph-key-solo" data-penna="floating"><button class="penna-key-cap" data-penna="phone.key" type="button" aria-label="Control rest"><i data-icon="key-control"></i></button></div>
        <span class="docs-ph-mono">Rest</span>
      </div>
      <div class="docs-ph-key-state">
        <div class="penna-key-pill docs-ph-key-solo" data-penna="floating"><button class="penna-key-cap" data-penna="phone.pill.icon" type="button" aria-label="Control latched"><i data-icon="key-control"></i></button></div>
        <span class="docs-ph-mono">Latched / pressed</span>
      </div>
      <div class="docs-ph-key-state">
        <button class="penna-key-disc" data-penna="floating" data-armed type="button" aria-label="Arrows active"><i data-icon="arrows-move"></i></button>
        <span class="docs-ph-mono">Disc active</span>
      </div>
    </div>
  </div>
  <div class="docs-ph-term-card" data-penna="panel" data-ground data-phone-chords>
    <div class="docs-ph-card-head">
      <span class="docs-ph-aside-title">Holding ctrl</span>
      <span class="docs-ph-mono" data-phone-chord-val>Ctrl+C · Interrupt</span>
    </div>
    <div class="docs-ph-chords-well">
      <div class="penna-stack-group docs-ph-chords-grid" data-chords data-columns="2">
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+A · Line start"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>A</span><span>Line start</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+E · Line end"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>E</span><span>Line end</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+W · Kill word"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>W</span><span>Kill word</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+K · Kill to end"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>K</span><span>Kill to end</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+R · History"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>R</span><span>History</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+U · Kill line"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>U</span><span>Kill line</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+Z · Suspend"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>Z</span><span>Suspend</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+L · Clear"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>L</span><span>Clear</span></button>
        <button class="penna-stack-chip" data-penna="floating" data-state="selected" type="button" data-phone-chord="Ctrl+C · Interrupt"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>C</span><span>Interrupt</span></button>
        <button class="penna-stack-chip" data-penna="floating" type="button" data-phone-chord="Ctrl+D · End input"><span class="penna-stack-cap docs-ph-cap"><i data-icon="key-control"></i>D</span><span>End input</span></button>
      </div>
    </div>
  </div>
  <div class="docs-ph-term-card" data-penna="panel" data-ground data-phone-find>
    <div class="docs-ph-card-head">
      <span class="docs-ph-aside-title">Find riding the keyboard</span>
      <span class="docs-ph-mono">56 px field · 44 px buttons</span>
    </div>
    <div class="docs-ph-strip-well">
      <div class="penna-find docs-ph-find-bar" data-penna="field.composer">
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" data-phone-find-close aria-label="Close find"><i data-icon="xmark"></i></button>
        <span class="penna-find-query">pill-find</span>
        <span class="penna-find-count">3 of 7</span>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" data-phone-find-prev aria-label="Previous match"><i data-icon="chevron-up"></i></button>
        <button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" data-phone-find-next aria-label="Next match"><i data-icon="chevron-down"></i></button>
      </div>
    </div>
  </div>
</div>
```

Find takes the bar's place by the same rule — `phone_find_field`, or `phone_field_at_rest` when the keyboard is dismissed — a 56 field with a 44 close, the query, the count as its value, and previous and next as 44 buttons. A tap on a link opens a sheet titled with the link, with Open in a browser pane, Open on this phone and Copy. Scrolled back, a 44 jump to bottom floats over the bar.

## Browser

No header and no bar: the page runs to the edges and the bottom strip shows the page's title over its host. Its stack leads with a glyph row of Back, Forward and Reload, then the address, then Tabs, Pick element and Open on this phone; what the bar offers on every pane waits behind More. Hold the strip, slide onto Back and let go: back in one stroke.

```figure uses=touch,compact,button,label
<div class="docs-ph-browser-grid" data-phone-browser>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Browser stack</span>
      <span class="docs-pr-col-meta" data-phone-browser-val>Back</span>
    </div>
    <div class="docs-ph-frame docs-ph-browser-frame">
      <span class="docs-ph-progress" aria-hidden="true"></span>
      <div class="penna-stack" data-open data-phone-browser-stack>
        <div class="penna-stack-scrim"></div>
        <div class="penna-stack-column docs-ph-stack-col">
          <div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="ellipsis"></i><span>More</span></button></div>
          <div class="penna-stack-group">
            <button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="external-link"></i><span>Open on this phone</span></button>
            <button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="inspector"></i><span>Pick element</span></button>
            <button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="layers"></i><span>Tabs</span><span class="penna-stack-value">3</span></button>
          </div>
          <div class="penna-stack-group"><button class="penna-stack-chip" data-penna="floating" type="button"><i data-icon="globe"></i><span>github.com/…/pull/412</span></button></div>
          <div class="penna-stack-group" data-row>
            <button class="penna-stack-chip" data-penna="floating" data-glyph data-state="selected" type="button" aria-label="Back"><i data-icon="arrow-left"></i></button>
            <button class="penna-stack-chip" data-penna="floating" data-glyph disabled type="button" aria-label="Forward"><i data-icon="arrow-right"></i></button>
            <button class="penna-stack-chip" data-penna="floating" data-glyph type="button" aria-label="Reload"><i data-icon="redo-2"></i></button>
          </div>
        </div>
      </div>
      <div class="penna-bar-stack docs-ph-bar-dock">
        <div class="penna-bar-handle" aria-hidden="true"></div>
        <div class="penna-strip-row">
          <div class="penna-strip-pill" data-penna="floating">
            <span class="penna-strip-slot" data-active data-penna="phone.pill.icon"><i data-icon="globe"></i><span class="penna-pill-copy"><span class="penna-pill-title">Browser touch pane</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">github.com</span></span></span></span>
          </div>
          <button class="penna-bar-disc" data-penna="floating" type="button" aria-label="New"><i data-icon="plus"></i></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Address &amp; suggestions</span>
      <span class="docs-pr-col-meta">riding keyboard</span>
    </div>
    <div class="docs-ph-frame docs-ph-addr-frame">
      <span class="docs-ph-progress" aria-hidden="true"></span>
      <div class="docs-ph-addr-dock">
        <div class="penna-touch-group" data-penna="field.composer">
          <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" tabindex="0" data-phone-suggest="penna.dev/stack-menu"><i data-icon="history"></i><span class="penna-touch-row-lines"><span>Stack menu · penna</span><span>penna.dev/stack-menu</span></span></div>
          <div class="penna-touch-row" data-penna="phone.row" data-lines="2" role="button" tabindex="0" data-phone-suggest="penna.dev/phone"><i data-icon="history"></i><span class="penna-touch-row-lines"><span>Phone · penna</span><span>penna.dev/phone</span></span></div>
        </div>
        <div class="penna-address" data-penna="field.composer">
          <button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" data-phone-addr-reset aria-label="Close address"><i data-icon="xmark"></i></button>
          <span class="penna-address-text"><span data-phone-addr-text>penna.dev/p</span></span>
          <button class="penna-button" data-penna="button.quiet" data-shape="icon" type="button" data-phone-addr-clear aria-label="Clear"><i data-icon="circle-x"></i></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

Picking the address raises the keyboard and the address field takes the bar's place, like find: a 56 field with a 44 close, the address all selected, and a 44 clear; go is on the keyboard. Suggestions — `phone_suggestions` — sit as two-line rows just above it, the best nearest. A page loading draws a 2 line in the accent along its top edge, and Reload turns to Stop. When a field on the page has the keyboard, the bar hides and nothing rides it.

## Surfaces

Flat by default; glass only for what sits over something. Tiles, window cards, rows, fields and the main action are flat fills on the screen, which is the window's own background. The strip pill, action discs, stack chips, sheets, key row pill and chat composer float: glass in the mode's look when glass is on, with the look's float shadow, and with glass off the popover fill, a hairline all round and a close shadow.

```figure uses=compact,touch,label
<div class="docs-ph-surfaces">
  <div class="docs-pr-col" data-look="glass-liquid">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Liquid glass</span>
      <span class="docs-pr-col-meta">blur + rim</span>
    </div>
    <div class="docs-ph-frame docs-ph-surface-frame">
      <div class="docs-ph-surface-body">
        <div class="penna-machine-grid">
          <div class="penna-machine-card" data-penna="phone.card" data-waiting><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span><span class="penna-machine-waiting">1 waiting</span></div><div class="penna-machine-bottom"><span class="penna-machine-label">macbook</span><div class="penna-machine-line"><span class="penna-face-stack"><span class="penna-face-chip"><i data-icon="bot"></i></span><span class="penna-face-chip"><i data-icon="square-terminal"></i></span></span></div></div></div>
          <div class="penna-machine-card" data-penna="phone.card" data-dimmed><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="hard-drive"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">xps</span><div class="penna-machine-line"><span class="penna-machine-state">Offline</span></div></div></div>
        </div>
        <div class="penna-touch-group" data-penna="settings.card" data-ground>
          <div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div>
          <div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus"></i></div>
        </div>
        <div class="docs-ph-surface-under" aria-hidden="true"><i></i><i></i><i></i></div>
      </div>
      <div class="penna-bar-stack docs-ph-bar-dock">
        <div class="penna-bar-handle" aria-hidden="true"></div>
        <div class="penna-strip-row">
          <div class="penna-strip-pill" data-penna="floating"><span class="penna-strip-slot" data-active data-penna="phone.pill.icon"><i data-icon="server"></i><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></span></div>
          <button class="penna-bar-disc" data-penna="floating" type="button" aria-label="New"><i data-icon="plus"></i></button>
        </div>
      </div>
    </div>
  </div>
  <div class="docs-pr-col" data-look="glass-none">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Glass off</span>
      <span class="docs-pr-col-meta">popover + hairline</span>
    </div>
    <div class="docs-ph-frame docs-ph-surface-frame">
      <div class="docs-ph-surface-body">
        <div class="penna-machine-grid">
          <div class="penna-machine-card" data-penna="phone.card" data-waiting><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span><span class="penna-machine-waiting">1 waiting</span></div><div class="penna-machine-bottom"><span class="penna-machine-label">macbook</span><div class="penna-machine-line"><span class="penna-face-stack"><span class="penna-face-chip"><i data-icon="bot"></i></span><span class="penna-face-chip"><i data-icon="square-terminal"></i></span></span></div></div></div>
          <div class="penna-machine-card" data-penna="phone.card" data-dimmed><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="hard-drive"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">xps</span><div class="penna-machine-line"><span class="penna-machine-state">Offline</span></div></div></div>
        </div>
        <div class="penna-touch-group" data-penna="settings.card" data-ground>
          <div class="penna-touch-row" data-penna="phone.row"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div>
          <div class="penna-touch-row" data-penna="phone.row"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>homelab</span></span><i data-icon="plus"></i></div>
        </div>
        <div class="docs-ph-surface-under" aria-hidden="true"><i></i><i></i><i></i></div>
      </div>
      <div class="penna-bar-stack docs-ph-bar-dock">
        <div class="penna-bar-handle" aria-hidden="true"></div>
        <div class="penna-strip-row">
          <div class="penna-strip-pill" data-penna="floating"><span class="penna-strip-slot" data-active data-penna="phone.pill.icon"><i data-icon="server"></i><span class="penna-pill-copy"><span class="penna-pill-title">Machines</span><span class="penna-pill-meta"><span class="penna-pill-meta-text">2 connected</span></span></span></span></div>
          <button class="penna-bar-disc" data-penna="floating" type="button" aria-label="New"><i data-icon="plus"></i></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

- **Flat**: the screen, machine tiles, window cards, rows and row groups, fields, the empty state, the zoom card, the key row's keys at rest.
- **Solid**: the main action and the highlighted stack chip, never glass.
- **Floating**: the strip pill, action discs, stack menu chips, sheets, the key row's pill and discs, the chat composer, the find and address fields, suggestions, and the action bar when content scrolls under it.

## Corners

The desktop's four corner rules, unchanged. Controls round from their height, the floating disc included. Phone containers share one corner, twice `radius`, since the phone's 56 rung is twice the desktop's 28. A settings card is one too. A phone has no Corner radius knob, so `radius` is the look's default and phone containers are 24. Rows inside a group are concentric with it.

```figure uses=compact,touch,button,label,segmented looks=default,pill
<div class="docs-ph-corners" data-phone-corners>
  <div class="docs-sz-density-bar" data-penna="panel" data-ground>
    <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" data-phone-corners-control role="radiogroup" aria-label="Controls corner style">
      <div class="penna-segments" data-penna="button.quiet" data-state="current">
        <span class="penna-segment-pill"></span>
        <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Rounded</span></span>
        <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Pill</span></span>
      </div>
    </div>
    <span class="docs-ph-mono" data-phone-corners-val>Rounded · radius 12</span>
  </div>
  <div class="docs-ph-corners-stage" data-phone-corners-stage>
    <div class="docs-ph-corners-controls">
      <div class="docs-ph-corner-cell" data-penna="panel" data-ground>
        <div class="docs-ph-corner-slot"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Settings"><i data-icon="settings"></i></button></div>
        <span class="docs-ph-mono">Icon button · 44</span>
      </div>
      <div class="docs-ph-corner-cell" data-penna="panel" data-ground>
        <div class="docs-ph-corner-slot"><span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Settings in disc"><i data-icon="settings"></i></button></span></div>
        <span class="docs-ph-mono">Disc · 56</span>
      </div>
      <div class="docs-ph-corner-cell" data-penna="panel" data-ground>
        <div class="docs-ph-corner-slot"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" type="button"><span class="penna-label" data-penna-text>Connect</span></button></div>
        <span class="docs-ph-mono">Main action · 56</span>
      </div>
    </div>
    <div class="docs-ph-corners-containers">
      <div class="docs-ph-corner-box">
        <div class="penna-touch-group" data-penna="settings.card" data-ground>
          <div class="penna-touch-row" data-penna="phone.row" role="button" data-state="hover"><i data-icon="laptop"></i><span class="penna-touch-row-lines"><span>macbook</span></span><i data-icon="plus"></i></div>
          <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="hard-drive"></i><span class="penna-touch-row-lines"><span>alienware</span></span><i data-icon="plus"></i></div>
        </div>
        <span class="docs-ph-mono">Row group · 24 (rows stay rounded)</span>
      </div>
      <div class="docs-ph-corner-box">
        <div class="penna-machine-grid docs-ph-tile-fill">
          <div class="penna-machine-card" data-penna="phone.card" role="button"><div class="penna-machine-top"><span class="penna-face-chip"><i data-icon="laptop"></i></span></div><div class="penna-machine-bottom"><span class="penna-machine-label">xps</span><div class="penna-machine-line"><span class="penna-machine-state">Connected</span></div></div><span class="penna-press"></span></div>
        </div>
        <span class="docs-ph-mono">Machine tile · 24</span>
      </div>
    </div>
  </div>
</div>
```

| Piece | Kind | Rounded | Pill |
| --- | --- | --- | --- |
| Main action, field, strip, disc (56) | control | 17.5 | capsule |
| Icon button (44) | control | 13.7 | circle |
| Icon button in the disc | disc less 6 | 11.5 | circle |
| Stack chip (52) | control | 16.2 | capsule |
| Stack glyph chip (52 square) | control | 16.2 | circle |
| Find and address fields | 44 buttons plus 6 | 19.7 | 28 |
| Progress line | square ends | 0 | 0 |
| Touch row, 52 and 64 | control, never a capsule | 16.2, 20 | 16.2, 20 |
| Touch row in a group | at most group less 4 | 16.2, 20 | 16.2, 20 |
| Tiles, cards, groups, sheets, zoom, suggestions | phone container, twice `radius` | 24 | 24 |

## Status marks

```figure uses=touch,compact,badge,label
<div class="docs-ph-marks" data-penna="panel" data-ground>
  <div class="docs-ph-marks-grid">
    <span class="docs-ph-mark-cell"><span class="penna-tone-dot"></span><span class="penna-type" data-type="touch-small" data-penna-text>Connected</span></span>
    <span class="docs-ph-mark-cell"><span class="penna-tone-dot" data-tone="warning"></span><span class="penna-type" data-type="touch-small" data-penna-text style="color: var(--penna-warning-ink)">1 waiting</span></span>
    <span class="docs-ph-mark-cell"><span class="penna-tone-dot" data-tone="danger"></span><span class="penna-type" data-type="touch-small" data-penna-text style="color: var(--penna-danger-ink)">Failed</span></span>
    <span class="docs-ph-mark-cell"><span class="penna-tone-dot" data-tone="strong"></span><span class="penna-type" data-type="touch-small" data-penna-text>Done</span></span>
    <span class="docs-ph-mark-cell"><span class="penna-tone-dot" data-tone="off"></span><span class="penna-type" data-type="touch-small" data-penna-text>Not connected</span></span>
    <span class="docs-ph-mark-cell"><i class="penna-touch-spinner" data-icon="loader"></i><span class="penna-type" data-type="touch-small" data-penna-text>Connecting</span></span>
  </div>
  <div class="docs-ph-marks-foot">
    <div class="docs-ph-marks-extra">
      <span class="docs-ph-mark-cell"><span class="penna-touch-badge" data-penna="badge">3</span><span class="penna-type" data-type="touch-small" data-penna-text>chats</span></span>
      <span class="docs-ph-mono">count badge · 20 px</span>
    </div>
    <div class="docs-ph-marks-extra">
      <span class="penna-touch-disc" data-penna="floating"><button class="penna-touch-icon" data-penna="phone.button" type="button" aria-label="Settings"><i data-icon="settings"></i></button></span>
      <span class="docs-ph-mono">44 icon in 56 disc</span>
    </div>
  </div>
</div>
```

- **Tone dot**: 8, beside its word, which takes its tone's ink: green connected, amber waiting for you, red failed, the ink for done and not yet seen, faint off. Idle has no mark. Calm windows drop the word and keep it for waiting and failed so the ones asking for attention stand out. No state takes the accent: it is a person's pick, and an orange or red accent would read as waiting or failed.
- **Spinner**: 16, one turn every 900ms, for working and connecting. Working is never a green dot: green only means connected.
- **Count badge**: 20, the `touch-small` line.
- **Icon button**: 44 with a 20 icon; over the content, in a 56 disc.

## The pieces

[Button](button.html), [Field](field.html), [List](list.html) for touch rows, [Tile](tile.html) for machine tiles and window cards, and the [action bar](action-bar.html), [zoom card](zoom-card.html), [sheet](sheet.html) and [empty state](empty-state.html) blocks.

# Sheet

Asks a short question and returns you where you were: a password, a confirmation, a few choices. To go into something, a zoom card.

## Anatomy

```figure uses=sheet,touch,button,input,label
<div style="position: relative; width: 390px; height: 420px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
  <div class="penna-sheet" style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-title" data-penna-text>Sign in to xps</span><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button></div>
        <div class="penna-sheet-content" style="gap: 8px; padding: 0 16px">
          <span class="penna-type" data-type="touch-body" data-muted data-penna-text>fabrico@xps's password:</span>
          <label class="penna-input" data-penna="field" data-size="touch-large" data-state="focused"><input type="password" value="hunter22" /></label>
        </div>
        <div class="penna-action-bar" data-keys><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Sign in</span></button></div>
      </div>
    </div>
  </div>
</div>
```

1. **Panel**: the phone container corner, twice `radius`; rows inside it are never rounder than it less the row inset.
2. **Grabber**: 36 by 5. Drag down past half its height, or fling, to dismiss.
3. **Header**: 60, the title in `display`, the close control at its end.
4. **Body**: `inset-pane` 16, `touch-body`; rows are touch rows.
5. **Action bar**: on a phone, riding the keyboard.

## Close

```figure uses=sheet,button,notice
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button><span class="docs-note">44</span></div>
</div>
```

Nothing tappable is under 44 on a phone, so the close is on the touch rung and rounds like any icon button of that height. The panel takes the phone corner, and its rows round no further than it less their inset.

## A question from ssh

Titled for the machine, ssh's own prompt word for word over a 56 field, and an action that says what it does: "Sign in", not "Continue". It comes up over whatever is on screen; closing it cancels the connect. Not built: the thin client asks in a centered dialog.

## Press and hold

```figure uses=sheet,touch,label
<div style="position: relative; width: 390px; height: 330px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
  <div class="penna-sheet" style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-title" data-penna-text>macbook</span><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button></div>
        <div class="penna-sheet-list">
          <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename</span></span></div>
          <div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Remove</span></span></div>
        </div>
      </div>
    </div>
  </div>
</div>
```

Holding a tile, or the header over a group of tiles, opens a sheet titled with its name, with one muted line under the title for what a tile leaves out (a window's work, folder, branch and age): then one row per thing you can do to it, the one that closes or removes last in danger ink, and no action bar, since each row is an action. Natively a machine's sheet is built; windows and sessions are next.

## Confirm and rename

```figure uses=sheet,touch,button,input,label
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div style="position: relative; width: 370px; height: 300px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
    <div class="penna-sheet" style="--penna-sheet-rest: 8px">
      <div class="penna-sheet-scrim"></div>
      <div class="penna-sheet-frame">
        <div class="penna-sheet-panel" data-penna="drawer">
          <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
          <div class="penna-sheet-header"><span class="penna-sheet-copy"><span class="penna-sheet-title" data-penna-text>Close this pane?</span><span class="penna-sheet-detail" data-penna-text>The chat stops. Its transcript stays.</span></span></div>
          <div class="penna-action-bar" style="padding-top: 8px"><button class="penna-button" data-penna="button.quiet" data-size="touch-large"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="danger" data-prominent data-size="touch-large" data-emphasis="solid" data-solid="danger" style="color: var(--penna-on-danger)"><span class="penna-label" data-penna-text>Close</span></button></div>
        </div>
      </div>
    </div>
  </div>
  <div style="position: relative; width: 370px; height: 300px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
    <div class="penna-sheet" style="--penna-sheet-rest: 8px">
      <div class="penna-sheet-scrim"></div>
      <div class="penna-sheet-frame">
        <div class="penna-sheet-panel" data-penna="drawer">
          <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
          <div class="penna-sheet-header"><span class="penna-sheet-title" data-penna-text>Rename pane</span><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button></div>
          <div style="padding: 4px 16px 8px"><label class="penna-input" data-penna="field" data-size="touch-large" data-state="focused"><input value="Make the pill menu offer New chat" /></label></div>
          <div class="penna-action-bar" data-keys><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large" disabled><span class="penna-label" data-penna-text>Save</span></button></div>
        </div>
      </div>
    </div>
  </div>
</div>
```

**Confirm** asks before something is destroyed: the question as the title, one muted line on what goes, then the verb as the one main action in danger, Cancel quiet over it, and no close control. **Rename** holds the name in a 56 field, selected, with Save riding the keyboard, off until the name changes. Not built natively yet: closing a pane runs at once, and rename opens the desktop palette.

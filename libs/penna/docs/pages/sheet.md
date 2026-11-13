# Sheet

Holds a list or a page you drill into and returns you where you were: a picker, a chat's options, a browser's tabs. A short ask (confirm, rename, a few actions, one control) is a modal in the middle of the screen. To go into something, a zoom card.

## Anatomy

```figure uses=sheet,touch,button,label
<div style="position: relative; width: 390px; height: 520px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
  <div class="penna-sheet" style="--penna-sheet-rest: 8px">
    <div class="penna-sheet-scrim"></div>
    <div class="penna-sheet-frame">
      <div class="penna-sheet-panel" data-penna="drawer">
        <div class="penna-sheet-grip"><div class="penna-sheet-grabber" data-penna="grabber"></div></div>
        <div class="penna-sheet-header"><span class="penna-sheet-title" data-penna-text>Claude Code</span><button class="penna-sheet-disc" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
        <div class="penna-sheet-groups">
          <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
            <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="list-tree"></i><span class="penna-touch-row-lines"><span>Mention</span></span><i data-icon="chevron-right"></i></div>
          </div>
          <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground>
            <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="sparkle"></i><span class="penna-touch-row-lines"><span>Model</span></span><span class="penna-touch-row-value"><span>Default</span></span><i data-icon="chevron-right"></i></div>
            <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="cpu"></i><span class="penna-touch-row-lines"><span>Effort</span></span><span class="penna-touch-row-value"><span>Default</span></span><i data-icon="chevron-right"></i></div>
            <div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="hand"></i><span class="penna-touch-row-lines"><span>Permissions</span></span><span class="penna-touch-row-value"><span>Ask</span></span><i data-icon="chevron-right"></i></div>
          </div>
          <p class="penna-sheet-note">Context 0%</p>
        </div>
      </div>
    </div>
  </div>
</div>
```

1. **Panel**: the phone container corner, twice `radius`.
2. **Grabber**: 36 by 5. Drag down past half its height, or fling, to dismiss.
3. **Header**: 44, the title in `touch-small` at 600, centered on the sheet; the close disc at its end.
4. **Groups**: 16 below the header, 8 from the sides and bottom, 16 apart, their corner the panel's less 8. Rows are flush, with a hairline from the label to the end.
5. **Note**: a muted line under the last group, for the whole sheet.

## Close and back

```figure uses=sheet
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-sheet-disc" style="position: static" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button><span class="docs-note">close, 44 target</span></div>
  <div class="docs-cell"><button class="penna-sheet-disc" style="position: static" aria-label="Back"><span data-penna="button"><i data-icon="chevron-left"></i></span></button><span class="docs-note">back on a page drilled into</span></div>
</div>
```

A 30 disc in the control fill with a 12 glyph, on a 44 target. Its outer edge sits on the content edge, 16 in. A page you drilled into adds back at the start; the close stays at the end.

## A question from ssh

Titled for the machine, ssh's own prompt word for word over a 56 field, and an action that says what it does: "Sign in", not "Continue". It comes up over whatever is on screen; closing it cancels the connect. Not built: the thin client asks in a centered dialog.

## Modal

```figure uses=sheet,touch,button,label
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div style="position: relative; width: 370px; height: 340px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
    <div class="penna-modal">
      <div class="penna-modal-scrim"></div>
      <div class="penna-modal-area">
        <div class="penna-modal-card" data-penna="drawer">
          <div class="penna-modal-head"><span data-penna-text>bash</span><button class="penna-sheet-disc" aria-label="Close"><span data-penna="button"><i data-icon="xmark"></i></span></button></div>
          <div class="penna-sheet-groups" data-menu>
            <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground><div class="penna-touch-row" data-penna="phone.row" role="button"><i data-icon="pencil"></i><span class="penna-touch-row-lines"><span>Rename</span></span></div></div>
            <div class="penna-touch-group" data-penna="settings.card" data-ruled data-ground><div class="penna-touch-row" data-penna="phone.row" data-tone="danger" role="button"><i data-icon="xmark"></i><span class="penna-touch-row-lines"><span>Close window</span></span></div></div>
          </div>
        </div>
      </div>
    </div>
  </div>
  <div style="position: relative; width: 370px; height: 340px; overflow: hidden; border-radius: 28px; background: var(--penna-bg)">
    <div class="penna-modal">
      <div class="penna-modal-scrim"></div>
      <div class="penna-modal-area">
        <div class="penna-modal-card" data-penna="drawer">
          <div class="penna-modal-head"><span data-penna-text>Close this pane?</span></div>
          <p class="penna-modal-detail">The chat stops. Its transcript stays.</p>
          <div class="penna-modal-bar"><button class="penna-button" data-penna="button" data-size="touch"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="danger" data-prominent data-size="touch" data-emphasis="solid" data-solid="danger" style="color: var(--penna-on-danger)"><span class="penna-label" data-penna-text>Close</span></button></div>
        </div>
      </div>
    </div>
  </div>
</div>
```

**Action menu**: the thing's name, the close disc, its actions in a group, the one that removes in its own group 8 below. **Confirm**: the question, one muted line on what goes, then Cancel and the verb side by side, 48 tall, the verb in `danger`. **Rename**: a 48 field, then Cancel and Rename in `accent`, off until the name changes. **Control**: the label, the close, a slider with its value after it.

The card is the sheet's own surface and its groups the sheet's, so the two read as one material. It is the screen's width less 64, at most 340, padded 20, centered above the keyboard. A tap on the scrim cancels; there is no drag. In light the scrim is the ink at 28%, stronger than a sheet's, so the card stands off a busy screen. When the two buttons' labels can't share a line at large text, they stack, the verb on top.

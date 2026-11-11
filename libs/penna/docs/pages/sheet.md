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
          <label class="penna-input" data-penna="field" data-size="touch-large"><input type="password" value="hunter22" /></label>
        </div>
        <div class="penna-action-bar" data-keys><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Sign in</span></button></div>
      </div>
    </div>
  </div>
</div>
```

1. **Grabber**: 36 by 5. Drag down past half its height, or fling, to dismiss.
2. **Header**: 60, the title in `display`, the close control at its end.
3. **Body**: `inset-pane` 16, `touch-body`; rows are touch rows.
4. **Action bar**: on a phone, riding the keyboard.

## Close

```figure uses=sheet,button,notice
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-sheet-close penna-button" data-penna="button.quiet" aria-label="Close"><i data-icon="xmark"></i></button><span class="docs-note">Built: 36</span></div>
  <div class="docs-cell"><button class="penna-sheet-close penna-button" data-penna="button.quiet" data-size="touch" aria-label="Close"><i data-icon="xmark"></i></button><span class="docs-note">Phone: 44, not built</span></div>
</div>
```

Nothing tappable is under 44 on a phone, so the close grows to the touch rung.

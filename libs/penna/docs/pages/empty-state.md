# Empty state

On a phone, an empty screen says what is missing; its main action is how to get it.

## Anatomy

```figure uses=touch,button
<div style="display: flex; flex-direction: column; width: 370px; height: 460px; border-radius: 28px; background: var(--penna-panel)">
  <div style="height: 96px"></div>
  <div class="penna-empty"><i data-icon="laptop"></i><b>No machines yet</b></div>
  <div style="flex: 1"></div>
  <div class="penna-action-bar"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Add machine</span></button></div>
</div>
```

1. **Icon**: 32, muted, in the upper middle.
2. **Title**: `title`, what is missing.
3. **Line**: `touch-small`, muted, only when there is a state to say ("Waiting for ssh on fabrico@xps"), never how to use the screen.
4. **Action**: the screen's main action, in the action bar, never a button under the text.

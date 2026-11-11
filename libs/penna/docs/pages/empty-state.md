# Empty state

On a phone, an empty screen says what is missing; its main action is how to get it.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Not built natively yet</span><span class="penna-text penna-notice-detail" data-penna-text>Drawn with the HTML kit. The desktop's empty state stays as it is.</span></div></div></div>
```

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

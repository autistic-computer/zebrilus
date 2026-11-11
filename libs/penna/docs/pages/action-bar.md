# Action bar

Holds a phone screen's one main action at the bottom, where the thumb is. For moving around, the pill.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Not built natively yet</span><span class="penna-text penna-notice-detail" data-penna-text>Drawn with the HTML kit.</span></div></div></div>
```

## Layouts

```figure uses=touch,button
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px"><div class="penna-action-bar" style="width: 100%; padding-inline: 0"><button class="penna-button" data-penna="button.quiet" data-size="touch-large"><span class="penna-label" data-penna-text>Scan QR instead</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Connect</span></button></div><span class="docs-note">Secondary above</span></div>
  <div class="docs-cell" style="width: 370px"><div class="penna-action-bar" data-row style="width: 100%; padding-inline: 0"><button class="penna-button" data-penna="button" data-size="touch-large"><span class="penna-label" data-penna-text>Not now</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch-large"><span class="penna-label" data-penna-text>Add key</span></button></div><span class="docs-note">Secondary beside, main last</span></div>
</div>
```

- One main action at `touch-large` 56, full width; at most one secondary, above or beside it.
- 16 from the sides and 20 above the bottom edge: the pill's spot. A screen has the pill or the action bar, never both.
- With the keyboard up it rides on it, 8 above.
- Inside a machine the main action (New chat) is in the pill's menu, not a bar.

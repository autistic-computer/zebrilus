# Notice

An inline banner that sits inside a page or settings column to report status, permissions, or a required action without blocking the rest of the screen.

## Inline and stacked notices

`Notice` carries a semantic tone (`warning`, `danger`, `success`, `primary`, or `neutral`), an automatic tone icon, optional detail text, and an optional action button painted `.on(Notice::ground(tone, cx))`. Call `.stacked()` when the notice leads a card or status panel with a `Lead` title and action below the copy.

```figure uses=notice,chip
<div class="penna-notices" style="width: 100%">
  <div class="penna-notice" data-penna="notice" data-tone="warning" role="status">
    <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
      <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
      <div class="penna-notice-body">
        <span class="penna-notice-title" data-penna-text>Accessibility access is off</span>
        <span class="penna-notice-detail">Turn on Accessibility so the pointer can cross screen edges.</span>
      </div>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" type="button"><span class="penna-label" data-penna-text>Allow…</span></button>
    </div>
  </div>
  <div class="penna-notice" data-penna="notice" data-tone="danger" role="alert">
    <div class="penna-notice-tone" data-penna="chip" data-tone="danger">
      <span class="penna-notice-lead"><i data-icon="circle-x"></i></span>
      <div class="penna-notice-body">
        <span class="penna-notice-title" data-penna-text>Certificate expired</span>
        <span class="penna-notice-detail">Pair ubuntu again to rotate the local QUIC key.</span>
      </div>
    </div>
  </div>
  <div class="penna-notice" data-penna="notice" data-tone="success" data-stacked role="status" style="max-width: 320px">
    <div class="penna-notice-tone" data-penna="chip" data-tone="success">
      <div class="penna-notice-head"><span class="penna-dot" data-tone="success"></span><span class="penna-notice-title" data-penna-text>Ready</span></div>
      <div class="penna-notice-detail">Push past the right edge to reach ubuntu.</div>
    </div>
  </div>
</div>
```

```rust
Notice::new(Tone::Warning, "Accessibility access is off")
    .detail("Turn on Accessibility so the pointer can cross screen edges.")
    .action(
        Button::new("allow")
            .label("Allow…")
            .primary()
            .small()
            .on(Notice::ground(Tone::Warning, cx)),
    )

Notice::new(Tone::Success, "Ready")
    .dot(Tone::Success)
    .detail("Push past the right edge to reach ubuntu.")
    .stacked()
```

## Callouts and inline alerts

`blocks::feedback` also provides `callout` (toned `inline-panel`) and `inline_alert` (neutral `inline-panel` with a danger warning icon and `alert_actions` row) for inline error recovery inside a pane. New screens should prefer `Notice`.

```figure uses=feedback
<div class="docs-show" style="flex-direction: column; align-items: stretch; width: 100%; gap: 12px">
  <div class="penna-callout" data-penna="inline-panel" data-tone="warning" data-ground role="status">
    <div class="penna-callout-head">
      <span class="penna-callout-icon"><i data-icon="triangle-alert"></i></span>
      <div class="penna-callout-text">
        <div class="penna-callout-title"><span data-penna-text>Update ready</span></div>
        <div class="penna-callout-message"><span>Restart to finish installing version 0.5.</span></div>
      </div>
    </div>
    <div class="penna-alert-actions">
      <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Restart</span></button>
    </div>
  </div>
  <div class="penna-callout" data-penna="inline-panel" data-ground role="alert">
    <div class="penna-callout-head">
      <span class="penna-callout-icon"><i data-icon="triangle-alert"></i></span>
      <div class="penna-callout-text">
        <div class="penna-callout-title"><span data-penna-text>This page could not load</span></div>
        <div class="penna-callout-message"><span>The server closed the connection before sending a response.</span></div>
      </div>
    </div>
    <div class="penna-alert-actions">
      <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Try again</span></button>
    </div>
  </div>
</div>
```

## API

| Rust (`penna::Notice`, `blocks::feedback`) | HTML (`uses=notice,chip` / `uses=feedback`) | Role |
| --- | --- | --- |
| `Notice::new(tone, title)` | `.penna-notice[data-penna="notice"][data-tone]` | Toned inline banner |
| `.penna-notice-tone` | `.penna-notice-tone[data-penna="chip"][data-tone]` | Required HTML tone carrier |
| `.detail(...)` / `.icon(...)` / `.dot(tone)` | `.penna-notice-detail`, `.penna-notice-lead`, `.penna-dot` | Body text and leading mark |
| `.stacked()` | `data-stacked` + `.penna-notice-head` | Vertical lead-title layout |
| `.action(...)` + `Notice::ground(tone, cx)` | Trailing `.penna-button` | Action calibrated to notice ground |
| `feedback::callout(tone, icon, title, msg, cx)` | `.penna-callout[data-penna="inline-panel"][data-tone]` | Inline panel callout |
| `feedback::inline_alert(title, msg, cx)` | `.penna-callout[data-penna="inline-panel"]` | Neutral error recovery panel |

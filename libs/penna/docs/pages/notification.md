# Notification

Transient floating status messages stacked from the top center of the window. Notifications auto-hide after 5 seconds by default, deduplicate by key, and clamp long copy to three lines until clicked.

## Kinds and clamping

Four semantic kinds pair an icon tint with neutral text on the floating `toast` glass surface: `info`, `success`, `warning`, and `error`. Click the long message below to toggle the three-line clamp.

```figure uses=notification
<div class="docs-show" style="flex-direction: column; align-items: flex-start; width: 100%; gap: 10px">
  <div class="penna-toast" data-penna="toast" data-ground data-still role="status" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="info"><i data-icon="info"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-message"><span>Copied pane address to clipboard.</span></div>
      </div>
    </div>
  </div>
  <div class="penna-toast" data-penna="toast" data-ground data-still role="status" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="success"><i data-icon="circle-check"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-message"><span>Config reloaded cleanly.</span></div>
      </div>
    </div>
  </div>
  <div class="penna-toast" data-penna="toast" data-ground data-still role="status" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="warning"><i data-icon="triangle-alert"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-title"><span data-penna-text>Slow connection</span></div>
        <div class="penna-toast-message"><span>Round-trip time to devbox climbed above 240 ms. Streaming output may lag briefly while the QUIC path re-probes direct routes over the local network interface.</span></div>
      </div>
    </div>
  </div>
  <div class="penna-toast" data-penna="toast" data-ground data-still role="alert" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="error"><i data-icon="circle-x"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-title"><span data-penna-text>Connection lost</span></div>
        <div class="penna-toast-message"><span>ssh devbox exited with status 255.</span></div>
      </div>
    </div>
  </div>
</div>
```

```rust
window.push_notification(
    Notification::success("Config reloaded cleanly.").key("reload"),
    cx,
);

window.push_notification(
    Notification::error("ssh devbox exited with status 255.")
        .title("Connection lost")
        .autohide(false),
    cx,
);
```

## Progress and custom content

Pass `.content(...)` to embed a [Progress](progress.html) bar or trailing action buttons inside a persistent toast (`.autohide(false).closable(false)`).

```figure uses=notification,progress
<div class="docs-show" style="flex-direction: column; align-items: flex-start; width: 100%; gap: 10px">
  <div class="penna-toast" data-penna="toast" data-ground data-still role="status" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="info"><i data-icon="info"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-title"><span data-penna-text>Downloading update</span></div>
        <div class="penna-toast-message"><span>penna-0.5.0.tar.gz · 68%</span></div>
        <div style="padding-top: 6px"><div class="penna-progress" data-size="small" role="progressbar" aria-valuenow="68" aria-valuemin="0" aria-valuemax="100"><div class="penna-progress-fill" style="width: 68%"></div></div></div>
      </div>
    </div>
  </div>
  <div class="penna-toast" data-penna="toast" data-ground data-still role="status" style="width: min(100%, 340px)">
    <div class="penna-toast-body">
      <span class="penna-toast-icon" data-kind="info"><i data-icon="info"></i></span>
      <div class="penna-toast-text">
        <div class="penna-toast-title"><span data-penna-text>Update ready</span></div>
        <div class="penna-toast-message"><span>Restart to apply version 0.5.0.</span></div>
        <div class="penna-toast-actions">
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="small" type="button"><span class="penna-label" data-penna-text>Restart</span></button>
          <button class="penna-button" data-penna="button" data-size="small" type="button"><span class="penna-label" data-penna-text>Later</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

- **Deduplication & promises:** pushing a notification with the same `.key(...)` (or using `window.toast_promise(key, "Loading...", cx)` with `.resolve(...)` / `.reject(...)`) replaces the existing toast in place and resets its autohide timer.
- **Pause & dismissal:** hovering a toast pauses its autohide countdown (and `.pause_when_inactive(true)` pauses while the window is unfocused, mirrored by `document.hidden` on `[data-autohide]` in HTML); hover reveals the close button; middle-click dismisses immediately; clicking an expanded 3-line message pauses autohide until collapsed.
- **Stack limits:** `Theme::notification` controls placement (`Anchor::TopCenter`), margins, and `max_items` (default 10).

## API

| Rust (`penna::blocks::notification`) | HTML (`uses=notification`) | Role |
| --- | --- | --- |
| `Notification::{info,success,warning,error}(msg)` | `.penna-toast[data-penna="toast"][data-ground]` | Floating toast surface |
| `.title(...)` / `.message(...)` | `.penna-toast-title`, `.penna-toast-message` | Bold heading and 3-line clamped copy |
| `.key(...)` | — | Deduplicate and replace in place |
| `.autohide(bool)` / `.autohide_after(Duration)` | `[data-autohide]` | Auto-dismiss timer (default 5 s), paused on hover |
| `.pause_when_inactive(bool)` | `document.hidden` | Pause autohide countdown while the window is unfocused |
| `WindowExt::toast_promise(key, loading, cx)` | — | Loading toast handle (`ToastPromise::resolve` / `reject`) |
| `.closable(bool)` | `.penna-toast-close > button` | Hover-revealed dismiss control |
| `.content(f)` | `.penna-toast-actions` or embedded widgets | Actions or progress inside toast |
| `WindowExt::push_notification` / `push_toast` | `.penna-toast-stack` | Stack host at window top center |
| `WindowExt::dismiss_notification` | `window.pennaToast.dismiss(el)` | Dismiss a toast programmatically |

# Status dot

A semantic status indicator for live hosts, sessions, agents, and connections. Unlike [Chip](chip.html) or [Badge](badge.html), it carries no container fill and sits directly beside labels or inside tight chrome rows.

## Tones and pulse

Set `pulse()` for active work or live connections. Under `Motion::Off` or `prefers-reduced-motion`, the pulse ring hides and the dot stays still.

```figure uses=status_dot
<div class="docs-show" data-gap="wide">
  <span class="penna-status-dot" data-tone="success" data-pulse role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Online</span></span>
  <span class="penna-status-dot" data-tone="accent" data-pulse role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Streaming</span></span>
  <span class="penna-status-dot" data-tone="warning" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Reconnecting</span></span>
  <span class="penna-status-dot" data-tone="danger" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Failed</span></span>
  <span class="penna-status-dot" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Idle</span></span>
</div>
```

## Sizes

```figure uses=status_dot
<div class="docs-show" data-gap="wide">
  <span class="penna-status-dot" data-size="small" data-tone="success" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Small · 6 px</span></span>
  <span class="penna-status-dot" data-tone="success" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Medium · 8 px</span></span>
  <span class="penna-status-dot" data-size="large" data-tone="success" role="status"><span class="penna-status-dot-mark" aria-hidden="true"></span><span>Large · 10 px</span></span>
</div>
```

```rust
StatusDot::new("host-status", Tone::Success)
    .label("Online")
    .pulse()
```

## API (`penna::StatusDot`)

| Rust | HTML (`uses=status_dot`) | Role |
| --- | --- | --- |
| `StatusDot::new(id, tone)` | `.penna-status-dot[data-tone]` | Status indicator with `Role::Status` |
| `.label(text)` | Trailing `<span>` | Optional label next to the dot |
| `.pulse()` | `data-pulse` | Animated pulse ring (disabled under reduced motion) |
| `.small()` / `.medium()` / `.large()` | `data-size` (`small` \| `large`) | 6 px, 8 px, or 10 px dot diameter |
| `.on(ground)` | `data-ground` ancestor | Contrast-safe neutral tone on nested surfaces |

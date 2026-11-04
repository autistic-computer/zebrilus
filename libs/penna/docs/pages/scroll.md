# Scrollbar

Shows where a scroll area is and drags through it. It overlays the edge and takes no space.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>Its widths are fixed pixels that don't follow the interface size. On the web a bar with no show mode never hides.</span></div></div></div>
```

## Modes

- **Scrolling**: hidden until the area scrolls. The default.
- **Hover**: shows under the pointer, and while scrolling.
- **Always**: the thumb stays.

The theme picks Scrolling or Hover from the system setting.

```rust
v_flex().overflow_y_scrollbar().children(rows)
```

## States

```figure uses=scroll
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="scrolling" style="width: 176px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: var(--penna-radius); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div><div>server.rs</div><div>client.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-visible><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Scrolling</span></div>
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="hover" style="width: 176px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: var(--penna-radius); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div><div>server.rs</div><div>client.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-state="hover"><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="hover" style="width: 176px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: var(--penna-radius); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div><div>server.rs</div><div>client.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-state="active"><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Dragging</span></div>
</div>
```

- Scrolling: a thin thumb, gone 3s after the last scroll.
- Hover: widens. On the thumb or dragging, its fill gets stronger.
- Reduced motion: no fade.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.overflow_y_scrollbar()` | `.penna-scroll`, `.penna-scroll-viewport` | Scroll area with its bar |
| `.vertical_scrollbar(&handle)` | `.penna-scrollbar` | Bar over an area you scroll |
| `Scrollbar::vertical(&handle)` | `.penna-scrollbar` | The bar on its own |
| `.axis(ScrollbarAxis::Both)` | `data-axis` | Vertical, horizontal or both |
| `.scrollbar_show(ScrollbarShow::Hover)` | `data-show` | Mode; the theme's by default |
| `.id(id)` | `data-bar` | Names the bars |

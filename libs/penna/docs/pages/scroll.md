# Scrollbar

Shows where a scroll area is and drags through it. It overlays the edge and takes no space.

## Modes

- **Scrolling**: hidden until the area scrolls. The default.
- **Hover**: shows under the pointer, and while scrolling.
- **Always**: the thumb stays.

The theme picks Scrolling or Hover from the system setting. On the web a bar with no `data-show` is Scrolling.

```rust
v_flex().overflow_y_scrollbar().children(rows)
```

## Sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Part</span><span class="docs-head">Token</span><span class="docs-head">Length</span><span class="docs-head">Example</span>
  <span>Track</span><span><code>space-16</code></span><span><code>16</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"></span></span>
  <span>Thumb</span><span><code>space-8</code></span><span><code>8</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-8); width: calc(var(--penna-space-16) * 4); border-radius: min(var(--penna-radius), var(--penna-space-8) / 2); background: var(--penna-thumb)"></span></span></span>
  <span>Thumb at rest, Scrolling</span><span><code>space-6</code></span><span><code>6</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-6); width: calc(var(--penna-space-16) * 4); border-radius: min(var(--penna-radius), var(--penna-space-6) / 2); background: var(--penna-thumb)"></span></span></span>
  <span>Shortest thumb</span><span><code>3 × track</code></span><span><code>48</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-8); width: calc(var(--penna-space-16) * 3); border-radius: min(var(--penna-radius), var(--penna-space-8) / 2); background: var(--penna-thumb)"></span></span></span>
</div>
```

The thumb sits `space-2` (2) in from the edge and the track's ends. Every length grows with the interface font.

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

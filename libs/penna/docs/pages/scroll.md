# Scrollbar

Shows where a scroll area is and drags through it. It overlays the edge and takes no layout space.

## Modes and axes

- **Scrolling**: hidden until the area scrolls. The default.
- **Hover**: shows under the pointer and while scrolling.
- **Always**: the thumb stays visible.

The theme picks Scrolling or Hover from the system setting. A bar supports vertical, horizontal or both axes (the horizontal track stops one track width short of the corner) and hides whenever content fits.

```rust
v_flex().overflow_y_scrollbar().children(rows)
```

## Anatomy and sizes

```figure
<div class="docs-tokens">
  <span class="docs-head">Part</span><span class="docs-head">Token</span><span class="docs-head">Length</span><span class="docs-head">Example</span>
  <span>Track</span><span><code>space-16</code></span><span><code>16</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"></span></span>
  <span>Thumb</span><span><code>space-8</code></span><span><code>8</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-8); width: calc(var(--penna-space-16) * 4); border-radius: min(var(--penna-radius), var(--penna-space-8) / 2); background: var(--penna-thumb)"></span></span></span>
  <span>Thumb at rest, Scrolling</span><span><code>space-6</code></span><span><code>6</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-6); width: calc(var(--penna-space-16) * 4); border-radius: min(var(--penna-radius), var(--penna-space-6) / 2); background: var(--penna-thumb)"></span></span></span>
  <span>Shortest thumb</span><span><code>3 × track</code></span><span><code>48</code></span><span><span style="position: relative; display: block; width: calc(var(--penna-space-16) * 5); height: var(--penna-space-16); box-shadow: inset 0 0 0 1px var(--penna-divider)"><span style="position: absolute; bottom: var(--penna-space-2); left: var(--penna-space-2); height: var(--penna-space-8); width: calc(var(--penna-space-16) * 3); border-radius: min(var(--penna-radius), var(--penna-space-8) / 2); background: var(--penna-thumb)"></span></span></span>
</div>
```

The thumb sits `space-2` in from the edge and the track's ends, painted at 28% (`--penna-thumb`) and 35% on hover or drag. Every length grows with the interface font. Edge fades (`.top_fade` / `.bottom_fade`) soften scrolled edges (32 in settings, 40 in sidebars) with a 70% solid stop capped at twice the overflow.

## States

```figure uses=scroll
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="scrolling" style="width: 152px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-visible><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Scrolling</span></div>
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="hover" style="width: 152px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-state="hover"><div class="penna-scroll-thumb" data-state="hover"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="hover" style="width: 152px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical" data-state="active"><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Dragging</span></div>
  <div class="docs-cell"><div class="penna-scroll" data-axis="vertical" data-show="always" style="width: 152px; height: 112px; box-sizing: border-box; border: 1px solid var(--penna-border); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k))"><div class="penna-scroll-viewport"><div style="padding: var(--penna-space-8) var(--penna-space-12); color: var(--penna-fg-muted); font-size: var(--penna-text-small-size); line-height: var(--penna-text-small-line)"><div>main.rs</div><div>lib.rs</div><div>pane.rs</div><div>session.rs</div><div>window.rs</div><div>layout.rs</div><div>keys.rs</div><div>theme.rs</div><div>status.rs</div><div>copy.rs</div></div></div><div class="penna-scrollbar" data-axis="vertical"><div class="penna-scroll-thumb"><div class="penna-scroll-fill"></div></div></div></div><span class="docs-note">Always</span></div>
</div>
```

- **Scrolling**: thin 6 px thumb, held 2s after the last scroll then fades out over 1s (no fade under reduced motion). Hidden tracks let pointer events pass through.
- **Hover and drag**: track hover widens to 8 px when visible or in Hover mode; thumb hover and dragging strengthen the fill. Dragging moves proportionally, clicking the track jumps the thumb center there, and wheel events scroll through the track.
- **Accessibility**: announces `role="scrollbar"`, orientation, and 0–100 position; keyboard scrolling belongs to the scroll area.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.overflow_y_scrollbar()`, `_x_`, `_scrollbar()` | `.penna-scroll`, `.penna-scroll-viewport` | Scrollable wrapper with bars (`ScrollableElement`) |
| `.vertical_scrollbar(&handle)`, `.horizontal_scrollbar(&handle)` | `.penna-scrollbar` | Bar overlay over a caller-managed handle |
| `.top_fade(&h, ground, height)`, `.bottom_fade(&h, ground, height)` | `[data-fade-top]`, `[data-fade-bottom]` | Soft vertical edge fade on a scrolling region |
| `.left_fade(&h, ground, width)`, `.right_fade(&h, ground, width)` | `[data-fade-left]`, `[data-fade-right]` | Soft horizontal edge fade on a scrolling region |
| `Scrollbar::new(&handle)`, `::vertical(&handle)`, `::horizontal(&handle)` | `pennaScroll.mount(el)` | Standalone scrollbar overlay |
| `Scrollbar::axis(ScrollbarAxis::Both)` | `data-axis` (`vertical`, `horizontal`, `both`) | Active axes |
| `Scrollbar::scrollbar_show(ScrollbarShow::Hover)` | `data-show` (`scrolling`, `hover`, `always`) | Visibility mode; theme's by default |
| `Scrollbar::id(id)` | `data-bar`, `data-scroll-x`, `data-scroll-y`, `data-hovered` | Author id and initial scroll offsets |

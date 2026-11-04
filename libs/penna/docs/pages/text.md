# Text

Markdown for reading: documents, help and chat. Larger and more open than interface text.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>The quote bar and table column limits are fixed pixels. Streaming text fades for up to 400ms, and not at all on the web. The web version can't take its container's text style.</span></div></div></div>
```

## Reading ladder

```figure uses=label
<div class="docs-tokens">
  <span class="docs-head">Style</span><span class="docs-head">Size / line</span><span class="docs-head">Tag</span><span class="docs-head">Sample</span>
  <span><code>read-h1</code></span><span><code>26 / 32</code></span><span>h1</span><span class="penna-type" data-type="read-h1" data-penna-text>Sessions</span>
  <span><code>read-h2</code></span><span><code>20 / 28</code></span><span>h2</span><span class="penna-type" data-type="read-h2" data-penna-text>Windows</span>
  <span><code>read-h3</code></span><span><code>16 / 24</code></span><span>h3</span><span class="penna-type" data-type="read-h3" data-penna-text>Panes</span>
  <span><code>read-h4</code></span><span><code>14 / 22</code></span><span>h4</span><span class="penna-type" data-type="read-h4" data-penna-text>Keys</span>
  <span><code>read-h5</code></span><span><code>13 / 20</code></span><span>h5, h6</span><span class="penna-type" data-type="read-h5" data-penna-text>Copy mode</span>
  <span><code>read-body</code></span><span><code>14 / 22</code></span><span>p</span><span class="penna-type" data-type="read-body" data-penna-text>The server owns every process.</span>
</div>
```

Headings are 600, body 400, bold 600. Lines stop at 640.

```rust
TextView::markdown("notes", source).selectable(true)
```

## Rhythm

Blocks sit 12 apart. Headings have 24 above at h1 and h2, 16 above at h3 to h6, and 8 below.

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-md" style="width: 280px"><h2>Windows</h2><p>A window holds panes side by side.</p><p>Close the last pane to close it.</p></div><span class="docs-note">h2, then two paragraphs</span></div>
  <div class="docs-cell"><div class="penna-md" style="width: 280px"><p>The server keeps every pane alive.</p><h3>Panes</h3><p>Each pane runs one process.</p></div><span class="docs-note">h3 after a paragraph</span></div>
</div>
```

## Code

Code takes the reading size in the code face. Blocks sit on a card.

```figure
<div class="penna-md docs-full"><p>Run <code data-penna="card.code">cargo run</code> to start the server.</p><div class="md-code"><pre data-penna="card.code"><code><span class="tk-keyword">let</span> pane = session.<span class="tk-function">active_pane</span>();</code></pre></div></div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `TextView::markdown(id, text)` | `.penna-md` | Text set once |
| `TextView::new(&state)` | `.penna-md` | Text that changes |
| `.selectable(true)` | `data-selectable` | Select and copy |
| `.streaming(true)` | `data-streaming` | Text still arriving |
| `.scrollable(true)` | none | Fills its container and scrolls |
| `.code_block_actions(f)` | `data-actions`, `.md-actions` | Buttons on code blocks |
| `.links(handler)` | `a` | Handles link clicks |
| `.text(None)` on `TextViewStyle` | none | Takes its container's text style, no measure |

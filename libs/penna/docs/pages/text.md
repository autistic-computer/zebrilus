# Text

Markdown for reading: documents, help and chat. Larger and more open than interface text.

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

Code takes the reading size in the code face. Blocks sit on a card. With the `tree-sitter` feature, fences in Rust, Python, JavaScript, TypeScript, TSX, shell, Go, C, CSS, HTML, JSON, TOML, YAML, Markdown and diff are colored; a diff's added and removed lines take the success and danger inks.

```figure
<div class="penna-md docs-full"><p>Run <code data-penna="card.code">cargo run</code> to start the server.</p><div class="md-code"><pre data-penna="card.code"><code><span class="tk-keyword">let</span> pane = session.<span class="tk-function">active_pane</span>();</code></pre></div></div>
```

## Quotes and tables

A quote has a 4 bar and 16 inside it. Table columns size to their content from 48 to 480 and share the width; a scrolling table keeps those widths and scrolls sideways.

```figure
<div class="penna-md" style="width: 320px"><div class="md-quote"><blockquote><p>The server outlives the app.</p></blockquote></div><div class="md-table"><div class="md-figure" data-penna="card.figure"><div class="md-row"><div class="md-cell" data-align="left">Command</div><div class="md-cell" data-align="center">Key</div></div><div class="md-row"><div class="md-cell" data-align="left">Split right</div><div class="md-cell" data-align="center"><code data-penna="card.code">Ctrl+D</code></div></div></div></div></div>
```

## Footnotes

A reference is a superscript number. The notes gather after the last block, numbered in caption size.

```figure
<div class="penna-md" style="width: 320px"><p>The server outlives the app.¹</p><div class="md-footnotes"><div class="md-footnote"><span>1.</span><p>Until the machine restarts.</p></div></div></div>
```

## Streaming

New text fades in over 120 to 220ms, at the pace it arrives. With reduced motion it shows at once.

```figure
<div class="penna-md" data-streaming style="width: 320px"><p>Splitting the pane now.</p><template data-tail><p>The new pane keeps the working directory.</p></template></div>
```

## Inherited

`text(None)` takes the text style of its container and drops the measure. Headings keep the reading ladder.

```figure
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-md" data-text="inherit" style="width: 280px"><h4>Split the pane</h4><p>The new pane keeps the working directory, so the build stays warm.</p></div><span class="docs-note">In a body container, 13 on 18</span></div>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `TextView::markdown(id, text)` | `.penna-md` | Text set once |
| `TextView::new(&state)` | `.penna-md` | Text that changes |
| `.selectable(true)` | `data-selectable` | Select and copy |
| `.streaming(true)` | `data-streaming`, `template[data-tail]` | Text still arriving |
| `.scrollable(true)` | none | Fills its container and scrolls |
| `.code_block_actions(f)` | `data-actions`, `.md-actions` | Buttons on code blocks |
| `.links(handler)` | `a` | Handles link clicks |
| `.text(None)` on `TextViewStyle` | `data-text="inherit"` | Takes its container's text style, no measure |
| `.table(style)` on `TextViewStyle` | `data-scroll`, `.md-scroll` | `overflow_x` scroll: tables scroll sideways |
